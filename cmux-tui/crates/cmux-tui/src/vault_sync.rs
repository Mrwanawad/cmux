//! The Mac daemon's half of the end-to-end encrypted cmux vault (cmuxterm-hq docs/design/e2e-vault.md, "Who seals
//! to a machine, and when"): while the person is signed in and `cloud.vault.enabled` is on, this app's daemon seals
//! what each of their Cloud machines (spares too) should hold, every [`PERIOD`] and at once when the setting turns on
//! or something touches the wake file (`cmux vault sync --soon`, cli/vault.rs `wake_file`). Turning the setting on
//! also enrolls this Mac (the first device makes the vault; a later one waits for an approve by its safety code), so
//! the palette, `cmux settings set` and MCP turn it on as Settings > Cloud > Vault does. Off stops the work and
//! deletes nothing.
//!
//! The work is the CLI's own (`cli::vault_background`, on this device's key in the Keychain and the signed-in app's
//! token), run on one thread of its own. Each call is bounded by the transport's timeouts; the log names states,
//! codes, machine ids and value names, never a value. No polling: one timer and the settings file's kernel events.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;

use crate::cli::VaultBackground;

/// The setting (schemas/settings/source/machines.json, default off).
pub(crate) const SETTING: &str = "cloud.vault.enabled";
/// How often a signed-in Mac with the vault on seals to the person's machines (the design's "every 60 s").
pub(crate) const PERIOD: Duration = Duration::from_secs(60);

/// What one tick does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The setting is off, or the vault answered a state this loop does not act on.
    Rest,
    /// The setting is on and this Mac has no device in the vault yet: enroll it.
    Enroll,
    /// This Mac waits for an approve on another device: nothing to seal with yet.
    Wait,
    /// The vault is open here: seal to every machine whose delivery is not current.
    Sync,
}

/// The tick's step from the setting and this device's vault state (`cmux vault status`'s `state`).
pub(crate) fn step(enabled: bool, state: Option<&str>) -> Step {
    if !enabled {
        return Step::Rest;
    }
    match state {
        Some("active") => Step::Sync,
        Some("pending") => Step::Wait,
        Some("off" | "not_enrolled") => Step::Enroll,
        _ => Step::Rest,
    }
}

/// Whether `session` is the one this daemon runs the vault for: the app's own (`cmux-app`, a tagged build's
/// `cmux-app-<tag>`), never a Chief's, so one Mac seals from one daemon per build.
pub(crate) fn runs_for(session: &str) -> bool {
    session == "cmux-app" || session.starts_with("cmux-app-")
}

/// Whether the settings document turns the vault on (a missing or unreadable file: off).
pub(crate) fn enabled_in(settings: &acpmux::user_settings::Settings) -> bool {
    match settings {
        acpmux::user_settings::Settings::Parsed(doc) => {
            acpmux::user_settings::value(doc, SETTING).and_then(|v| v.as_bool()).unwrap_or(false)
        }
        _ => false,
    }
}

fn enabled_now() -> bool {
    enabled_in(&acpmux::user_settings::read())
}

/// What a file event is compared by: the setting and the wake file's time. A change of either wakes the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Signature {
    pub enabled: bool,
    pub wake: Option<SystemTime>,
}

/// Whether a file event should wake the loop: the setting turned on, or the wake file was touched while it is on.
pub(crate) fn wakes(before: Option<&Signature>, now: &Signature) -> bool {
    match before {
        None => now.enabled,
        Some(before) => now.enabled && (!before.enabled || before.wake != now.wake),
    }
}

#[derive(Default)]
struct Flags {
    stop: bool,
    poked: bool,
}

type Shared = Arc<(Mutex<Flags>, Condvar)>;

fn poke(shared: &Shared) {
    let (lock, wake) = &**shared;
    lock.lock().unwrap_or_else(PoisonError::into_inner).poked = true;
    wake.notify_one();
}

/// The running loop. [`VaultSync::stop`] ends it at its next wake without waiting for a tick in flight.
pub(crate) struct VaultSync {
    shared: Shared,
    _watch: Option<cmux_config::watch::Watcher>,
}

