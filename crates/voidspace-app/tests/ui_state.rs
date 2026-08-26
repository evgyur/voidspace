use voidspace_app::settings::Settings;
use voidspace_app::{
    AggregateSelection, PreviewState, TreemapAction, TreemapState, ViewPath, WorkspaceMode,
    workspace_mode,
};
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

#[test]
fn nested_zoom_builds_canonical_path_and_back_returns_real_parent() {
    let root = NodeId(1);
    let parent = NodeId(2);
    let nested = NodeId(3);
    let parent_of = |id| match id {
        NodeId(2) => Some(root),
        NodeId(3) => Some(parent),
        _ => None,
    };
    let mut path = ViewPath::root(root);
    path.rebuild(nested, parent_of).unwrap();

    assert_eq!(path.as_slice(), &[root, parent, nested]);
    assert_eq!(path.back(), Some(parent));
    assert_eq!(path.as_slice(), &[root, parent]);
}

#[test]
fn action_reducer_distinguishes_selection_pin_and_zoom() {
    let root = NodeId(1);
    let directory = NodeId(2);
    let nested = NodeId(3);
    let mut state = TreemapState::new(root);

    state.apply(TreemapAction::ActivateBaseDirectory(directory));
    assert_eq!(state.selected, Some(directory));
    assert_eq!(state.pinned, Some(directory));

    state.apply(TreemapAction::ActivateNested(nested));
    assert_eq!(state.selected, Some(nested));
    assert_eq!(state.pinned, Some(directory));

    state.apply(TreemapAction::Zoom(nested));
    assert_eq!(state.selected, Some(nested));
    assert_eq!(state.pinned, None);
    assert_eq!(state.aggregate, None);

    state.apply(TreemapAction::ActivateBaseDirectory(directory));
    state.apply(TreemapAction::ActivateBaseLeaf(NodeId(4)));
    assert_eq!(state.selected, Some(NodeId(4)));
    assert_eq!(state.pinned, None);

    state.apply(TreemapAction::OpenAggregate(AggregateSelection {
        parent: directory,
        depth: 1,
        members: vec![NodeId(5), NodeId(6)],
    }));
    assert_eq!(state.selected, Some(directory));
    assert_eq!(
        state
            .aggregate
            .as_ref()
            .map(|group| group.members.as_slice()),
        Some(&[NodeId(5), NodeId(6)][..])
    );
    state.apply(TreemapAction::ClearPreview);
    assert_eq!(state.selected, Some(directory));
    assert_eq!(
        state.aggregate.as_ref().map(|group| group.members.len()),
        Some(2)
    );
}

#[test]
fn live_repair_prunes_missing_tail_and_clears_stale_transient_state() {
    let mut state = TreemapState::new(NodeId(1));
    state.view_path = ViewPath::from_ids(vec![NodeId(1), NodeId(2), NodeId(3)]).unwrap();
    state.selected = Some(NodeId(3));
    state.pinned = Some(NodeId(2));
    state.aggregate = Some(AggregateSelection {
        parent: NodeId(3),
        depth: 1,
        members: vec![NodeId(4)],
    });

    state.repair(
        |id| id != NodeId(3),
        |id| match id {
            NodeId(2) => Some(NodeId(1)),
            _ => None,
        },
    );

    assert_eq!(state.view_path.as_slice(), &[NodeId(1), NodeId(2)]);
    assert_eq!(state.selected, Some(NodeId(2)));
    assert_eq!(state.pinned, Some(NodeId(2)));
    assert_eq!(state.aggregate, None);
}

#[test]
fn live_repair_truncates_at_the_first_broken_internal_link() {
    let mut state = TreemapState::new(NodeId(1));
    state.view_path = ViewPath::from_ids(vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)]).unwrap();
    state.repair(
        |_| true,
        |id| match id {
            NodeId(2) => Some(NodeId(1)),
            NodeId(3) => Some(NodeId(99)),
            NodeId(4) => Some(NodeId(3)),
            _ => None,
        },
    );
    assert_eq!(state.view_path.as_slice(), &[NodeId(1), NodeId(2)]);
}

#[test]
fn pin_nested_zoom_back_and_breadcrumb_share_one_path() {
    let root = NodeId(1);
    let base = NodeId(2);
    let nested = NodeId(3);
    let mut state = TreemapState::new(root);
    state.apply(TreemapAction::ActivateBaseDirectory(base));
    state.apply(TreemapAction::ActivateNested(nested));
    state
        .view_path
        .rebuild(nested, |id| match id {
            NodeId(2) => Some(root),
            NodeId(3) => Some(base),
            _ => None,
        })
        .unwrap();
    state.apply(TreemapAction::Zoom(nested));

    assert_eq!(state.view_path.back(), Some(base));
    assert_eq!(state.view_path.jump_to(root), Some(root));
    assert_eq!(state.view_path.as_slice(), &[root]);
}
