use voidspace_app::settings::Settings;
use voidspace_app::{PreviewState, WorkspaceMode, workspace_mode};
use voidspace_model::NodeId;

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

#[test]
fn hover_preview_temporarily_wins_over_a_pinned_tile() {
    let state = PreviewState {
        pinned: Some(NodeId(10)),
    };

    assert_eq!(state.active(Some(NodeId(20))), Some(NodeId(20)));
    assert_eq!(state.active(None), Some(NodeId(10)));
}

#[test]
fn left_click_pins_and_empty_canvas_click_clears_preview() {
    let mut state = PreviewState::default();
    state.apply_canvas_click(Some(NodeId(42)));
    assert_eq!(state.active(None), Some(NodeId(42)));

    state.apply_canvas_click(None);
    assert_eq!(state.active(None), None);
}
