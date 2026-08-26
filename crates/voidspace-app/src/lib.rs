//! Voidspace native desktop application.

mod app;
pub mod diagnostics;
pub mod settings;
mod theme;
mod treemap;
mod treemap_state;
mod volume;
mod volume_switcher;

pub use app::{VoidspaceApp, WorkspaceMode, workspace_mode};
pub use treemap::PreviewState;
pub use treemap_state::{AggregateSelection, TreemapAction, TreemapState, ViewPath};
pub use volume_switcher::{
    ScopePresentation, VolumeRootKey, VolumeSwitcherAction, VolumeSwitcherState,
    repair_volume_focus,
};
