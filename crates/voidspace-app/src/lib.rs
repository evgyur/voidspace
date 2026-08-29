//! Voidspace native desktop application.

mod app;
pub mod diagnostics;
pub mod hud;
mod overlay_coordinator;
pub mod settings;
mod shell;
mod status_bar;
mod tactical_arc;
mod theme;
mod treemap;
mod treemap_state;
mod volume;
mod volume_display_registry;
mod volume_switcher;
mod window;

pub use app::{VoidspaceApp, WorkspaceMode, workspace_mode};
pub use theme::release_typography_diagnostic;
pub use treemap::PreviewState;
pub use treemap_state::{AggregateSelection, TreemapAction, TreemapState, ViewPath};
pub use volume_switcher::{
    ScopePresentation, VolumeRootKey, VolumeSwitcherAction, VolumeSwitcherState,
    repair_volume_focus,
};
pub use window::main_viewport;
