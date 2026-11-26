//! A projected repository's `.git` switching from the Mac to the machine's own fetch (RFD 0007,
//! seed bundles: "a repo with a public remote should still take its objects from the remote
//! (C) ... The open part is the swap: serving `.git` by projection until the machine's own fetch
//! lands, then switching to it").
//!
//! At session start `.git` is the projection's, its bytes from the Mac on first read. Beside it,
//! [`prepare`] builds the machine's own repository on its disk (`<state>/git-local`):
//! 1. a blobless fetch (`--filter=blob:none`) from the repository's public remote of the branches
//!    the Mac tracks there: history and trees from the remote, blobs on first use, from the remote
//!    too (a `cmux-seed` promisor remote, so the Mac's own remote settings stay as they are);
//! 2. a fetch from the projected `.git` itself of every ref (`+refs/*:refs/*`), negotiated against
//!    what step 1 brought: only what the remote lacks crosses the Mac's link, which is the
//!    unpushed commits, their trees and blobs, and the stash;
//! 3. the Mac's `HEAD`, `index`, `config` (with the promisor remote added) and `info/` copied as
//!    they read through the mount, the remote's refs of step 1 deleted.
//!
//! [`swap`] then puts a link to it at `.git` in the upper layer in one rename
//! ([`crate::overlay::Overlay::swap_to_link`]) and has the kernel forget the old entry. The two
//! repositories hold the same refs, HEAD and index, and the new one every object the old one
//! reached, so a git process that read part of its state before the switch and part after
//! still sees one repository. The Mac's state is fingerprinted ([`fingerprint`]) before step 2
//! and again just before the rename; a change between (a commit on the machine meanwhile) runs
//! steps 2 and 3 again.
//!
//! A repository is left on projection when its remote cannot be read without credentials
//! (`GIT_TERMINAL_PROMPT=0`), when it is shallow, has submodules' repositories in
//! `.git/modules`, or is mid-merge, rebase, cherry-pick or bisect.
//!
//! Every repository of the folder swaps, the folder's own and the ones in folders below it
//! ([`repositories`]: a folder of clones, issue 57201, where `git` in a nested clone of 17 GB read
//! its packs over the Mac's link). On by default ([`enabled`]); `CMUX_PROJECTION_GIT_SWAP=0` keeps
//! every `.git` on projection.
//!
//! Bring Back after a swap ([`export_for_back`]): the machine's repository is not in the upper
//! layer, so the listing Bring Back reads shows, under the swapped `.git`, what its git leg reads
//! from an upper `.git`: `HEAD`, `index`, the branch's ref as a loose file, and one pack of the
//! objects the Mac lacks (reachable from the machine's refs and index, not from the refs the Mac
//! had at the swap, [`TIPS`]). The upper layer's mark folds in the repository's refs, HEAD and
//! index ([`print`]), so a commit on the machine after Bring Back read it keeps the projection.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use cmux_process::{Command, Stdio};

/// The promisor remote the machine's repository fetches blobs from.
pub const SEED_REMOTE: &str = "cmux-seed";
/// Where the remote's refs land during [`prepare`] (deleted at its end).
const SEED_REFS: &str = "refs/cmux-seed/";
/// The machine's repository, in the projection's state folder.
pub const LOCAL_DIR: &str = "git-local";
/// The Mac's refs as they were copied at the swap (one object id a line, `HEAD` first): what
/// Bring Back's pack leaves out ([`export_for_back`]).
pub const TIPS: &str = "cmux-mac-tips";
/// How many times a moving Mac state is copied again before the swap gives up for now.
const ATTEMPTS: usize = 3;
/// How deep below the projected folder [`repositories`] looks for a `.git`.
const MAX_DEPTH: usize = 3;
/// The most repositories one projection swaps (each is a fetch from its remote).
const MAX_REPOSITORIES: usize = 16;
/// Folders [`repositories`] never looks in: build output and package caches, never a clone.
const NOT_CLONES: [&str; 6] = ["node_modules", "target", ".build", "DerivedData", "Pods", "vendor"];
/// What in `.git` leaves the repository on projection: git is mid-operation, or the state is one
/// the copy does not carry.
const UNSWAPPABLE: [&str; 9] = [
    "shallow",
    "modules",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
    "rebase-merge",
    "rebase-apply",
    "sequencer",
];

