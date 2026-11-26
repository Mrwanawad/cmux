//! The swap's parts that need no mount (issue 57201): which repositories a folder of clones
//! swaps, where each one's machine repository lives, and what Bring Back reads of a swapped
//! `.git` ([`export_for_back`] through [`crate::layer::Layer::collect`], and the mark).

use std::path::{Path, PathBuf};

use super::*;
use crate::layer::Layer;
use crate::manifest::Kind;

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn swapping_is_on_unless_turned_off() {
    assert!(enabled_by(None));
    assert!(enabled_by(Some("1")));
    assert!(enabled_by(Some("")));
    for off in ["0", "false", "off", "no", " 0 "] {
        assert!(!enabled_by(Some(off)), "{off}");
    }
}

#[test]
fn a_folder_of_clones_swaps_each_clone_and_nothing_below_one() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path();
    for repo in ["cmux-pr-20415", "cmux-pr-38675", "work/deep/clone", "a/b/c/too-deep"] {
        std::fs::create_dir_all(at.join(repo).join(".git")).unwrap();
    }
    // Below a clone (its own work tree), in a hidden folder or build output: never a clone.
    std::fs::create_dir_all(at.join("cmux-pr-20415/vendor/lib/.git")).unwrap();
    std::fs::create_dir_all(at.join(".claude/worktrees/x/.git")).unwrap();
    std::fs::create_dir_all(at.join("node_modules/pkg/.git")).unwrap();
    // A `.git` file (a linked worktree) is not a repository the swap copies.
    std::fs::create_dir_all(at.join("linked")).unwrap();
    std::fs::write(at.join("linked/.git"), "gitdir: /elsewhere\n").unwrap();
    std::fs::create_dir_all(at.join("artifacts/OUTPUT")).unwrap();
    assert_eq!(repositories(at), ["cmux-pr-20415", "cmux-pr-38675", "work/deep/clone"]);

    // The folder's own repository first, and nothing below it.
    std::fs::create_dir_all(at.join(".git")).unwrap();
    assert_eq!(repositories(at), [""]);
}

#[test]
fn each_repository_has_its_own_machine_repository_in_the_state_folder() {
    let state = Path::new("/state/chat-1");
    assert_eq!(local_dir(state, ""), state.join(LOCAL_DIR));
    let a = local_dir(state, "cmux-pr-20415");
    let b = local_dir(state, "cmux-pr-38675");
    assert_ne!(a, b);
    assert_eq!(a, local_dir(state, "cmux-pr-20415"), "the same path each time");
    for dir in [local_dir(state, ""), a.clone(), b] {
        assert!(is_local_dir(state, &dir), "{}", dir.display());
    }
    assert!(!is_local_dir(Path::new("/state/chat-2"), &a), "another projection's");
    assert!(!is_local_dir(state, &state.join("upper")));
    assert!(!is_local_dir(state, &state.join("git-local-nothex0000000000")));
    assert!(!is_local_dir(state, Path::new("../git-local")));
}

struct Swapped {
    _dir: tempfile::TempDir,
    mac: PathBuf,
    machine: PathBuf,
    state: PathBuf,
    local: PathBuf,
}