impl VaultSync {
    /// Starts the loop for `session` when it is the app's ([`runs_for`]); `None` otherwise or without a thread.
    pub(crate) fn start(session: &str) -> Option<VaultSync> {
        if !runs_for(session) {
            return None;
        }
        let shared: Shared = Arc::default();
        let looped = shared.clone();
        std::thread::Builder::new()
            .name("vault-sync".into())
            .spawn(move || run(&looped))
            .map_err(|error| {
                crate::client_log::stderr_log!("vault", "vault: no sync thread: {error}");
            })
            .ok()?;
        let wake_file = crate::cli::vault_wake_file();
        let mut files: Vec<PathBuf> = acpmux::user_settings::paths();
        files.extend(wake_file.clone());
        let watched = shared.clone();
        let mut seen: Option<Signature> = None;
        let watch = cmux_config::watch::Watcher::start(files, move || {
            let now = Signature {
                enabled: enabled_now(),
                wake: wake_file
                    .as_deref()
                    .and_then(|path| std::fs::metadata(path).ok())
                    .and_then(|meta| meta.modified().ok()),
            };
            if wakes(seen.as_ref(), &now) {
                poke(&watched);
            }
            seen = Some(now);
        })
        .map_err(|error| {
            // The timer still runs: a setting turned on is seen at the next tick.
            crate::client_log::stderr_log!("vault", "vault: no settings watch: {error}");
        })
        .ok();
        Some(VaultSync { shared, _watch: watch })
    }

    pub(crate) fn stop(self) {
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(PoisonError::into_inner).stop = true;
        wake.notify_one();
    }
}

fn run(shared: &Shared) {
    let (lock, wake) = &**shared;
    let mut next = Instant::now() + PERIOD;
    let mut last = Logged::default();
    loop {
        {
            let mut flags = lock.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if flags.stop {
                    return;
                }
                if flags.poked {
                    break;
                }
                let now = Instant::now();
                if now >= next {
                    break;
                }
                flags = wake
                    .wait_timeout(flags, next - now)
                    .map(|(flags, _)| flags)
                    .unwrap_or_else(|poisoned| poisoned.into_inner().0);
            }
            flags.poked = false;
        }
        tick(&mut last);
        next = Instant::now() + PERIOD;
    }
}

/// What the log said last, so a Mac signed out or waiting for an approve says it once, not every minute.
#[derive(Default)]
struct Logged {
    line: String,
}

impl Logged {
    fn say(&mut self, line: String) {
        if line != self.line {
            crate::client_log::stderr_log!("vault", "{line}");
            self.line = line;
        }
    }
}

fn tick(last: &mut Logged) {
    if !enabled_now() {
        return;
    }
    let status = match crate::cli::vault_background(VaultBackground::Status) {
        Ok(status) => status,
        Err(failure) => return last.say(format!("vault: status refused: {}", failure.code)),
    };
    let state = status.get("state").and_then(Value::as_str);
    match step(true, state) {
        Step::Rest => last.say(format!("vault: nothing to do in state {}", state.unwrap_or("?"))),
        Step::Wait => {
            let code = status.get("safetyCode").and_then(Value::as_str).unwrap_or("?");
            last.say(format!("vault: this Mac waits for an approve (code {code})"));
        }
        Step::Enroll => match crate::cli::vault_background(VaultBackground::Enroll) {
            Ok(answer) => last.say(format!(
                "vault: enrolled this Mac ({})",
                answer.get("state").and_then(Value::as_str).unwrap_or("?")
            )),
            Err(failure) => last.say(format!("vault: enroll refused: {}", failure.code)),
        },
        Step::Sync => match crate::cli::vault_background(VaultBackground::Sync) {
            Ok(answer) => {
                let line = sync_line(&answer);
                last.say(line);
            }
            Err(failure) => last.say(format!("vault: sync refused: {}", failure.code)),
        },
    }
}

/// One log line for a sync's rows: each machine it sealed to or failed, with the names (never a value).
pub(crate) fn sync_line(answer: &Value) -> String {
    let rows = answer.get("machines").and_then(Value::as_array).cloned().unwrap_or_default();
    let acted: Vec<String> = rows
        .iter()
        .filter(|row| {
            matches!(row.get("outcome").and_then(Value::as_str), Some("delivered" | "failed"))
        })
        .map(|row| {
            let names: Vec<&str> = row
                .get("names")
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            format!(
                "{} {} ({})",
                row.get("id").and_then(Value::as_str).unwrap_or("?"),
                row.get("outcome").and_then(Value::as_str).unwrap_or("?"),
                names.join(", ")
            )
        })
        .collect();
    if acted.is_empty() {
        format!("vault: {} machine(s) current", rows.len())
    } else {
        format!("vault: sealed {}", acted.join("; "))
    }
}

#[cfg(test)]
#[path = "vault_sync_tests.rs"]
mod tests;