fn git(git_dir: &Path) -> Command {
    let mut command = Command::new("git");
    // `GIT_DIR`, not `--git-dir`: a `git` wrapper on PATH (a machine's own) that reads only
    // the command line sees a plain command.
    command.env("GIT_DIR", git_dir);
    // A private remote fails at once instead of asking for a password nobody types.
    command.env("GIT_TERMINAL_PROMPT", "0").env("GIT_ASKPASS", "true").env("SSH_ASKPASS", "true");
    command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    command
}

fn output(mut command: Command) -> anyhow::Result<Vec<u8>> {
    let out = command.stdin(Stdio::null()).stderr(Stdio::piped()).output()?;
    anyhow::ensure!(
        out.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}

fn text(command: Command) -> anyhow::Result<String> {
    Ok(String::from_utf8(output(command)?)?)
}

/// Runs `command` with `lines` on its stdin.
fn with_stdin(mut command: Command, lines: &[String]) -> anyhow::Result<()> {
    let mut child =
        command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        for line in lines {
            writeln!(stdin, "{line}")?;
        }
    }
    let out = child.wait_with_output()?;
    anyhow::ensure!(
        out.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

/// Whether projections swap their repositories (module docs): on unless
/// `CMUX_PROJECTION_GIT_SWAP` is `0`, `false` or `off`.
#[must_use]
pub fn enabled() -> bool {
    enabled_by(std::env::var("CMUX_PROJECTION_GIT_SWAP").ok().as_deref())
}

fn enabled_by(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "false" | "off" | "no"))
}

/// The repositories of the folder at `at`, as paths relative to it (`""` for the folder itself):
/// every folder holding a `.git` folder, at most [`MAX_DEPTH`] folders down, shallowest first.
/// Never below a repository (its work tree is its own), never in a hidden folder or one of
/// [`NOT_CLONES`]; at most [`MAX_REPOSITORIES`]. Reads listings only (a projection answers them
/// from its manifest, no byte from the Mac).
#[must_use]
pub fn repositories(at: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut level = vec![String::new()];
    for depth in 0..=MAX_DEPTH {
        let mut next = Vec::new();
        for relative in level {
            let dir = if relative.is_empty() { at.to_path_buf() } else { at.join(&relative) };
            if std::fs::symlink_metadata(dir.join(".git")).is_ok_and(|meta| meta.is_dir()) {
                found.push(relative);
                if found.len() == MAX_REPOSITORIES {
                    return found;
                }
                continue;
            }
            if depth == MAX_DEPTH {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| !name.starts_with('.') && !NOT_CLONES.contains(&name.as_str()))
                .collect();
            names.sort();
            next.extend(
                names.into_iter().map(|name| {
                    if relative.is_empty() { name } else { format!("{relative}/{name}") }
                }),
            );
        }
        level = next;
    }
    found
}

/// Where the machine's repository for `repo` (relative to the folder, `""` for the folder itself)
/// lives in the projection's state folder: [`LOCAL_DIR`] for the folder's own, beside it for one
/// below (its name a hash of the path, so any path fits one name).
#[must_use]
pub fn local_dir(state: &Path, repo: &str) -> PathBuf {
    if repo.is_empty() {
        return state.join(LOCAL_DIR);
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in repo.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    state.join(format!("{LOCAL_DIR}-{hash:016x}"))
}

/// Whether `target` (a link's text in the upper layer) is one of the machine's repositories of
/// the projection whose state folder is `state` ([`local_dir`]).
#[must_use]
pub fn is_local_dir(state: &Path, target: &Path) -> bool {
    target.parent() == Some(state)
        && target.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
            name == LOCAL_DIR
                || name.strip_prefix(LOCAL_DIR).and_then(|rest| rest.strip_prefix('-')).is_some_and(
                    |hash| hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
                )
        })
}

