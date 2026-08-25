use voidspace_app::settings::Settings;
use voidspace_app::{WorkspaceMode, workspace_mode};

#[test]
fn inspector_docks_at_1024_and_closes_at_800() {
    assert_eq!(workspace_mode(1024.0), WorkspaceMode::Docked);
    assert_eq!(workspace_mode(800.0), WorkspaceMode::DrawerClosed);
}

#[test]
fn settings_round_trip_is_atomic() {
    let sandbox = tempfile::tempdir().unwrap();
    let path = sandbox.path().join("settings.json");
    let settings = Settings {
        version: 1,
        last_scope: r"C:\Data".into(),
        always_request_admin: true,
    };
    settings.save_to(&path).unwrap();
    assert_eq!(Settings::load_from(&path).unwrap(), settings);
}

#[test]
fn diagnostics_redacts_user_profile_and_newlines() {
    let profile = std::env::var("USERPROFILE").unwrap();
    let input = format!("failed at {profile}\\secret\nnext");
    let redacted = voidspace_app::diagnostics::redact(&input);
    assert!(!redacted.contains(&profile));
    assert!(redacted.contains("%USERPROFILE%"));
    assert!(!redacted.contains('\n'));
}