/// A Mac clone in `app/` of a public remote with one unpushed commit, its `.git` copied to the
/// machine as the swap copies it ([`prepare`], no mount needed: it reads the `.git` folder it is
/// given), and the upper layer's `app/.git` a link to it, as [`swap_at`] leaves it.
fn swapped() -> Swapped {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let public = root.join("public.git");
    std::fs::create_dir_all(&public).unwrap();
    sh(&public, &["init", "--bare", "--quiet", "-b", "main"]);
    sh(&public, &["config", "uploadpack.allowFilter", "true"]);
    sh(&public, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
    let url = format!("file://{}", public.display());
    let mac = root.join("mac");
    std::fs::create_dir_all(&mac).unwrap();
    sh(&mac, &["clone", "--quiet", &url, "app"]);
    let app = mac.join("app");
    sh(&app, &["checkout", "--quiet", "-b", "main"]);
    for n in 0..3 {
        std::fs::write(app.join("README.md"), format!("version {n}\n")).unwrap();
        sh(&app, &["add", "-A"]);
        sh(&app, &["commit", "--quiet", "-m", &format!("pushed {n}")]);
    }
    sh(&app, &["push", "--quiet", "-u", "origin", "main"]);
    std::fs::write(app.join("unpushed.txt"), "only on the mac\n").unwrap();
    sh(&app, &["add", "unpushed.txt"]);
    sh(&app, &["commit", "--quiet", "-m", "unpushed"]);
    sh(&app, &["gc", "--quiet"]);

    let state = root.join("state");
    let local = local_dir(&state, "app");
    std::fs::create_dir_all(state.join("upper/app")).unwrap();
    prepare(&app.join(".git"), &local, &url).unwrap();
    std::os::unix::fs::symlink(&local, state.join("upper/app/.git")).unwrap();
    // The machine's work tree: the Mac's files (what the mount shows), its `.git` the swapped one.
    let machine = root.join("machine");
    std::fs::create_dir_all(&machine).unwrap();
    for name in ["README.md", "unpushed.txt"] {
        std::fs::copy(app.join(name), machine.join(name)).unwrap();
    }
    std::os::unix::fs::symlink(&local, machine.join(".git")).unwrap();
    Swapped { _dir: dir, mac: app, machine, state, local }
}

fn git_paths(layer: &Layer) -> Vec<(String, Kind)> {
    layer
        .upper
        .iter()
        .filter(|entry| entry.path.starts_with("app/.git"))
        .map(|entry| (entry.path.clone(), entry.kind))
        .collect()
}

#[test]
fn bring_back_lists_a_swapped_git_as_the_files_its_git_leg_reads() {
    let s = swapped();
    // Nothing done on the machine yet: HEAD, the index and the branch, no pack.
    let layer = Layer::collect(&s.state).unwrap();
    let paths = git_paths(&layer);
    assert!(paths.contains(&("app/.git".into(), Kind::Dir)), "{paths:?}");
    assert!(paths.contains(&("app/.git/HEAD".into(), Kind::File)), "{paths:?}");
    assert!(paths.contains(&("app/.git/index".into(), Kind::File)), "{paths:?}");
    assert!(paths.contains(&("app/.git/refs/heads/main".into(), Kind::File)), "{paths:?}");
    assert!(!paths.iter().any(|(path, _)| path.contains("objects/pack/")), "{paths:?}");
    assert!(!layer.upper.iter().any(|entry| entry.kind == Kind::Link), "no link to the state");
    let mac_head = sh(&s.mac, &["rev-parse", "HEAD"]);
    let branch = std::fs::read_to_string(s.local.join("refs/heads/main")).unwrap();
    assert_eq!(branch, mac_head, "the branch as the machine has it, written loose");

    // The agent commits on the machine.
    let before = crate::state::mark(&s.state).unwrap();
    std::fs::write(s.machine.join("made-here.txt"), "the machine's file\n").unwrap();
    sh(&s.machine, &["add", "made-here.txt"]);
    sh(&s.machine, &["commit", "--quiet", "-m", "on the machine"]);
    let commit = sh(&s.machine, &["rev-parse", "HEAD"]);
    let after = crate::state::mark(&s.state).unwrap();
    assert_ne!(before, after, "a commit in the machine's repository is a write of the chat");
    // Reading the layer (the export) leaves the mark as it was: Bring Back can free it.
    let layer = Layer::collect(&s.state).unwrap();
    assert_eq!(crate::state::mark(&s.state).unwrap(), after);
    let paths = git_paths(&layer);
    let packs: Vec<&String> = paths
        .iter()
        .map(|(path, _)| path)
        .filter(|path| path.starts_with("app/.git/objects/pack/pack-") && path.ends_with(".pack"))
        .collect();
    assert_eq!(packs.len(), 1, "{paths:?}");
    assert_eq!(std::fs::read_to_string(s.local.join("refs/heads/main")).unwrap(), commit);

    // What the Mac's git leg does with it: the pack in a quarantine borrowing the Mac's objects
    // holds the new commit, its tree and blob, and nothing the Mac had.
    let pack = s.state.join("upper").join(packs[0]);
    let listed = sh(&s.mac, &["verify-pack", "-v", pack.to_str().unwrap()]);
    let objects: Vec<&str> = listed
        .lines()
        .filter(|line| [" commit ", " tree ", " blob "].iter().any(|kind| line.contains(kind)))
        .collect();
    assert_eq!(objects.len(), 3, "the commit, its tree and the new blob: {listed}");
    let quarantine = s.mac.parent().unwrap().join("quarantine.git");
    sh(s.mac.parent().unwrap(), &["init", "--bare", "--quiet", "quarantine.git"]);
    let alternates = format!("{}\n", s.mac.join(".git/objects").display());
    std::fs::write(quarantine.join("objects/info/alternates"), alternates).unwrap();
    std::fs::create_dir_all(quarantine.join("objects/pack")).unwrap();
    let copied = quarantine.join("objects/pack/machine.pack");
    std::fs::copy(&pack, &copied).unwrap();
    sh(&quarantine, &["index-pack", copied.to_str().unwrap()]);
    let walked = sh(&quarantine, &["rev-list", "--objects", commit.trim()]);
    assert!(walked.lines().count() >= 6, "the whole history reads: {walked}");
    sh(&quarantine, &["fsck", "--connectivity-only", "--no-dangling", commit.trim()]);

    // A second listing packs the same objects under the same name.
    let again = git_paths(&Layer::collect(&s.state).unwrap());
    assert_eq!(again, paths);
}

#[test]
fn a_staged_file_on_the_machine_comes_back_in_the_pack() {
    let s = swapped();
    std::fs::write(s.machine.join("staged.txt"), "staged on the machine\n").unwrap();
    sh(&s.machine, &["add", "staged.txt"]);
    let blob = sh(&s.machine, &["rev-parse", ":staged.txt"]);
    let layer = Layer::collect(&s.state).unwrap();
    let pack = git_paths(&layer)
        .into_iter()
        .map(|(path, _)| path)
        .find(|path| path.ends_with(".pack"))
        .expect("a pack of the staged blob");
    let listed =
        sh(&s.mac, &["verify-pack", "-v", s.state.join("upper").join(pack).to_str().unwrap()]);
    assert!(listed.contains(blob.trim()), "{listed}");
}