/// The URL a public remote is fetched from anonymously: `https://` and `file://` as they are
/// (user info cut), a GitHub SSH form as its `https://` one; anything else is none.
#[must_use]
pub fn anonymous_url(url: &str) -> Option<String> {
    let url = url.trim();
    for prefix in ["git@github.com:", "ssh://git@github.com/"] {
        if let Some(rest) = url.strip_prefix(prefix) {
            return Some(format!("https://github.com/{rest}"));
        }
    }
    if let Some(rest) = url.strip_prefix("https://") {
        let rest = match rest.split_once('/') {
            Some((host, path)) => {
                let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
                format!("{host}/{path}")
            }
            None => rest.to_owned(),
        };
        return Some(format!("https://{rest}"));
    }
    url.starts_with("file://").then(|| url.to_owned())
}

/// The remote the repository at `git_dir` takes its objects from, and its anonymous URL: the
/// current branch's upstream remote, else `origin`.
pub fn seed_remote(git_dir: &Path) -> Option<(String, String)> {
    let config = git_dir.join("config");
    let get = |key: &str| {
        let mut command = Command::new("git");
        command.arg("config").arg("--file").arg(&config).args(["--get", key]);
        text(command).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let branch = head.trim().strip_prefix("ref: refs/heads/").map(str::to_owned);
    let remote = branch
        .and_then(|branch| get(&format!("branch.{branch}.remote")))
        .filter(|remote| remote != ".")
        .unwrap_or_else(|| "origin".to_owned());
    let url = anonymous_url(&get(&format!("remote.{remote}.url"))?)?;
    Some((remote, url))
}

/// What must match between the copy and the swap: HEAD, every ref, and the index's size and
/// modification time. `None` while git is mid-operation there (a lock file, a merge).
pub fn fingerprint(git_dir: &Path) -> anyhow::Result<Option<String>> {
    for name in UNSWAPPABLE {
        if std::fs::symlink_metadata(git_dir.join(name)).is_ok() {
            return Ok(None);
        }
    }
    for lock in ["index.lock", "HEAD.lock", "packed-refs.lock"] {
        if std::fs::symlink_metadata(git_dir.join(lock)).is_ok() {
            return Ok(None);
        }
    }
    let head = std::fs::read_to_string(git_dir.join("HEAD"))?;
    let mut refs = git(git_dir);
    refs.args(["for-each-ref", "--format=%(objectname) %(refname)"]);
    let refs = text(refs)?;
    let index = match std::fs::metadata(git_dir.join("index")) {
        Ok(meta) => {
            use std::os::unix::fs::MetadataExt as _;
            format!("{} {}.{}", meta.len(), meta.mtime(), meta.mtime_nsec())
        }
        Err(_) => "none".to_owned(),
    };
    Ok(Some(format!("{head}\n{index}\n{refs}")))
}

/// The machine's repository at `local`, made from `projected` (the `.git` as the mount serves
/// it) and the public `url`, as the module doc says. Returns the fingerprint it matches.
pub fn prepare(projected: &Path, local: &Path, url: &str) -> anyhow::Result<String> {
    let Some(before) = fingerprint(projected)? else {
        anyhow::bail!("git is busy in {} (a lock or an operation in progress)", projected.display())
    };
    if local.exists() {
        std::fs::remove_dir_all(local)?;
    }
    let mut init = Command::new("git");
    init.args(["init", "--bare", "--quiet"]).arg(local);
    output(init)?;
    for (key, value) in [
        ("core.repositoryformatversion", "1"),
        ("extensions.partialClone", SEED_REMOTE),
        (&format!("remote.{SEED_REMOTE}.url"), url),
        (&format!("remote.{SEED_REMOTE}.promisor"), "true"),
        (&format!("remote.{SEED_REMOTE}.partialclonefilter"), "blob:none"),
    ] {
        let mut set = git(local);
        set.args(["config", key, value]);
        output(set)?;
    }
    fetch_remote(projected, local)?;
    let mut fingerprint_now = before;
    for _ in 0..ATTEMPTS {
        copy_mac_state(projected, local)?;
        match fingerprint(projected)? {
            Some(after) if after == fingerprint_now => {
                write_tips(local)?;
                return Ok(after);
            }
            Some(after) => fingerprint_now = after,
            None => {}
        }
    }
    anyhow::bail!("the repository kept changing while it was copied")
}

/// Step 1: the branches the Mac tracks on the remote (that the remote still has), blobless.
fn fetch_remote(projected: &Path, local: &Path) -> anyhow::Result<()> {
    let mut remote = git(local);
    remote.args(["ls-remote", "--symref", SEED_REMOTE, "HEAD", "refs/heads/*"]);
    let listed = text(remote)?;
    let mut mac = git(projected);
    mac.args(["for-each-ref", "--format=%(refname)", "refs/remotes/"]);
    let tracked: std::collections::BTreeSet<String> = text(mac)?
        .lines()
        .filter_map(|name| name.strip_prefix("refs/remotes/"))
        .filter_map(|rest| rest.split_once('/').map(|(_, branch)| branch.to_owned()))
        .collect();
    let mut wanted = Vec::new();
    let mut head = None;
    for line in listed.lines() {
        if let Some(rest) = line.strip_prefix("ref: refs/heads/") {
            head = rest.split_whitespace().next().map(str::to_owned);
            continue;
        }
        let Some((_, name)) = line.split_once('\t') else { continue };
        let Some(branch) = name.strip_prefix("refs/heads/") else { continue };
        if tracked.contains(branch) {
            wanted.push(branch.to_owned());
        }
    }
    if wanted.is_empty() {
        wanted.extend(head);
    }
    if wanted.is_empty() {
        return Ok(());
    }
    let specs: Vec<String> =
        wanted.iter().map(|branch| format!("+refs/heads/{branch}:{SEED_REFS}{branch}")).collect();
    let mut fetch = git(local);
    fetch.args(["fetch", "--quiet", "--no-tags", "--no-write-fetch-head", "--filter=blob:none"]);
    fetch.args(["--recurse-submodules=no", "--stdin", SEED_REMOTE]);
    with_stdin(fetch, &specs)
}

/// Steps 2 and 3: every ref and the objects the remote lacks from the projected `.git`, then
/// its HEAD, index, config and info.
fn copy_mac_state(projected: &Path, local: &Path) -> anyhow::Result<()> {
    let mut fetch = git(local);
    fetch.args(["fetch", "--quiet", "--no-tags", "--no-write-fetch-head", "--update-head-ok"]);
    // No `--prune`: git prunes before it negotiates, which would drop the seed refs and with
    // them every `have`, and the Mac would send its whole history. Pruned below instead.
    fetch.args(["--recurse-submodules=no"]);
    // Negotiation only from what is already here, never a lazy blob fetch to check one.
    fetch.env("GIT_NO_LAZY_FETCH", "1");
    fetch.arg(projected).arg("+refs/*:refs/*");
    output(fetch)?;
    let mut mac = git(projected);
    mac.args(["for-each-ref", "--format=%(refname)"]);
    let on_mac: std::collections::HashSet<String> = text(mac)?.lines().map(str::to_owned).collect();
    let mut list = git(local);
    list.args(["for-each-ref", "--format=%(refname)"]);
    // The seed refs, and refs the Mac deleted since an earlier attempt.
    let deletes: Vec<String> = text(list)?
        .lines()
        .filter(|name| !on_mac.contains(*name))
        .map(|name| format!("delete {name}"))
        .collect();
    if !deletes.is_empty() {
        let mut update = git(local);
        update.args(["update-ref", "--stdin"]);
        with_stdin(update, &deletes)?;
    }
    let config = std::fs::read_to_string(projected.join("config")).unwrap_or_default();
    let lead = if config.is_empty() || config.ends_with('\n') { "" } else { "\n" };
    let ours = format!(
        "{lead}[core]\n\tbare = false\n\trepositoryformatversion = 1\n\tcheckStat = minimal\n\t\
         trustctime = false\n[extensions]\n\tpartialClone = {SEED_REMOTE}\n[remote \"{SEED_REMOTE}\"]\n\t\
         url = {url}\n\tpromisor = true\n\tpartialclonefilter = blob:none\n",
        url = seed_url(local)?,
    );
    write_atomic(&local.join("config"), format!("{config}{ours}").as_bytes())?;
    let info = projected.join("info");
    if let Ok(entries) = std::fs::read_dir(&info) {
        std::fs::create_dir_all(local.join("info"))?;
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                let bytes = std::fs::read(entry.path())?;
                write_atomic(&local.join("info").join(entry.file_name()), &bytes)?;
            }
        }
    }
    match std::fs::read(projected.join("index")) {
        Ok(bytes) => write_atomic(&local.join("index"), &bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = std::fs::remove_file(local.join("index"));
        }
        Err(error) => return Err(error.into()),
    }
    write_atomic(&local.join("HEAD"), &std::fs::read(projected.join("HEAD"))?)?;
    Ok(())
}

