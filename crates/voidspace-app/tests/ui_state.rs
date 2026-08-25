use voidspace_app::{WorkspaceMode, workspace_mode};

#[test]
fn inspector_docks_at_1024_and_closes_at_800() {
    assert_eq!(workspace_mode(1024.0), WorkspaceMode::Docked);
    assert_eq!(workspace_mode(800.0), WorkspaceMode::DrawerClosed);
}
