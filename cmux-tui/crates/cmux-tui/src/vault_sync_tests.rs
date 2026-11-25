use super::*;
use serde_json::json;

#[test]
fn the_setting_and_the_vault_state_pick_the_step() {
    for state in [None, Some("off"), Some("pending"), Some("active"), Some("not_enrolled")] {
        assert_eq!(step(false, state), Step::Rest, "off does nothing in {state:?}");
    }
    assert_eq!(step(true, Some("off")), Step::Enroll);
    assert_eq!(step(true, Some("not_enrolled")), Step::Enroll);
    assert_eq!(step(true, Some("pending")), Step::Wait);
    assert_eq!(step(true, Some("active")), Step::Sync);
    assert_eq!(step(true, Some("something_new")), Step::Rest);
    assert_eq!(step(true, None), Step::Rest);
}

#[test]
fn only_the_apps_own_daemon_syncs() {
    assert!(runs_for("cmux-app"));
    assert!(runs_for("cmux-app-vault-mac"));
    assert!(!runs_for("cmux-chief-0a1b2c"));
    assert!(!runs_for("default"));
    assert!(!runs_for("cmux-application"));
}

#[test]
fn the_setting_reads_nested_or_flat_and_defaults_off() {
    use acpmux::user_settings::Settings;
    assert!(enabled_in(&Settings::Parsed(json!({"cloud": {"vault": {"enabled": true}}}))));
    assert!(enabled_in(&Settings::Parsed(json!({"cloud.vault.enabled": true}))));
    assert!(!enabled_in(&Settings::Parsed(json!({"cloud": {"vault": {"enabled": false}}}))));
    assert!(!enabled_in(&Settings::Parsed(json!({"cloud.vault.enabled": "yes"}))));
    assert!(!enabled_in(&Settings::Parsed(json!({}))));
    assert!(!enabled_in(&Settings::Missing));
    assert!(!enabled_in(&Settings::Unreadable("bad".into())));
}

#[test]
fn a_file_event_wakes_only_on_turning_on_or_a_touch_while_on() {
    let at = |s: u64| Some(SystemTime::UNIX_EPOCH + Duration::from_secs(s));
    let off = Signature { enabled: false, wake: at(1) };
    let on = Signature { enabled: true, wake: at(1) };
    let touched = Signature { enabled: true, wake: at(2) };
    assert!(wakes(None, &on), "the first look with the vault on runs a tick");
    assert!(!wakes(None, &off));
    assert!(wakes(Some(&off), &on), "turning it on enrolls or syncs at once");
    assert!(!wakes(Some(&on), &on), "another setting's change is not a reason to sync");
    assert!(wakes(Some(&on), &touched), "`cmux vault sync --soon`");
    assert!(!wakes(Some(&on), &Signature { enabled: false, wake: at(3) }), "off never wakes it");
}

#[test]
fn the_sync_log_names_machines_and_names_only() {
    let answer = json!({"machines": [
        {"id": "vm_aaaaaaaaaaaaaaaaaaaa", "outcome": "delivered", "names": ["OPENAI_API_KEY", "DATABASE_URL"]},
        {"id": "vm_bbbbbbbbbbbbbbbbbbbb", "outcome": "current", "names": ["OPENAI_API_KEY"]},
        {"id": "vm_cccccccccccccccccccc", "outcome": "failed", "names": []},
    ]});
    assert_eq!(
        sync_line(&answer),
        "vault: sealed vm_aaaaaaaaaaaaaaaaaaaa delivered (OPENAI_API_KEY, DATABASE_URL); vm_cccccccccccccccccccc failed ()"
    );
    assert_eq!(
        sync_line(&json!({"machines": [{"id": "vm_x", "outcome": "current"}]})),
        "vault: 1 machine(s) current"
    );
    assert_eq!(sync_line(&json!({})), "vault: 0 machine(s) current");
}

#[test]
fn stop_ends_the_loop_without_a_tick() {
    let shared: Shared = Arc::default();
    let looped = shared.clone();
    let thread = std::thread::spawn(move || run(&looped));
    let sync = VaultSync { shared, _watch: None };
    sync.stop();
    // The loop is waiting for its first period; stop wakes it and it returns at once.
    let started = Instant::now();
    thread.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
}