fn seed_url(local: &Path) -> anyhow::Result<String> {
    let mut get = git(local);
    get.args(["config", "--get", &format!("remote.{SEED_REMOTE}.url")]);
    Ok(text(get)?.trim().to_owned())
}

fn write_atomic(file: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temp = file.with_extension("cmux-swap.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, file)?;
    Ok(())
}

/// Keeps the Mac's refs as the copy has them ([`TIPS`]): `HEAD`'s commit, then every ref's
/// object.
fn write_tips(local: &Path) -> anyhow::Result<()> {
    let mut list = git(local);
    list.args(["for-each-ref", "--format=%(objectname)"]);
    let mut tips: Vec<String> = Vec::new();
    let mut head = git(local);
    head.args(["rev-parse", "--verify", "--quiet", "HEAD"]);
    if let Ok(id) = text(head) {
        tips.push(id.trim().to_owned());
    }
    for id in text(list)?.lines().map(str::trim).filter(|id| !id.is_empty()) {
        if !tips.iter().any(|tip| tip == id) {
            tips.push(id.to_owned());
        }
    }
    let mut body = tips.join("\n");
    body.push('\n');
    write_atomic(&local.join(TIPS), body.as_bytes())
}

/// Every ref of the repository at `git_dir` and its value, loose files over `packed-refs` as git
/// reads them, by reading files only (no `git`): a ref written loose with the value it had packed
/// is the same map.
fn refs_of(git_dir: &Path) -> std::collections::BTreeMap<String, String> {
    let mut refs = std::collections::BTreeMap::new();
    if let Ok(packed) = std::fs::read_to_string(git_dir.join("packed-refs")) {
        for line in packed.lines() {
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            if let Some((id, name)) = line.split_once(' ') {
                refs.insert(name.to_owned(), id.to_owned());
            }
        }
    }
    let mut stack = vec![(git_dir.join("refs"), "refs".to_owned())];
    while let Some((dir, name)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let Ok(child) = entry.file_name().into_string() else { continue };
            if child.ends_with(".lock") {
                continue;
            }
            let full = format!("{name}/{child}");
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push((entry.path(), full)),
                Ok(kind) if kind.is_file() => {
                    if let Ok(value) = std::fs::read_to_string(entry.path()) {
                        refs.insert(full, value.trim().to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    refs
}

/// What changes when git changes the machine's repository at `local`: `HEAD`, every ref's value
/// and the index's size and modification time. Read from files only, so the upper layer's mark
/// ([`crate::state::mark`]) stays cheap; [`export_for_back`] leaves it as it was.
#[must_use]
pub fn print(local: &Path) -> Vec<u8> {
    let mut out = std::fs::read(local.join("HEAD")).unwrap_or_default();
    out.push(0);
    if let Ok(meta) = std::fs::metadata(local.join("index")) {
        use std::os::unix::fs::MetadataExt as _;
        out.extend_from_slice(
            format!("{} {}.{}", meta.len(), meta.mtime(), meta.mtime_nsec()).as_bytes(),
        );
    }
    out.push(0);
    for (name, value) in refs_of(local) {
        out.extend_from_slice(name.as_bytes());
        out.push(b' ');
        out.extend_from_slice(value.as_bytes());
        out.push(b'\n');
    }
    out
}

/// What Bring Back reads of the machine's repository at `local` (module docs): paths relative to
/// it, every one a file there. `HEAD` and `index` as git keeps them; `HEAD`'s branch written loose
/// with the value it has (git's own lock, so a commit meanwhile is never undone) when only
/// `packed-refs` holds it, as the Mac reads a branch loose first; and one pack in
/// `objects/pack` of what the machine has that the Mac had not at the swap, none when that is
/// nothing. A repository swapped before [`TIPS`] existed leaves out what its remote-tracking refs
/// reach instead.
pub fn export_for_back(local: &Path) -> anyhow::Result<Vec<String>> {
    let mut paths = vec!["HEAD".to_owned()];
    if local.join("index").is_file() {
        paths.push("index".to_owned());
    }
    let head = std::fs::read_to_string(local.join("HEAD"))?;
    if let Some(branch) = head.trim().strip_prefix("ref:").map(str::trim) {
        if branch.starts_with("refs/heads/") {
            if loose_ref(local, branch)? {
                paths.push(branch.to_owned());
            } else if refs_of(local).contains_key(branch) {
                // The Mac would read its own value of the branch: never a guess.
                anyhow::bail!("git is busy in {} ({branch} is locked)", local.display());
            }
        }
    }
    // `pack-objects --revs` takes object ids and `--not`, never `--branches`: the tips by hand.
    let mut lines: Vec<String> = vec!["HEAD".to_owned()];
    lines.extend(
        refs_of(local)
            .into_iter()
            .filter(|(name, id)| name.starts_with("refs/heads/") && is_object_id(id))
            .map(|(_, id)| id),
    );
    // Staged changes: the blobs the index holds that HEAD's tree does not (never every indexed
    // blob, which is the whole checkout). Before `--not`, which makes what follows it left out.
    lines.extend(staged_blobs(local)?);
    match std::fs::read_to_string(local.join(TIPS)) {
        Ok(tips) => {
            let tips: Vec<String> = tips
                .lines()
                .map(str::trim)
                .filter(|id| is_object_id(id))
                .map(str::to_owned)
                .collect();
            if !tips.is_empty() {
                lines.push("--not".to_owned());
                lines.extend(tips);
            }
        }
        Err(_) => {
            let remotes: Vec<String> = refs_of(local)
                .into_iter()
                .filter(|(name, id)| name.starts_with("refs/remotes/") && is_object_id(id))
                .map(|(_, id)| id)
                .collect();
            if !remotes.is_empty() {
                lines.push("--not".to_owned());
                lines.extend(remotes);
            }
        }
    }
    if let Some(pack) = pack_new_objects(local, &lines)? {
        paths.push(format!("objects/pack/{pack}"));
    }
    Ok(paths)
}

fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Writes `branch` loose with its packed value when it is not loose already, under git's lock
/// (`<ref>.lock`, made exclusively). Whether the branch is a loose file now (false: it has no
/// value, or git held the lock).
fn loose_ref(local: &Path, branch: &str) -> anyhow::Result<bool> {
    let file = local.join(branch);
    if file.is_file() {
        return Ok(true);
    }
    let Some(parent) = file.parent() else { return Ok(false) };
    std::fs::create_dir_all(parent)?;
    let lock = file.with_file_name(format!(
        "{}.lock",
        file.file_name().and_then(|name| name.to_str()).unwrap_or_default()
    ));
    let mut held = match std::fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
        Ok(held) => held,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    // Under the lock: a commit that wrote it loose meanwhile wins.
    let value = if file.is_file() { None } else { refs_of(local).get(branch).cloned() };
    let done = match value {
        Some(id) if is_object_id(&id) => {
            held.write_all(format!("{id}\n").as_bytes())?;
            drop(held);
            std::fs::rename(&lock, &file)?;
            true
        }
        _ => {
            drop(held);
            std::fs::remove_file(&lock)?;
            file.is_file()
        }
    };
    Ok(done)
}

/// The blobs the index of `local` stages that differ from `HEAD`'s tree (every staged blob when
/// there is no commit yet). Reads the index and trees only, never a work tree.
fn staged_blobs(local: &Path) -> anyhow::Result<Vec<String>> {
    let mut head = git(local);
    head.args(["rev-parse", "--verify", "--quiet", "HEAD^{tree}"]);
    let listed = match text(head) {
        Ok(_) => {
            let mut diff = git(local);
            diff.env("GIT_NO_LAZY_FETCH", "1");
            diff.args(["diff-index", "--cached", "--no-renames", "--abbrev=64", "HEAD"]);
            text(diff)?
                .lines()
                // `:old_mode new_mode old_id new_id status\tpath`
                .filter_map(|line| line.split_whitespace().nth(3).map(str::to_owned))
                .collect::<Vec<_>>()
        }
        Err(_) => {
            let mut files = git(local);
            files.args(["ls-files", "--stage"]);
            text(files)?
                .lines()
                .filter_map(|line| line.split_whitespace().nth(1).map(str::to_owned))
                .collect()
        }
    };
    Ok(listed.into_iter().filter(|id| is_object_id(id) && id.bytes().any(|b| b != b'0')).collect())
}

/// One pack in `local`'s `objects/pack` of the objects `revs` (a `pack-objects --revs` list)
/// reach; its file name, None when there is no such object. Never asks the promisor
/// remote for a missing object (one the Mac had, so left out anyway).
fn pack_new_objects(local: &Path, revs: &[String]) -> anyhow::Result<Option<String>> {
    let pack_dir = local.join("objects").join("pack");
    std::fs::create_dir_all(&pack_dir)?;
    let mut pack = git(local);
    pack.env("GIT_NO_LAZY_FETCH", "1");
    pack.args(["pack-objects", "--quiet", "--revs", "--missing=allow-any"]);
    // `pack-<id>.pack`: the only pack name the Mac's git leg takes. The same objects make the same
    // name, so a second Bring Back writes no second pack.
    pack.arg(pack_dir.join("pack"));
    let mut child =
        pack.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        for line in revs {
            writeln!(stdin, "{line}")?;
        }
    }
    let out = child.wait_with_output()?;
    anyhow::ensure!(
        out.status.success(),
        "packing the machine's objects failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let id = String::from_utf8(out.stdout)?.trim().to_owned();
    anyhow::ensure!(is_object_id(&id), "pack-objects named no pack");
    let name = format!("pack-{id}.pack");
    let file = pack_dir.join(&name);
    // Header (12 bytes) and trailer (the checksum) only: no object.
    let empty = 12 + id.len() as u64 / 2;
    if std::fs::metadata(&file)?.len() <= empty {
        for ext in ["pack", "idx", "rev"] {
            let _ = std::fs::remove_file(pack_dir.join(format!("pack-{id}.{ext}")));
        }
        return Ok(None);
    }
    Ok(Some(name))
}

/// What a swap attempt did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// `.git` is now the machine's repository at this path.
    Swapped(PathBuf),
    /// `.git` was already the machine's (an earlier swap, kept in the upper layer).
    Already,
    /// The folder has no `.git` folder from the Mac.
    NoRepository,
    /// No remote that reads without credentials: `.git` stays on projection.
    NoPublicRemote,
}

/// Prepares and swaps the `.git` of the projected folder mounted at `at` with its state in
/// `state`. `forget` drops the kernel's cached `.git` entry once the overlay answers the link
/// ([`crate::mount::Mounted::forget_entry`] of the root); it runs only after the long fetch, so a
/// caller can take its mount's lock there alone. Linux only (the mount is).
#[cfg(target_os = "linux")]
pub fn run(
    overlay: &crate::overlay::Overlay,
    forget: &dyn Fn() -> std::io::Result<()>,
    at: &Path,
    state: &Path,
) -> anyhow::Result<Outcome> {
    run_repo(overlay, &|_| forget(), at, "", state)
}

/// [`run`] for the repository at `repo` below the folder (`""` for the folder itself, else a
/// path from [`repositories`]). `forget` gets the number of the folder holding that `.git`.
#[cfg(target_os = "linux")]
pub fn run_repo(
    overlay: &crate::overlay::Overlay,
    forget: &dyn Fn(u64) -> std::io::Result<()>,
    at: &Path,
    repo: &str,
    state: &Path,
) -> anyhow::Result<Outcome> {
    let work = if repo.is_empty() { at.to_path_buf() } else { at.join(repo) };
    let projected = work.join(".git");
    match std::fs::symlink_metadata(&projected) {
        Ok(meta) if meta.file_type().is_symlink() => return Ok(Outcome::Already),
        Ok(meta) if meta.is_dir() => {}
        _ => return Ok(Outcome::NoRepository),
    }
    let Some((_, url)) = seed_remote(&projected) else { return Ok(Outcome::NoPublicRemote) };
    let local = local_dir(state, repo);
    let ready = match prepare(&projected, &local, &url) {
        Ok(ready) => ready,
        Err(error) if is_unreachable(&error) => return Ok(Outcome::NoPublicRemote),
        Err(error) => return Err(error),
    };
    swap_at(overlay, forget, repo, &projected, &local, &ready)?;
    Ok(Outcome::Swapped(local))
}

/// The rename itself: refused (an error, `.git` stays on projection) when the Mac's state moved
/// since `ready` was taken.
#[cfg(target_os = "linux")]
pub fn swap(
    overlay: &crate::overlay::Overlay,
    forget: &dyn Fn() -> std::io::Result<()>,
    projected: &Path,
    local: &Path,
    ready: &str,
) -> anyhow::Result<()> {
    swap_at(overlay, &|_| forget(), "", projected, local, ready)
}

/// [`swap`] of the `.git` of `repo` below the folder (`""` for the folder itself).
#[cfg(target_os = "linux")]
pub fn swap_at(
    overlay: &crate::overlay::Overlay,
    forget: &dyn Fn(u64) -> std::io::Result<()>,
    repo: &str,
    projected: &Path,
    local: &Path,
    ready: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        fingerprint(projected)?.as_deref() == Some(ready),
        "the repository changed after it was copied"
    );
    let name = if repo.is_empty() { ".git".to_owned() } else { format!("{repo}/.git") };
    overlay
        .swap_to_link(&name, local)
        .map_err(|errno| anyhow::anyhow!("swapping {name}: errno {errno}"))?;
    let parent = overlay
        .folder_ino(repo)
        .ok_or_else(|| anyhow::anyhow!("{repo}: the folder is gone after the swap"))?;
    forget(parent)?;
    Ok(())
}

fn is_unreachable(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}");
    text.contains("ls-remote")
        && ["could not read", "Authentication", "not found", "terminal prompts disabled"]
            .iter()
            .any(|needle| text.contains(needle))
}

#[cfg(all(test, target_os = "linux"))]
#[path = "git_swap_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "git_swap_back_tests.rs"]
mod back_tests;
