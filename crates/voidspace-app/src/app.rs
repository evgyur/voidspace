use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui;
use voidspace_export::{ReportFormat, export_report, save_snapshot};
use voidspace_fileops::{
    CancellationToken, ConfirmableOperation, OperationDraft, OperationKind, OperationReport,
    confirm, execute, prepare, reveal_in_explorer,
};
use voidspace_filter::{Expr, parse};
use voidspace_index::{Index, IndexSnapshot};
use voidspace_layout::{
    LayoutSnapshot, Rect as LayoutRect, SizeMode, ViewState, layout, layout_subset,
};
use voidspace_model::{DirtySet, EventEnvelope, EventPayload, NodeId, WinName};
use voidspace_scan::{ScanHandle, ScanRequest, describe_root, start};
use voidspace_watch::{WatchHandle, WatchRequest, WatchSignal, watch};

use crate::{
    TreemapAction, TreemapState, ViewPath, hud,
    overlay_coordinator::{ModalOverlay, OverlayCoordinator, TransientOverlay},
    settings::Settings,
    shell::{ABOUT_LINKS, FilterPlacement, InspectorPlacement, ShellLayout},
    status_bar::{self, StatusKind, StatusModule},
    tactical_arc::{ContextTarget, TacticalAction, TacticalArcOutcome, TacticalArcState},
    theme, treemap, volume,
    volume_display_registry::VolumeDisplayRegistry,
    volume_switcher::{self, VolumeRootKey, VolumeSwitcherAction, VolumeSwitcherState},
    window::StartupWindowSizer,
};

const MAX_SCAN_EVENTS_PER_FRAME: usize = 2_048;
const MAX_SCAN_WORK_PER_FRAME: Duration = Duration::from_millis(5);
const VOLUME_REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const VOLUME_CARD_MIN_WIDTH: f32 = 280.0;
const VOLUME_CARD_HEIGHT: f32 = 148.0;
const VOLUME_CARD_GAP: f32 = 16.0;
const VOLUME_GRID_MAX_WIDTH: f32 = 1_280.0;

#[derive(Clone, Copy, Debug)]
struct StatusBarGeometry {
    height: f32,
    vertical_margin: i8,
}

impl StatusBarGeometry {
    fn content_height(self) -> f32 {
        self.height - f32::from(self.vertical_margin) * 2.0
    }
}

fn status_bar_geometry() -> StatusBarGeometry {
    StatusBarGeometry {
        height: 48.0,
        vertical_margin: 7,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceMode {
    Docked,
    DrawerClosed,
}

pub fn workspace_mode(width: f32) -> WorkspaceMode {
    if width >= 900.0 {
        WorkspaceMode::Docked
    } else {
        WorkspaceMode::DrawerClosed
    }
}

struct ScanTab {
    title: String,
    root_path: PathBuf,
    generation: u64,
    index: Index,
    snapshot: IndexSnapshot,
    layout: LayoutSnapshot,
    events: Receiver<EventEnvelope>,
    watcher_events: Receiver<WatchSignal>,
    scan: Option<ScanHandle>,
    watcher: Option<WatchHandle>,
    scanning: bool,
    paused: bool,
    files_seen: u64,
    treemap_state: TreemapState,
    pending_rescan: bool,
    last_watch_event: Option<Instant>,
    errors: Vec<String>,
    volume_usage: Option<volume::VolumeUsage>,
    pending_navigation: Option<TreemapBookmark>,
}

#[derive(Clone, Debug)]
struct AggregateBookmark {
    parent: Vec<WinName>,
    depth: usize,
    members: Vec<Vec<WinName>>,
}

#[derive(Clone, Debug)]
struct TreemapBookmark {
    view: Vec<WinName>,
    selected: Option<Vec<WinName>>,
    pinned: Option<Vec<WinName>>,
    aggregate: Option<AggregateBookmark>,
    aggregate_views: Vec<AggregateBookmark>,
}

impl TreemapBookmark {
    fn capture(snapshot: &IndexSnapshot, state: &TreemapState) -> Self {
        Self {
            view: logical_path(snapshot, state.view_path.current()).unwrap_or_default(),
            selected: state.selected.and_then(|id| logical_path(snapshot, id)),
            pinned: state.pinned.and_then(|id| logical_path(snapshot, id)),
            aggregate: state
                .aggregate
                .as_ref()
                .and_then(|group| capture_aggregate(snapshot, group)),
            aggregate_views: state
                .aggregate_views
                .iter()
                .filter_map(|group| capture_aggregate(snapshot, group))
                .collect(),
        }
    }

    fn restore(self, snapshot: &IndexSnapshot) -> TreemapState {
        let mut state = TreemapState::new(snapshot.root);
        if let Some(view) = resolve_logical_path(snapshot, &self.view) {
            let _ = state
                .view_path
                .rebuild(view, |id| snapshot.node(id).and_then(|node| node.parent));
        }
        state.selected = self
            .selected
            .as_deref()
            .and_then(|path| resolve_logical_path(snapshot, path))
            .or(Some(state.view_path.current()));
        state.pinned = self
            .pinned
            .as_deref()
            .and_then(|path| resolve_logical_path(snapshot, path));
        state.aggregate = self
            .aggregate
            .and_then(|group| restore_aggregate(snapshot, group));
        state.aggregate_views = self
            .aggregate_views
            .into_iter()
            .filter_map(|group| restore_aggregate(snapshot, group))
            .collect();
        state
    }
}

fn logical_path(snapshot: &IndexSnapshot, target: NodeId) -> Option<Vec<WinName>> {
    let mut path = Vec::new();
    let mut current = target;
    while current != snapshot.root {
        let node = snapshot.node(current)?;
        path.push(node.name.clone());
        current = node.parent?;
    }
    path.reverse();
    Some(path)
}

fn resolve_logical_path(snapshot: &IndexSnapshot, path: &[WinName]) -> Option<NodeId> {
    let mut current = snapshot.root;
    for segment in path {
        current = snapshot
            .node(current)?
            .children
            .iter()
            .copied()
            .find(|child| {
                snapshot
                    .node(*child)
                    .is_some_and(|node| node.name == *segment)
            })?;
    }
    Some(current)
}

fn capture_aggregate(
    snapshot: &IndexSnapshot,
    group: &crate::AggregateSelection,
) -> Option<AggregateBookmark> {
    Some(AggregateBookmark {
        parent: logical_path(snapshot, group.parent)?,
        depth: group.depth,
        members: group
            .members
            .iter()
            .filter_map(|id| logical_path(snapshot, *id))
            .collect(),
    })
}

fn restore_aggregate(
    snapshot: &IndexSnapshot,
    group: AggregateBookmark,
) -> Option<crate::AggregateSelection> {
    let parent = resolve_logical_path(snapshot, &group.parent)?;
    let members = group
        .members
        .iter()
        .filter_map(|path| resolve_logical_path(snapshot, path))
        .filter(|id| snapshot.node(*id).and_then(|node| node.parent) == Some(parent))
        .collect::<Vec<_>>();
    (!members.is_empty()).then_some(crate::AggregateSelection {
        parent,
        depth: group.depth,
        members,
    })
}

impl ScanTab {
    fn apply_treemap_action(&mut self, action: TreemapAction) {
        match action {
            TreemapAction::Zoom(target) => {
                if self
                    .snapshot
                    .node(target)
                    .is_some_and(|node| !node.children.is_empty())
                {
                    let root = self.snapshot.root;
                    if self
                        .treemap_state
                        .view_path
                        .rebuild(target, |id| {
                            self.snapshot.node(id).and_then(|node| node.parent)
                        })
                        .is_none()
                    {
                        self.treemap_state.view_path = ViewPath::root(root);
                        self.errors
                            .push("Cannot build breadcrumb path for zoom target".into());
                        return;
                    }
                    self.treemap_state.apply(TreemapAction::Zoom(target));
                }
            }
            other => self.treemap_state.apply(other),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavigationIntent {
    None,
    Back,
    Jump(NodeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackDestination {
    Parent,
    VolumePicker,
}

enum InspectorAction {
    Reveal(PathBuf),
    Recycle(PathBuf),
    Permanent(PathBuf),
    Rescan,
    Snapshot,
    Report(ReportFormat),
    ShowVolumePicker,
}

enum ArtifactAction {
    Snapshot,
    Report(ReportFormat),
}

struct FileOpDialog {
    operation: ConfirmableOperation,
    phrase: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeleteDispatch {
    Immediate,
    Confirm,
}

fn delete_dispatch(kind: OperationKind) -> DeleteDispatch {
    match kind {
        OperationKind::Recycle => DeleteDispatch::Immediate,
        OperationKind::Permanent => DeleteDispatch::Confirm,
    }
}

pub struct VoidspaceApp {
    startup_window_sizer: StartupWindowSizer,
    typography: theme::Typography,
    volume_switcher: VolumeSwitcherState,
    tabs: Vec<ScanTab>,
    active_tab: usize,
    next_scan_id: u64,
    scope_text: String,
    filter_text: String,
    filter: Option<Expr>,
    filter_error: Option<String>,
    compact_filter_draft: String,
    compact_filter_prior: String,
    details_drawer: bool,
    toast: Option<String>,
    fileop_dialog: Option<FileOpDialog>,
    fileop_tx: Sender<Result<OperationReport, String>>,
    fileop_rx: Receiver<Result<OperationReport, String>>,
    fileop_running: bool,
    volume_refresh_tx: Sender<Result<Vec<volume::VolumeInfo>, String>>,
    volume_refresh_rx: Receiver<Result<Vec<volume::VolumeInfo>, String>>,
    available_volumes: Vec<volume::VolumeInfo>,
    volume_refresh_started_at: Instant,
    volume_refresh_in_flight: bool,
    volume_discovery_complete: bool,
    volume_discovery_error: Option<String>,
    volume_picker_visible: bool,
    overlays: OverlayCoordinator,
    tactical_arc: Option<TacticalArcState>,
    volume_labels: VolumeDisplayRegistry,
    status_detail_modules: Vec<StatusModule>,
    ui_frame_diagnostic: crate::diagnostics::UiFrameDiagnostic,
    settings: Settings,
}

impl VoidspaceApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        let typography = theme::install(&context.egui_ctx);
        tracing::info!(source = ?typography.source(), "typography installed");
        let settings = Settings::load();
        let default_scope = settings.last_scope.clone();
        let (fileop_tx, fileop_rx) = bounded(4);
        let (volume_refresh_tx, volume_refresh_rx) = bounded(1);
        let mut app = Self {
            startup_window_sizer: StartupWindowSizer::new(),
            typography,
            volume_switcher: VolumeSwitcherState::default(),
            tabs: Vec::new(),
            active_tab: 0,
            next_scan_id: 1,
            scope_text: default_scope,
            filter_text: String::new(),
            filter: None,
            filter_error: None,
            compact_filter_draft: String::new(),
            compact_filter_prior: String::new(),
            details_drawer: false,
            toast: None,
            fileop_dialog: None,
            fileop_tx,
            fileop_rx,
            fileop_running: false,
            volume_refresh_tx,
            volume_refresh_rx,
            available_volumes: Vec::new(),
            volume_refresh_started_at: Instant::now(),
            volume_refresh_in_flight: false,
            volume_discovery_complete: false,
            volume_discovery_error: None,
            volume_picker_visible: false,
            overlays: OverlayCoordinator::default(),
            tactical_arc: None,
            volume_labels: VolumeDisplayRegistry::default(),
            status_detail_modules: Vec::new(),
            ui_frame_diagnostic: crate::diagnostics::UiFrameDiagnostic::default(),
            settings,
        };
        let arguments: Vec<String> = std::env::args().collect();
        if let Some(index) = arguments.iter().position(|argument| argument == "--scan")
            && let Some(scope) = arguments.get(index + 1)
        {
            app.open_or_activate_scan(PathBuf::from(scope));
        }
        app.request_volume_refresh(&context.egui_ctx);
        app
    }

    fn request_volume_refresh(&mut self, context: &egui::Context) {
        if self.volume_refresh_in_flight {
            return;
        }
        self.volume_refresh_in_flight = true;
        self.volume_refresh_started_at = Instant::now();
        let sink = self.volume_refresh_tx.clone();
        let context = context.clone();
        std::thread::spawn(move || {
            let _ = sink.send(volume::list());
            context.request_repaint();
        });
    }

    fn open_or_activate_scan(&mut self, requested: PathBuf) {
        if let Some(index) = volume_switcher::matching_volume_tab_index(
            self.tabs.iter().map(|tab| tab.root_path.as_path()),
            &requested,
        ) {
            self.active_tab = index;
            self.scope_text = self.tabs[index].root_path.display().to_string();
            self.volume_switcher.open = false;
            self.volume_switcher.issue = None;
            self.volume_picker_visible = false;
            self.overlays.dismiss_transient();
            return;
        }
        self.scope_text = requested.display().to_string();
        if self.start_scan(requested) {
            self.volume_switcher.open = false;
            self.volume_switcher.issue = None;
            self.volume_picker_visible = false;
            self.overlays.dismiss_transient();
        } else {
            self.volume_switcher.issue = self.toast.clone();
        }
    }

    fn start_scan(&mut self, root_path: PathBuf) -> bool {
        self.settings.last_scope = root_path.display().to_string();
        if let Err(error) = self.settings.save() {
            let _ = crate::diagnostics::log_line(&format!("settings save failed: {error}"));
        }
        let scan_id = self.next_scan_id;
        self.next_scan_id += 1;
        let generation = 1;
        let root = match describe_root(&root_path, generation) {
            Ok(root) => root,
            Err(error) => {
                self.toast = Some(error.to_string());
                return false;
            }
        };
        let index = Index::new(
            voidspace_model::ScanId(scan_id),
            generation,
            root.identity,
            root.name,
        );
        let snapshot = index.snapshot();
        let layout = empty_layout(snapshot.root);
        let (event_tx, event_rx) = bounded(65_536);
        let scan = match start(
            ScanRequest::new(scan_id, generation, root_path.clone()),
            event_tx,
        ) {
            Ok(handle) => Some(handle),
            Err(error) => {
                self.toast = Some(error.to_string());
                return false;
            }
        };
        let (watch_tx, watch_rx) = bounded(4096);
        let watcher = watch(
            WatchRequest {
                root: root_path.clone(),
            },
            watch_tx,
        )
        .ok();
        let title = root_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root_path.display().to_string());
        let volume_usage = volume::query(&root_path);
        self.tabs.push(ScanTab {
            title,
            root_path,
            generation,
            treemap_state: TreemapState::new(snapshot.root),
            index,
            snapshot,
            layout,
            events: event_rx,
            watcher_events: watch_rx,
            scan,
            watcher,
            scanning: true,
            paused: false,
            files_seen: 0,
            pending_rescan: false,
            last_watch_event: None,
            errors: Vec::new(),
            volume_usage,
            pending_navigation: None,
        });
        self.active_tab = self.tabs.len() - 1;
        true
    }

    fn update_workers(&mut self, context: &egui::Context) {
        while let Ok(result) = self.volume_refresh_rx.try_recv() {
            self.volume_refresh_in_flight = false;
            self.volume_discovery_complete = true;
            apply_volume_refresh(
                &mut self.available_volumes,
                &mut self.volume_discovery_error,
                result,
            );
            let roots = self
                .available_volumes
                .iter()
                .filter_map(|volume| VolumeRootKey::from_scan_root(&volume.root_path))
                .collect::<Vec<_>>();
            self.volume_switcher.focused = volume_switcher::repair_volume_focus(
                self.volume_switcher.focused,
                self.volume_switcher.focused_index,
                &roots,
            );
            self.volume_switcher.focused_index = self
                .volume_switcher
                .focused
                .and_then(|focused| roots.iter().position(|root| *root == focused))
                .unwrap_or(0);
        }
        if self.tabs.is_empty() || self.volume_switcher.open || self.volume_picker_visible {
            let elapsed = self.volume_refresh_started_at.elapsed();
            if !self.volume_refresh_in_flight && elapsed >= VOLUME_REFRESH_INTERVAL {
                self.request_volume_refresh(context);
            } else if !self.volume_refresh_in_flight {
                context.request_repaint_after(VOLUME_REFRESH_INTERVAL.saturating_sub(elapsed));
            }
        }
        while let Ok(result) = self.fileop_rx.try_recv() {
            self.fileop_running = false;
            match result {
                Ok(report) => {
                    self.toast = Some(if report.failed.is_empty() {
                        format!("Removed {} item(s)", report.deleted.len())
                    } else {
                        format!(
                            "Removed {}; {} failed",
                            report.deleted.len(),
                            report.failed.len()
                        )
                    });
                    if !report.deleted.is_empty()
                        && let Some(tab) = self.tabs.get_mut(self.active_tab)
                    {
                        tab.volume_usage = volume::query(&tab.root_path);
                        tab.pending_rescan = true;
                        tab.last_watch_event = Some(Instant::now());
                    }
                }
                Err(error) => self.toast = Some(error),
            }
        }
        let mut restart = Vec::new();
        for (tab_index, tab) in self.tabs.iter_mut().enumerate() {
            while let Ok(signal) = tab.watcher_events.try_recv() {
                tab.pending_rescan = true;
                tab.last_watch_event = Some(Instant::now());
                if let WatchSignal::Invalidated { reason, .. } = signal {
                    tab.errors.push(reason);
                }
            }
            let mut dirty = DirtySet::default();
            let batch_started = Instant::now();
            for processed in 0..MAX_SCAN_EVENTS_PER_FRAME {
                if scan_batch_exhausted(processed, batch_started.elapsed()) {
                    break;
                }
                let Ok(event) = tab.events.try_recv() else {
                    break;
                };
                let finished = matches!(&event.payload, EventPayload::BaselineFinished(_));
                if matches!(&event.payload, EventPayload::UpsertNode(_)) {
                    tab.files_seen += 1;
                }
                match tab.index.apply(event) {
                    Ok(event_dirty) => dirty.merge(event_dirty),
                    Err(error) => tab.errors.push(error.to_string()),
                }
                if finished {
                    tab.scanning = false;
                }
            }
            if tab.pending_navigation.is_some() && !tab.scanning {
                tab.snapshot = tab.index.snapshot();
                if let Some(bookmark) = tab.pending_navigation.take() {
                    tab.treemap_state = bookmark.restore(&tab.snapshot);
                }
            } else if !dirty.is_empty() && tab.pending_navigation.is_none() {
                tab.snapshot = tab.index.snapshot();
                let snapshot = &tab.snapshot;
                tab.treemap_state.repair(
                    |id| snapshot.node(id).is_some(),
                    |id| snapshot.node(id).and_then(|node| node.parent),
                );
            }
            if tab.scanning || !dirty.is_empty() {
                context.request_repaint_after(Duration::from_millis(16));
            }
            if tab.pending_rescan
                && !tab.scanning
                && tab
                    .last_watch_event
                    .is_some_and(|at| at.elapsed() >= Duration::from_millis(120))
            {
                restart.push(tab_index);
            }
        }
        for tab_index in restart.into_iter().rev() {
            self.restart_tab(tab_index);
        }
    }

    fn restart_tab(&mut self, tab_index: usize) {
        let restarting_scan_id = self.tabs[tab_index].snapshot.scan_id;
        if self
            .tactical_arc
            .as_ref()
            .is_some_and(|arc| arc.target().scan_id == restarting_scan_id)
        {
            self.tactical_arc = None;
            self.overlays.dismiss_transient();
            self.toast = Some("TARGET CHANGED · OPEN AGAIN".to_owned());
        }
        let tab = &mut self.tabs[tab_index];
        if let Some(scan) = tab.scan.take() {
            scan.cancel();
        }
        tab.generation += 1;
        let Ok(root) = describe_root(&tab.root_path, tab.generation) else {
            tab.errors.push("Scan root is no longer available".into());
            return;
        };
        let navigation = TreemapBookmark::capture(&tab.snapshot, &tab.treemap_state);
        tab.index = Index::new(restarting_scan_id, tab.generation, root.identity, root.name);
        tab.pending_navigation = Some(navigation);
        tab.volume_usage = volume::query(&tab.root_path);
        let (event_tx, event_rx) = bounded(65_536);
        tab.events = event_rx;
        match start(
            ScanRequest::new(restarting_scan_id.0, tab.generation, tab.root_path.clone()),
            event_tx,
        ) {
            Ok(scan) => {
                tab.scan = Some(scan);
                tab.scanning = true;
                tab.paused = false;
                tab.pending_rescan = false;
                tab.files_seen = 0;
            }
            Err(error) => {
                tab.pending_navigation = None;
                tab.pending_rescan = false;
                tab.errors.push(error.to_string());
            }
        }
    }

    fn top_bar(&mut self, root_ui: &mut egui::Ui) {
        let top_interactive = self.overlays.modal().is_none()
            && !matches!(
                self.overlays.transient(),
                Some(
                    TransientOverlay::TacticalArc
                        | TransientOverlay::InspectorDrawer
                        | TransientOverlay::StatusDetails
                )
            );
        let active_root = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| VolumeRootKey::from_scan_root(&tab.root_path));
        let active_volume_id = self
            .tabs
            .get(self.active_tab)
            .map(|tab| self.volume_labels.label_for(&tab.root_path));
        let volume_ids = self
            .available_volumes
            .iter()
            .map(|volume| self.volume_labels.label_for(&volume.root_path))
            .collect::<Vec<_>>();
        let was_switcher_open = self.volume_switcher.open;
        let mut switcher_action = VolumeSwitcherAction::None;
        let shell_layout = ShellLayout::for_width(root_ui.max_rect().width());
        let compact = shell_layout.filter == FilterPlacement::OverlayTrigger;
        let compact_filter_width = if self.filter_error.is_some() || self.filter.is_some() {
            74.0
        } else {
            42.0
        };
        let mut open_about = false;
        let mut open_filter = false;
        let mut empty_recycle_bin = false;
        egui::Panel::top("topbar")
            .exact_size(62.0)
            .frame(
                egui::Frame::new()
                    .fill(hud::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 11))
                    .stroke(egui::Stroke::new(1.0, hud::HAIRLINE)),
            )
            .show(root_ui, |ui| {
                if !top_interactive {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [if compact { 96.0 } else { 122.0 }, 38.0],
                        egui::Label::new(theme::brand_wordmark(&self.typography)),
                    );
                    let utility_width = if compact {
                        compact_filter_width + 96.0 + 48.0 + 30.0
                    } else {
                        260.0 + 116.0 + 92.0 + 64.0
                    } + ui.spacing().item_spacing.x * 5.0;
                    let scope_width = (ui.available_width() - utility_width).max(130.0);
                    ui.allocate_ui(egui::vec2(scope_width, 36.0), |ui| {
                        ui.set_width(scope_width);
                        switcher_action = volume_switcher::show(
                            ui,
                            &mut self.scope_text,
                            &mut self.volume_switcher,
                            &self.available_volumes,
                            &volume_ids,
                            active_root,
                            active_volume_id.as_deref(),
                            self.volume_refresh_in_flight,
                            self.volume_discovery_error.as_deref(),
                            &self.typography,
                        );
                    });
                    if compact {
                        let filter_glyph = if self.filter_error.is_some() {
                            "Q ERROR"
                        } else if self.filter.is_some() {
                            "Q ACTIVE"
                        } else {
                            "⌕"
                        };
                        if ui
                            .add_sized(
                                [compact_filter_width, 38.0],
                                egui::Button::new(
                                    egui::RichText::new(filter_glyph).color(hud::CYAN),
                                )
                                .fill(hud::PANEL_RAISED),
                            )
                            .on_hover_text("Open filter")
                            .clicked()
                        {
                            open_filter = true;
                        }
                    } else {
                        let filter_response = ui.add_sized(
                            [260.0, 38.0],
                            egui::TextEdit::singleline(&mut self.filter_text)
                                .hint_text("FILTER / size > 1GiB")
                                .font(self.typography.font(theme::TypographyToken::DataNormal))
                                .margin(egui::Margin::symmetric(11, 8)),
                        );
                        if filter_response.changed() {
                            self.reparse_filter();
                        }
                    }
                    let mut turbo = egui::text::LayoutJob::default();
                    turbo.append(
                        "TURBO",
                        0.0,
                        egui::TextFormat {
                            font_id: self.typography.font(theme::TypographyToken::DisplayAction),
                            color: theme::BG,
                            ..Default::default()
                        },
                    );
                    turbo.append(
                        " / F5",
                        0.0,
                        egui::TextFormat {
                            font_id: self.typography.font(theme::TypographyToken::DataMicro),
                            color: theme::BG,
                            ..Default::default()
                        },
                    );
                    ui.add_sized(
                        [if compact { 96.0 } else { 116.0 }, 38.0],
                        egui::Button::new(turbo)
                            .fill(theme::ORANGE)
                            .sense(egui::Sense::hover()),
                    )
                    .on_hover_text("Voidspace is already running with administrator privileges");
                    if ui
                        .add_sized(
                            [if compact { 48.0 } else { 92.0 }, 38.0],
                            egui::Button::new(
                                egui::RichText::new(if compact { "BIN" } else { "EMPTY BIN" })
                                    .font(self.typography.font(theme::TypographyToken::DataCompact))
                                    .color(hud::MAGENTA),
                            )
                            .fill(hud::PANEL_RAISED),
                        )
                        .on_hover_text(
                            "Empty the Windows Recycle Bin · Windows will ask for confirmation",
                        )
                        .clicked()
                    {
                        empty_recycle_bin = true;
                    }
                    if ui
                        .add_sized(
                            [if compact { 30.0 } else { 64.0 }, 38.0],
                            egui::Button::new(
                                egui::RichText::new(if compact { "i" } else { "ABOUT" }).font(
                                    self.typography.font(theme::TypographyToken::DataCompact),
                                ),
                            )
                            .fill(hud::PANEL_RAISED),
                        )
                        .on_hover_text("ABOUT / AUTHOR")
                        .clicked()
                    {
                        open_about = true;
                    }
                });
            });
        if open_about {
            self.volume_switcher.open = false;
            if self.overlays.transient() == Some(TransientOverlay::About) {
                self.overlays.close_transient(root_ui.ctx());
            } else {
                self.overlays.open_transient(TransientOverlay::About, None);
            }
        }
        if empty_recycle_bin {
            self.volume_switcher.open = false;
            match crate::recycle_bin::empty_with_windows_confirmation() {
                Ok(()) => self.toast = Some("Recycle Bin emptied".to_owned()),
                Err(error) => self.toast = Some(error),
            }
        }
        if open_filter {
            self.volume_switcher.open = false;
            self.compact_filter_prior.clone_from(&self.filter_text);
            self.compact_filter_draft.clone_from(&self.filter_text);
            self.overlays
                .open_transient(TransientOverlay::CompactFilter, None);
        }
        match switcher_action {
            VolumeSwitcherAction::None => {}
            VolumeSwitcherAction::Close => {
                self.volume_switcher.open = false;
                if self.overlays.transient() == Some(TransientOverlay::DiskPicker) {
                    self.overlays.dismiss_transient();
                }
            }
            VolumeSwitcherAction::OpenOrActivate(path) => {
                self.volume_switcher.open = false;
                if self.overlays.transient() == Some(TransientOverlay::DiskPicker) {
                    self.overlays.dismiss_transient();
                }
                self.open_or_activate_scan(path);
            }
        }
        if self.volume_switcher.open && !was_switcher_open {
            self.tactical_arc = None;
            self.overlays
                .open_transient(TransientOverlay::DiskPicker, None);
        }
        if self.volume_switcher.open
            && (!was_switcher_open
                || self.volume_refresh_started_at.elapsed() >= VOLUME_REFRESH_INTERVAL)
        {
            self.request_volume_refresh(root_ui.ctx());
        }
    }

    fn reparse_filter(&mut self) {
        if self.filter_text.trim().is_empty() {
            self.filter = None;
            self.filter_error = None;
            return;
        }
        match parse(&self.filter_text) {
            Ok(filter) => {
                self.filter = Some(filter);
                self.filter_error = None;
            }
            Err(error) => self.filter_error = Some(error.to_string()),
        }
    }

    fn apply_compact_filter(&mut self) -> bool {
        if self.compact_filter_draft.trim().is_empty() {
            self.filter_text.clear();
            self.filter = None;
            self.filter_error = None;
            return true;
        }
        match parse(&self.compact_filter_draft) {
            Ok(filter) => {
                self.filter_text.clone_from(&self.compact_filter_draft);
                self.filter = Some(filter);
                self.filter_error = None;
                true
            }
            Err(error) => {
                self.filter_error = Some(error.to_string());
                false
            }
        }
    }

    fn save_artifact(&mut self, action: ArtifactAction) {
        let snapshot = self.tabs[self.active_tab].snapshot.clone();
        let (extension, description) = match action {
            ArtifactAction::Snapshot => ("voidspace", "Voidspace snapshot"),
            ArtifactAction::Report(ReportFormat::Csv) => ("csv", "CSV report"),
            ArtifactAction::Report(ReportFormat::Json) => ("json", "JSON report"),
            ArtifactAction::Report(ReportFormat::Html) => ("html", "HTML report"),
            ArtifactAction::Report(ReportFormat::Text) => ("txt", "Text report"),
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter(description, &[extension])
            .set_file_name(format!("voidspace-report.{extension}"))
            .save_file()
        else {
            return;
        };
        let result = match action {
            ArtifactAction::Snapshot => save_snapshot(&path, &snapshot),
            ArtifactAction::Report(format) => export_report(&path, &snapshot, format),
        };
        match result {
            Ok(()) => self.toast = Some(format!("Saved {}", path.display())),
            Err(error) => {
                let _ = crate::diagnostics::log_line(&format!("artifact save failed: {error}"));
                self.toast = Some(error.to_string());
            }
        }
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self
            .tactical_arc
            .as_ref()
            .is_some_and(|arc| arc.target().scan_id == self.tabs[index].snapshot.scan_id)
        {
            self.tactical_arc = None;
            self.overlays.dismiss_transient();
            self.toast = Some("TARGET CHANGED · OPEN AGAIN".to_owned());
        }
        let previous_len = self.tabs.len();
        let mut tab = self.tabs.remove(index);
        if let Some(scan) = tab.scan.take() {
            scan.cancel();
        }
        if let Some(watcher) = tab.watcher.take() {
            watcher.stop();
        }
        self.active_tab =
            active_index_after_close(previous_len, self.active_tab, index).unwrap_or(0);
        if let Some(active) = self.tabs.get(self.active_tab) {
            self.scope_text = active.root_path.display().to_string();
        } else {
            self.scope_text.clear();
            self.details_drawer = false;
        }
    }

    fn tab_bar(&mut self, root_ui: &mut egui::Ui) {
        let interactive = !self.overlays.owns_pointer();
        let mut close_requested = None;
        egui::Panel::top("tabs")
            .exact_size(42.0)
            .frame(
                egui::Frame::new()
                    .fill(hud::PANEL)
                    .inner_margin(egui::Margin::symmetric(10, 2))
                    .stroke(egui::Stroke::new(1.0, hud::HAIRLINE)),
            )
            .show(root_ui, |ui| {
                if !interactive {
                    ui.disable();
                }
                egui::ScrollArea::horizontal()
                    .id_salt("volume-tabs-overflow")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (index, tab) in self.tabs.iter().enumerate() {
                                let active = index == self.active_tab;
                                let volume_id = self.volume_labels.label_for(&tab.root_path);
                                let tab_state = if !tab.errors.is_empty() {
                                    ("ERROR", hud::HudState::Danger)
                                } else if tab.scanning {
                                    ("SCANNING", hud::HudState::Warning)
                                } else {
                                    ("LIVE", hud::HudState::Active)
                                };
                                let label =
                                    format!("{} / {} · {}", volume_id, tab.title, tab_state.0);
                                let width =
                                    (label.chars().count() as f32 * 7.4 + 24.0).clamp(82.0, 230.0);
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let fill = if active {
                                    hud::PANEL_RAISED
                                } else {
                                    hud::PANEL
                                };
                                let response = ui.add_sized(
                                    [width, 34.0],
                                    egui::Button::new(
                                        egui::RichText::new(label)
                                            .font(
                                                self.typography
                                                    .font(theme::TypographyToken::UiControl),
                                            )
                                            .color(if active { theme::TEXT } else { theme::MUTED }),
                                    )
                                    .fill(fill)
                                    .stroke(egui::Stroke::NONE),
                                );
                                hud::paint_state_square(
                                    ui.painter(),
                                    egui::pos2(
                                        response.rect.left() + 5.0,
                                        response.rect.center().y - 3.0,
                                    ),
                                    tab_state.1,
                                );
                                let close = ui
                                    .add_sized(
                                        [30.0, 34.0],
                                        egui::Button::new(
                                            egui::RichText::new("×").size(18.0).color(if active {
                                                theme::TEXT
                                            } else {
                                                theme::MUTED
                                            }),
                                        )
                                        .fill(fill)
                                        .stroke(egui::Stroke::NONE),
                                    )
                                    .on_hover_text(format!("Close {}", tab.title));
                                if active {
                                    ui.painter().line_segment(
                                        [response.rect.left_bottom(), close.rect.right_bottom()],
                                        egui::Stroke::new(2.0, hud::ORANGE),
                                    );
                                }
                                if response.clicked() {
                                    if self.overlays.transient()
                                        == Some(TransientOverlay::TacticalArc)
                                    {
                                        self.tactical_arc = None;
                                        self.toast = Some("TARGET CHANGED · OPEN AGAIN".to_owned());
                                    }
                                    self.overlays.dismiss_transient();
                                    self.active_tab = index;
                                    self.volume_picker_visible = false;
                                }
                                if close.clicked() {
                                    close_requested = Some(index);
                                }
                                ui.add_space(4.0);
                            }
                        })
                    });
            });
        if let Some(index) = close_requested {
            self.close_tab(index);
        }
    }

    fn inspector(ui: &mut egui::Ui, tab: &mut ScanTab) -> Option<InspectorAction> {
        let mut action = None;
        let mut treemap_action = None;
        let mut navigation = NavigationIntent::None;
        ui.label(
            egui::RichText::new("OBJECT / 01")
                .monospace()
                .size(10.0)
                .color(hud::CYAN),
        );
        ui.add_space(8.0);
        let selected = tab
            .treemap_state
            .selected
            .unwrap_or_else(|| tab.treemap_state.view_path.current());
        if let Some(node) = tab.snapshot.node(selected) {
            ui.label(
                egui::RichText::new(node.name.display_escaped())
                    .size(15.0)
                    .strong()
                    .color(theme::TEXT),
            );
            let selected_path = path_for_node(tab, selected);
            ui.label(
                egui::RichText::new(selected_path.display().to_string())
                    .size(12.0)
                    .color(theme::MUTED),
            );
            ui.add_space(18.0);
            let is_scan_root = selected == tab.snapshot.root;
            for (label, value) in [
                (
                    if is_scan_root && tab.scanning {
                        "Indexed so far"
                    } else {
                        "Indexed"
                    },
                    treemap::format_bytes(node.allocated),
                ),
                ("Logical", treemap::format_bytes(node.logical)),
                ("Children", node.children.len().to_string()),
            ] {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.monospace(value);
                    });
                });
                ui.add_space(4.0);
            }
            if is_scan_root && let Some(usage) = tab.volume_usage {
                ui.add_space(14.0);
                ui.label(
                    egui::RichText::new("DISK / WINDOWS")
                        .monospace()
                        .size(10.0)
                        .color(theme::MUTED),
                );
                ui.add_space(4.0);
                for (label, value) in [
                    ("Used", volume::format_decimal_bytes(usage.used())),
                    ("Free", volume::format_decimal_bytes(usage.free)),
                    ("Capacity", volume::format_decimal_bytes(usage.total)),
                ] {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.monospace(value);
                        });
                    });
                    ui.add_space(4.0);
                }
            }
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new("NAVIGATION")
                    .monospace()
                    .size(10.0)
                    .color(theme::MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!node.children.is_empty(), egui::Button::new("ZOOM INTO"))
                    .clicked()
                {
                    treemap_action = Some(TreemapAction::Zoom(selected));
                }
                if ui.add(egui::Button::new("BACK")).clicked() {
                    navigation = NavigationIntent::Back;
                }
            });
            ui.add_space(16.0);
            ui.separator();
            ui.label(
                egui::RichText::new("SCAN CONTROLS")
                    .monospace()
                    .size(10.0)
                    .color(theme::MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let pause_label = if tab.paused { "RESUME" } else { "PAUSE" };
                if ui
                    .add_enabled(tab.scanning, egui::Button::new(pause_label))
                    .clicked()
                    && let Some(scan) = &tab.scan
                {
                    if tab.paused {
                        scan.resume();
                    } else {
                        scan.pause();
                    }
                    tab.paused = !tab.paused;
                }
                if ui.button("RESCAN").clicked() {
                    action = Some(InspectorAction::Rescan);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("SNAPSHOT").clicked() {
                    action = Some(InspectorAction::Snapshot);
                }
                ui.menu_button("EXPORT", |ui| {
                    for (label, format) in [
                        ("CSV", ReportFormat::Csv),
                        ("JSON", ReportFormat::Json),
                        ("HTML", ReportFormat::Html),
                        ("TEXT", ReportFormat::Text),
                    ] {
                        if ui.button(label).clicked() {
                            action = Some(InspectorAction::Report(format));
                            ui.close();
                        }
                    }
                });
            });
            ui.add_space(16.0);
            ui.separator();
            ui.label(
                egui::RichText::new("FILE ACTIONS")
                    .monospace()
                    .size(10.0)
                    .color(theme::MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("EXPLORER").clicked() {
                    action = Some(InspectorAction::Reveal(selected_path.clone()));
                }
                if ui.button("COPY PATH").clicked() {
                    ui.ctx().copy_text(selected_path.display().to_string());
                }
            });
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Click a folder to keep drilling · double-click to zoom")
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.add_space(10.0);
            if let Some(group) = &tab.treemap_state.aggregate {
                ui.separator();
                ui.label(
                    egui::RichText::new("OTHER · SMALL ITEMS")
                        .strong()
                        .color(theme::ORANGE),
                );
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for child_id in &group.members {
                            if let Some(child) = tab.snapshot.node(*child_id) {
                                ui.horizontal(|ui| {
                                    ui.label(child.name.display_escaped());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.monospace(treemap::format_bytes(child.allocated));
                                        },
                                    );
                                });
                            }
                        }
                    });
            }
        }
        if !tab.errors.is_empty() {
            ui.add_space(18.0);
            ui.label(egui::RichText::new("ISSUES").color(theme::ORANGE).strong());
            for error in tab.errors.iter().rev().take(4) {
                ui.label(egui::RichText::new(error).small().color(theme::MUTED));
            }
        }
        if let Some(treemap_action) = treemap_action {
            tab.apply_treemap_action(treemap_action);
        }
        if apply_navigation(tab, navigation) {
            action = Some(InspectorAction::ShowVolumePicker);
        }
        action
    }

    fn workspace(&mut self, root_ui: &mut egui::Ui) {
        if self.tabs.is_empty() || self.volume_picker_visible {
            let mut scan_root = None;
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::BG))
                .show(root_ui, |ui| {
                    if self.overlays.owns_pointer() {
                        ui.disable();
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_space(42.0);
                        let available_width = ui.available_width();
                        let content_width = available_width.min(VOLUME_GRID_MAX_WIDTH);
                        let side_space = ((available_width - content_width) / 2.0).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add_space(side_space);
                            ui.vertical(|ui| {
                                ui.set_width(content_width);
                                ui.label(
                                    egui::RichText::new("STORAGE / WINDOWS")
                                        .font(
                                            self.typography
                                                .font(theme::TypographyToken::DataCompact),
                                        )
                                        .color(theme::ORANGE),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new("CHOOSE A VOLUME")
                                        .font(
                                            self.typography
                                                .font(theme::TypographyToken::DisplayView),
                                        )
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    egui::RichText::new(
                                        "Select a disk to start scanning. Mounted volumes refresh automatically.",
                                    )
                                    .font(
                                        self.typography.font(theme::TypographyToken::UiBody),
                                    )
                                    .color(theme::MUTED),
                                );
                                ui.add_space(26.0);

                                if self.available_volumes.is_empty() {
                                    let (title, detail) = if !self.volume_discovery_complete {
                                        (
                                            "DETECTING VOLUMES",
                                            "Reading mounted Windows disks…",
                                        )
                                    } else if self.volume_discovery_error.is_some() {
                                        (
                                            "VOLUME DISCOVERY FAILED",
                                            "Use the path field above to scan a disk or folder.",
                                        )
                                    } else {
                                        (
                                            "NO READY VOLUMES",
                                            "Use the path field above to scan a disk or folder.",
                                        )
                                    };
                                    empty_volume_message(ui, title, detail);
                                } else {
                                    let columns = volume_grid_columns(content_width);
                                    let card_width = (content_width
                                        - VOLUME_CARD_GAP * (columns.saturating_sub(1) as f32))
                                        / columns as f32;
                                    let volume_ids = self
                                        .available_volumes
                                        .iter()
                                        .map(|volume| {
                                            self.volume_labels.label_for(&volume.root_path)
                                        })
                                        .collect::<Vec<_>>();
                                    let volume_states = self
                                        .available_volumes
                                        .iter()
                                        .map(|volume| {
                                            volume_switcher::matching_volume_tab_index(
                                                self.tabs.iter().map(|tab| tab.root_path.as_path()),
                                                &volume.root_path,
                                            )
                                            .map_or("START SCAN", |index| {
                                                let tab = &self.tabs[index];
                                                if !tab.errors.is_empty() {
                                                    "ERROR"
                                                } else if tab.scanning {
                                                    "SCANNING"
                                                } else {
                                                    "OPEN TAB"
                                                }
                                            })
                                        })
                                        .collect::<Vec<_>>();
                                    egui::Grid::new("volume_picker_grid")
                                        .num_columns(columns)
                                        .spacing([VOLUME_CARD_GAP, VOLUME_CARD_GAP])
                                        .show(ui, |ui| {
                                            for (index, ((volume, volume_id), volume_state)) in self
                                                .available_volumes
                                                .iter()
                                                .zip(&volume_ids)
                                                .zip(&volume_states)
                                                .enumerate()
                                            {
                                                if volume_card(
                                                    ui,
                                                    volume,
                                                    volume_id,
                                                    volume_state,
                                                    card_width,
                                                ) {
                                                    scan_root = Some(volume.root_path.clone());
                                                }
                                                if (index + 1) % columns == 0 {
                                                    ui.end_row();
                                                }
                                            }
                                        });
                                    if let Some(error) = &self.volume_discovery_error {
                                        ui.add_space(10.0);
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "LAST REFRESH ISSUE · {error}"
                                            ))
                                            .monospace()
                                            .size(10.0)
                                            .color(theme::MUTED),
                                        );
                                    }
                                }
                                ui.add_space(22.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Need a folder instead? Enter its path in the field above and press Enter.",
                                    )
                                    .size(12.0)
                                    .color(theme::MUTED),
                                );
                            });
                        });
                    });
                });
            if let Some(root) = scan_root {
                self.open_or_activate_scan(root);
            }
            return;
        }

        let context = root_ui.ctx().clone();
        let mode = match ShellLayout::for_width(root_ui.max_rect().width()).inspector {
            InspectorPlacement::Docked => WorkspaceMode::Docked,
            InspectorPlacement::Drawer => WorkspaceMode::DrawerClosed,
        };
        if self.details_drawer
            && self.overlays.transient() != Some(TransientOverlay::InspectorDrawer)
        {
            self.details_drawer = false;
        }
        let suppress_treemap = self.overlays.owns_pointer();
        let inspector_enabled = !self.overlays.owns_pointer();
        let tab = &mut self.tabs[self.active_tab];
        let mut inspector_action = None;
        let mut pending_context_target = None;
        let mut tactical_work_area = None;
        let mut open_inspector_drawer = false;
        if mode == WorkspaceMode::Docked {
            egui::Panel::right("inspector")
                .exact_size(300.0)
                .frame(
                    egui::Frame::new()
                        .fill(hud::PANEL)
                        .inner_margin(16.0)
                        .stroke(egui::Stroke::new(1.0, hud::HAIRLINE)),
                )
                .show(root_ui, |ui| {
                    ui.add_enabled_ui(inspector_enabled, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            inspector_action = Self::inspector(ui, tab);
                        });
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::MAP_BG).inner_margin(8.0))
            .show(root_ui, |ui| {
                if !suppress_treemap
                    && mode == WorkspaceMode::DrawerClosed
                    && ui.button("DETAILS").clicked()
                {
                    open_inspector_drawer = true;
                }
                let mut show_volume_picker = false;
                if !suppress_treemap
                    && ui.ctx().input_mut(|input| {
                        input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)
                    })
                {
                    show_volume_picker |= apply_navigation(tab, NavigationIntent::Back);
                }
                let navigation = treemap_navigation(ui, tab);
                if !suppress_treemap {
                    show_volume_picker |= apply_navigation(tab, navigation);
                }
                ui.separator();
                let aggregate_view = tab.treemap_state.aggregate_views.last().cloned();
                let view_root = aggregate_view.as_ref().map_or_else(
                    || tab.treemap_state.view_path.current(),
                    |group| group.parent,
                );
                let available = ui.available_rect_before_wrap();
                let metrics = treemap::candidate_metrics(
                    ui,
                    &tab.snapshot,
                    view_root,
                    1024,
                    &self.typography,
                );
                let view = ViewState {
                    root: view_root,
                    bounds: LayoutRect::new(
                        available.left(),
                        available.top(),
                        available.right(),
                        available.bottom(),
                    ),
                    size_mode: SizeMode::Allocated,
                    max_depth: 1,
                    min_area: 196.0,
                    min_label: metrics.footprint(),
                    max_rectangles: 1024,
                };
                tab.layout = if let Some(group) = aggregate_view.as_ref() {
                    layout_subset(&tab.snapshot, &view, &group.members, &DirtySet::default())
                } else {
                    layout(&tab.snapshot, &view, &DirtySet::default())
                };
                let pin_is_eligible = tab.treemap_state.pinned.is_some_and(|pinned| {
                    treemap::preview_chain(view_root, pinned, |id| {
                        tab.snapshot.node(id).and_then(|node| node.parent)
                    })
                    .is_some()
                        && tab
                            .snapshot
                            .node(pinned)
                            .is_some_and(|entry| !entry.children.is_empty())
                });
                if tab.treemap_state.pinned.is_some() && !pin_is_eligible {
                    tab.treemap_state.pinned = None;
                }
                let preview = treemap::PreviewState {
                    pinned: tab.treemap_state.pinned,
                };
                let response = treemap::show(
                    ui,
                    treemap::ShowRequest {
                        snapshot: &tab.snapshot,
                        base_layout: &tab.layout,
                        selected: tab.treemap_state.selected,
                        filter: self.filter.as_ref(),
                        preview,
                        typography: &self.typography,
                        base_metrics: &metrics,
                        open_aggregate: tab.treemap_state.aggregate.as_ref(),
                        interactive: !suppress_treemap,
                    },
                );
                if !response.aggregate_still_valid {
                    tab.treemap_state.aggregate = None;
                }
                if let Some((target, origin_focus, keyboard_open, source_rect)) =
                    response.context_target
                {
                    tab.treemap_state.selected = Some(target);
                    tab.treemap_state.aggregate = None;
                    if let Some(node) = tab.snapshot.node(target) {
                        pending_context_target = Some((
                            ContextTarget {
                                scan_id: tab.snapshot.scan_id,
                                generation: tab.snapshot.generation,
                                node_id: target,
                                identity: node.identity.clone(),
                                path: path_for_node(tab, target),
                                kind: node.kind,
                                root: tab.root_path.clone(),
                                view_root,
                                display_name: node.name.display_escaped(),
                                display_size: treemap::compact_bytes(node.allocated),
                                origin_focus,
                            },
                            source_rect,
                        ));
                        tactical_work_area = Some((available, keyboard_open));
                    }
                }
                if let Some(action) = response.action {
                    tab.apply_treemap_action(action);
                }
                if show_volume_picker {
                    inspector_action = Some(InspectorAction::ShowVolumePicker);
                }
            });

        if open_inspector_drawer {
            self.details_drawer = true;
            self.overlays
                .open_transient(TransientOverlay::InspectorDrawer, None);
        }

        if self.details_drawer {
            let mut open = true;
            egui::Window::new("DETAILS")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_TOP, [-10.0, 104.0])
                .fixed_size([340.0, 460.0])
                .frame(
                    egui::Frame::new()
                        .fill(hud::PANEL)
                        .stroke(egui::Stroke::new(1.0, hud::CYAN))
                        .inner_margin(egui::Margin::same(16)),
                )
                .show(&context, |ui| {
                    inspector_action = Self::inspector(ui, tab);
                });
            self.details_drawer = open;
            if !open {
                self.overlays.close_transient(&context);
            }
        }
        if let Some((target, source_rect)) = pending_context_target {
            let pointer = context
                .input(|input| input.pointer.interact_pos())
                .unwrap_or_else(|| context.content_rect().center());
            let origin_focus = target.origin_focus;
            let (work_area, keyboard_open) =
                tactical_work_area.unwrap_or((context.content_rect(), false));
            if let Some(arc) = TacticalArcState::new_with_source_rect(
                target,
                pointer,
                work_area,
                keyboard_open,
                source_rect,
            ) {
                self.tactical_arc = Some(arc);
                self.overlays
                    .open_transient(TransientOverlay::TacticalArc, Some(origin_focus));
            } else {
                self.toast = Some("WINDOW TOO SMALL FOR COMMAND MENU".to_owned());
            }
        }
        if let Some(action) = inspector_action {
            self.handle_inspector_action(action);
        }
    }

    fn handle_inspector_action(&mut self, action: InspectorAction) {
        match action {
            InspectorAction::Reveal(path) => {
                if let Err(error) = reveal_in_explorer(&path) {
                    self.toast = Some(error.to_string());
                }
            }
            InspectorAction::Recycle(path) => self.prepare_fileop(path, OperationKind::Recycle),
            InspectorAction::Permanent(path) => {
                self.prepare_fileop(path, OperationKind::Permanent);
            }
            InspectorAction::Rescan => self.restart_tab(self.active_tab),
            InspectorAction::Snapshot => self.save_artifact(ArtifactAction::Snapshot),
            InspectorAction::Report(format) => {
                self.save_artifact(ArtifactAction::Report(format));
            }
            InspectorAction::ShowVolumePicker => {
                self.volume_picker_visible = true;
                self.details_drawer = false;
                self.overlays.dismiss_transient();
            }
        }
    }

    fn prepare_fileop(&mut self, path: PathBuf, kind: OperationKind) {
        match prepare(OperationDraft {
            kind,
            paths: vec![path],
        }) {
            Ok(operation) => match delete_dispatch(kind) {
                DeleteDispatch::Immediate => self.execute_fileop(operation, ""),
                DeleteDispatch::Confirm => {
                    self.overlays.open_modal(ModalOverlay::PermanentDelete);
                    self.fileop_dialog = Some(FileOpDialog {
                        operation,
                        phrase: String::new(),
                    });
                }
            },
            Err(error) => self.toast = Some(error.to_string()),
        }
    }

    fn execute_fileop(&mut self, operation: ConfirmableOperation, phrase: &str) {
        match confirm(operation, phrase) {
            Ok(operation) => {
                let sink = self.fileop_tx.clone();
                self.fileop_running = true;
                std::thread::spawn(move || {
                    let result = execute(operation, None, &CancellationToken::default())
                        .map_err(|error| error.to_string());
                    let _ = sink.send(result);
                });
            }
            Err(error) => self.toast = Some(error.to_string()),
        }
    }

    fn fileop_dialog(&mut self, context: &egui::Context) {
        let Some(dialog) = &mut self.fileop_dialog else {
            return;
        };
        debug_assert_eq!(self.overlays.modal(), Some(ModalOverlay::PermanentDelete));
        let permanent = dialog.operation.kind == OperationKind::Permanent;
        let mut execute_now = false;
        let mut cancel = context.input(|input| input.key_pressed(egui::Key::Escape));
        egui::Window::new(if permanent {
            "DELETE FOREVER"
        } else {
            "MOVE TO RECYCLE BIN"
        })
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(context, |ui| {
            ui.set_min_width(460.0);
            ui.label(format!(
                "{} object(s) · {}",
                dialog.operation.manifest.len(),
                treemap::format_bytes(dialog.operation.total_bytes)
            ));
            ui.label(
                egui::RichText::new(dialog.operation.roots[0].display().to_string())
                    .small()
                    .color(theme::MUTED),
            );
            if permanent {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("This cannot be undone. Type DELETE to continue.")
                        .strong()
                        .color(theme::ORANGE),
                );
                ui.text_edit_singleline(&mut dialog.phrase);
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("CANCEL").clicked() {
                    cancel = true;
                }
                if ui
                    .add_enabled(
                        !permanent || dialog.phrase == "DELETE",
                        egui::Button::new(if permanent {
                            "DELETE FOREVER"
                        } else {
                            "RECYCLE"
                        })
                        .fill(if permanent {
                            hud::HudState::Danger.color()
                        } else {
                            theme::LIME
                        }),
                    )
                    .clicked()
                {
                    execute_now = true;
                }
            });
        });
        if cancel {
            self.fileop_dialog = None;
            self.overlays.close_modal();
        } else if execute_now && let Some(dialog) = self.fileop_dialog.take() {
            let phrase = if permanent { "DELETE" } else { "" };
            self.overlays.close_modal();
            self.execute_fileop(dialog.operation, phrase);
        }
    }

    fn tactical_target_is_current(&self, target: &ContextTarget) -> bool {
        self.tabs.get(self.active_tab).is_some_and(|tab| {
            let current_view_root = tab.treemap_state.aggregate_views.last().map_or_else(
                || tab.treemap_state.view_path.current(),
                |group| group.parent,
            );
            tab.snapshot.scan_id == target.scan_id
                && tab.snapshot.generation == target.generation
                && tab.root_path == target.root
                && current_view_root == target.view_root
                && tab.snapshot.node(target.node_id).is_some_and(|node| {
                    node.identity == target.identity
                        && node.kind == target.kind
                        && path_for_node(tab, target.node_id) == target.path
                })
        })
    }

    fn execute_tactical_action(&mut self, target: ContextTarget, action: TacticalAction) {
        if !self.tactical_target_is_current(&target) {
            self.toast = Some("TARGET CHANGED · OPEN AGAIN".to_owned());
            return;
        }
        match action {
            TacticalAction::OpenInExplorer => {
                self.handle_inspector_action(InspectorAction::Reveal(target.path));
            }
            TacticalAction::Recycle => {
                self.handle_inspector_action(InspectorAction::Recycle(target.path));
            }
            TacticalAction::DeletePermanently => {
                self.handle_inspector_action(InspectorAction::Permanent(target.path));
            }
        }
    }

    fn show_transient_overlays(
        &mut self,
        context: &egui::Context,
        tactical_arc_input_outcome: Option<TacticalArcOutcome>,
    ) {
        let transient = self.overlays.transient();
        let escape_pressed = context.input(|input| input.key_pressed(egui::Key::Escape));
        if escape_pressed && transient == Some(TransientOverlay::CompactFilter) {
            self.filter_text.clone_from(&self.compact_filter_prior);
            self.reparse_filter();
            self.overlays.close_transient(context);
            return;
        }
        let _ = self.overlays.route_escape(context);
        let just_opened = self.overlays.take_transient_just_opened();
        if self.overlays.transient() == Some(TransientOverlay::TacticalArc)
            && self
                .tactical_arc
                .as_ref()
                .is_some_and(|arc| !self.tactical_target_is_current(arc.target()))
        {
            self.tactical_arc = None;
            self.overlays.close_transient(context);
            self.toast = Some("TARGET CHANGED · OPEN AGAIN".to_owned());
            return;
        }
        if self.overlays.transient() != Some(TransientOverlay::TacticalArc) {
            self.tactical_arc = None;
        }
        match self.overlays.transient() {
            Some(TransientOverlay::TacticalArc) => {
                let chosen = self
                    .tactical_arc
                    .as_mut()
                    .and_then(|arc| {
                        arc.paint(
                            context,
                            self.typography.font(theme::TypographyToken::DataCompact),
                        )
                    })
                    .or(tactical_arc_input_outcome);
                match chosen {
                    Some(TacticalArcOutcome::Action(action)) => {
                        if let Some(arc) = self.tactical_arc.take() {
                            self.overlays.close_transient(context);
                            self.execute_tactical_action(arc.into_target(), action);
                        }
                    }
                    Some(TacticalArcOutcome::Dismiss) => {
                        self.tactical_arc = None;
                        self.overlays.close_transient(context);
                    }
                    None => {}
                }
            }
            Some(TransientOverlay::About) => {
                let mut open = true;
                let shown = egui::Window::new("ABOUT / VOIDSPACE")
                    .id(egui::Id::new("voidspace-about"))
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(hud::PANEL)
                            .stroke(egui::Stroke::new(1.0, hud::ORANGE))
                            .inner_margin(egui::Margin::same(18)),
                    )
                    .show(context, |ui| {
                        ui.set_min_width(500.0);
                        ui.label(theme::brand_wordmark(&self.typography));
                        ui.label(
                            egui::RichText::new("FAST NATIVE DISK INTELLIGENCE / WINDOWS")
                                .font(self.typography.font(theme::TypographyToken::DataCompact))
                                .color(hud::CYAN),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "VERSION {} / NATIVE RUST",
                                env!("CARGO_PKG_VERSION")
                            ))
                            .font(self.typography.font(theme::TypographyToken::DataMicro))
                            .color(theme::MUTED),
                        );
                        ui.add_space(16.0);
                        ui.label("Created by Евгений «Chip» Юрченко");
                        ui.label(
                            egui::RichText::new("AI, рынки, агенты · Человек 2.0")
                                .color(theme::MUTED),
                        );
                        ui.separator();
                        for &(label, url) in ABOUT_LINKS {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(label)
                                        .font(
                                            self.typography.font(theme::TypographyToken::DataMicro),
                                        )
                                        .color(theme::MUTED),
                                );
                                ui.hyperlink_to(url, url);
                            });
                        }
                    });
                let rect = shown.map(|response| response.response.rect);
                if !open || clicked_outside(context, rect, just_opened) {
                    self.overlays.close_transient(context);
                }
            }
            Some(TransientOverlay::CompactFilter) => {
                let mut open = true;
                let mut apply = false;
                let shown = egui::Window::new("FILTER / QUERY")
                    .id(egui::Id::new("compact-filter"))
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(hud::PANEL)
                            .stroke(egui::Stroke::new(1.0, hud::CYAN))
                            .inner_margin(egui::Margin::same(14)),
                    )
                    .show(context, |ui| {
                        let response = ui.add_sized(
                            [420.0, 38.0],
                            egui::TextEdit::singleline(&mut self.compact_filter_draft)
                                .hint_text("size > 1GiB AND NOT attr:system")
                                .font(self.typography.font(theme::TypographyToken::DataNormal)),
                        );
                        response.request_focus();
                        if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                            apply = true;
                        }
                        if let Some(error) = &self.filter_error {
                            ui.label(
                                egui::RichText::new(format!("QUERY ERROR / {error}"))
                                    .font(self.typography.font(theme::TypographyToken::DataMicro))
                                    .color(hud::ORANGE),
                            );
                        }
                    });
                let rect = shown.map(|response| response.response.rect);
                if clicked_outside(context, rect, just_opened) {
                    apply = true;
                }
                if !open {
                    self.filter_text.clone_from(&self.compact_filter_prior);
                    self.reparse_filter();
                    self.overlays.close_transient(context);
                } else if apply && self.apply_compact_filter() {
                    self.overlays.close_transient(context);
                }
            }
            Some(TransientOverlay::StatusDetails) => {
                let (open, rect) = status_bar::show_details(
                    context,
                    &self.typography,
                    &self.status_detail_modules,
                );
                if !open || clicked_outside(context, rect, just_opened) {
                    self.overlays.close_transient(context);
                }
            }
            Some(TransientOverlay::DiskPicker | TransientOverlay::InspectorDrawer) | None => {}
        }
    }

    fn status_bar(&mut self, root_ui: &mut egui::Ui) {
        let interactive = !self.overlays.owns_pointer();
        let geometry = status_bar_geometry();
        let modules = self.status_modules();
        let mut open_more = None;
        egui::Panel::bottom("status")
            .exact_size(geometry.height)
            .frame(
                egui::Frame::new()
                    .fill(hud::PANEL)
                    .stroke(egui::Stroke::new(1.0, hud::HAIRLINE))
                    .inner_margin(egui::Margin::symmetric(10, geometry.vertical_margin)),
            )
            .show(root_ui, |ui| {
                if !interactive {
                    ui.disable();
                }
                ui.set_min_height(geometry.content_height());
                open_more = status_bar::show(ui, &self.typography, &modules);
            });
        if let Some((focus, hidden)) = open_more {
            self.status_detail_modules = hidden;
            self.overlays
                .open_transient(TransientOverlay::StatusDetails, Some(focus));
        }
    }

    fn status_modules(&self) -> Vec<StatusModule> {
        let mut modules = Vec::new();
        if let Some(tab) = self.tabs.get(self.active_tab) {
            modules.push(StatusModule {
                kind: StatusKind::Scan,
                value: if tab.scanning { "SCANNING" } else { "LIVE" }.to_owned(),
                state: if tab.scanning {
                    hud::HudState::Warning
                } else {
                    hud::HudState::Active
                },
            });
            modules.push(StatusModule {
                kind: StatusKind::Entries,
                value: tab.files_seen.to_string(),
                state: hud::HudState::Neutral,
            });
            modules.push(StatusModule {
                kind: StatusKind::Indexed,
                value: treemap::format_bytes(
                    tab.snapshot
                        .node(tab.snapshot.root)
                        .map_or(0, |node| node.allocated),
                ),
                state: hud::HudState::Neutral,
            });
            if let Some(usage) = tab.volume_usage {
                modules.push(StatusModule {
                    kind: StatusKind::DiskUsed,
                    value: format!(
                        "{} / {}",
                        volume::format_decimal_bytes(usage.used()),
                        volume::format_decimal_bytes(usage.total)
                    ),
                    state: hud::HudState::Neutral,
                });
            }
            if tab
                .watcher
                .as_ref()
                .is_some_and(|watcher| watcher.health().overflowed)
            {
                modules.push(StatusModule {
                    kind: StatusKind::Watch,
                    value: "RESYNC".to_owned(),
                    state: hud::HudState::Warning,
                });
            }
        } else {
            modules.push(StatusModule {
                kind: StatusKind::Scan,
                value: "READY".to_owned(),
                state: hud::HudState::Neutral,
            });
        }
        modules.push(StatusModule {
            kind: StatusKind::Engine,
            value: "TURBO ACTIVE".to_owned(),
            state: hud::HudState::Active,
        });
        if self.fileop_running {
            modules.push(StatusModule {
                kind: StatusKind::FileOp,
                value: "RUNNING".to_owned(),
                state: hud::HudState::Warning,
            });
        }
        if let Some(error) = &self.filter_error {
            modules.push(StatusModule {
                kind: StatusKind::Filter,
                value: error.clone(),
                state: hud::HudState::Warning,
            });
        }
        if let Some(toast) = &self.toast {
            modules.push(StatusModule {
                kind: StatusKind::Notice,
                value: toast.clone(),
                state: hud::HudState::Warning,
            });
        }
        modules
    }

    fn show_passive_toast(&mut self, context: &egui::Context) {
        if !self.overlays.allows_passive_toast() {
            return;
        }
        let Some(message) = self.toast.clone() else {
            return;
        };
        let mut dismiss = false;
        egui::Area::new(egui::Id::new("voidspace-passive-toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -64.0])
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(hud::PANEL_RAISED)
                    .stroke(egui::Stroke::new(1.0, hud::ORANGE))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.horizontal(|ui| {
                            hud::paint_state_square(
                                ui.painter(),
                                ui.cursor().left_top() + egui::vec2(0.0, 5.0),
                                hud::HudState::Warning,
                            );
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new(format!("NOTICE / {message}"))
                                    .font(self.typography.font(theme::TypographyToken::DataCompact))
                                    .color(theme::TEXT),
                            );
                            if ui.button("×").on_hover_text("Dismiss notice").clicked() {
                                dismiss = true;
                            }
                        });
                    });
            });
        if dismiss {
            self.toast = None;
        }
    }
}

fn apply_volume_refresh(
    cache: &mut Vec<volume::VolumeInfo>,
    error: &mut Option<String>,
    result: Result<Vec<volume::VolumeInfo>, String>,
) {
    match result {
        Ok(volumes) => {
            *cache = volumes;
            *error = None;
        }
        Err(refresh_error) => *error = Some(refresh_error),
    }
}

fn clicked_outside(context: &egui::Context, rect: Option<egui::Rect>, just_opened: bool) -> bool {
    if just_opened {
        return false;
    }
    context.input(|input| {
        input.pointer.primary_clicked()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|position| rect.is_none_or(|rect| !rect.contains(position)))
    })
}

fn treemap_navigation(ui: &mut egui::Ui, tab: &ScanTab) -> NavigationIntent {
    let mut intent = NavigationIntent::None;
    let path = tab.treemap_state.view_path.as_slice();
    let available_for_segments = (ui.available_width() - 116.0).max(160.0);
    let labels = path
        .iter()
        .map(|node_id| {
            tab.snapshot
                .node(*node_id)
                .map(|node| truncate_volume_label(&node.name.display_escaped(), 18))
                .unwrap_or_else(|| "?".into())
        })
        .collect::<Vec<_>>();
    let label_widths = labels
        .iter()
        .map(|label| {
            ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(label.clone(), egui::FontId::monospace(10.0), theme::TEXT)
                    .size()
                    .x
            })
        })
        .collect::<Vec<_>>();
    let visible = collapsed_breadcrumb_for_width(&label_widths, available_for_segments);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("NAV / PATH")
                .monospace()
                .size(9.0)
                .color(hud::CYAN),
        );
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("← LVL-1")
                        .monospace()
                        .size(10.0)
                        .color(theme::TEXT),
                )
                .fill(hud::PANEL_RAISED)
                .stroke(egui::Stroke::new(1.0, hud::HAIRLINE)),
            )
            .clicked()
        {
            intent = NavigationIntent::Back;
        }
        for (slot, index) in visible.into_iter().enumerate() {
            if slot > 0 {
                ui.label(egui::RichText::new("//").color(theme::MUTED));
            }
            let Some(index) = index else {
                ui.label(egui::RichText::new("…").monospace().color(theme::MUTED));
                continue;
            };
            let node_id = path[index];
            let label = &labels[index];
            let current = index + 1 == path.len();
            let response = ui.add(
                egui::Button::new(
                    egui::RichText::new(label)
                        .monospace()
                        .size(10.0)
                        .color(if current { hud::ORANGE } else { theme::TEXT })
                        .strong(),
                )
                .fill(hud::PANEL_RAISED)
                .stroke(egui::Stroke::new(1.0, hud::HAIRLINE)),
            );
            if response.clicked() {
                intent = NavigationIntent::Jump(node_id);
            }
        }
        for group in &tab.treemap_state.aggregate_views {
            ui.label(egui::RichText::new("›").color(theme::MUTED));
            ui.label(
                egui::RichText::new(format!("OTHER · {}", group.members.len()))
                    .color(theme::ORANGE)
                    .strong(),
            );
        }
    });
    intent
}

fn collapsed_breadcrumb_indices(length: usize, max_visible: usize) -> Vec<Option<usize>> {
    if length <= max_visible || length <= 2 {
        return (0..length).map(Some).collect();
    }
    let tail_count = max_visible.saturating_sub(1).max(1);
    let mut result = Vec::with_capacity(tail_count + 2);
    result.push(Some(0));
    result.push(None);
    result.extend((length - tail_count..length).map(Some));
    result
}

fn collapsed_breadcrumb_for_width(widths: &[f32], available_width: f32) -> Vec<Option<usize>> {
    let maximum = widths.len().clamp(2, 6);
    for visible_count in (2..=maximum).rev() {
        let indices = collapsed_breadcrumb_indices(widths.len(), visible_count);
        let separators = indices.len().saturating_sub(1) as f32 * 22.0;
        let cells = indices
            .iter()
            .map(|index| index.map_or(24.0, |index| widths[index] + 20.0))
            .sum::<f32>();
        if cells + separators <= available_width {
            return indices;
        }
    }
    collapsed_breadcrumb_indices(widths.len(), 2)
}

fn back_destination(view_path_len: usize, aggregate_views_len: usize) -> BackDestination {
    if view_path_len > 1 || aggregate_views_len > 0 {
        BackDestination::Parent
    } else {
        BackDestination::VolumePicker
    }
}

fn apply_navigation(tab: &mut ScanTab, intent: NavigationIntent) -> bool {
    if intent == NavigationIntent::Back
        && back_destination(
            tab.treemap_state.view_path.as_slice().len(),
            tab.treemap_state.aggregate_views.len(),
        ) == BackDestination::VolumePicker
    {
        return true;
    }
    let target = match intent {
        NavigationIntent::None => None,
        NavigationIntent::Back => tab.treemap_state.back(),
        NavigationIntent::Jump(node_id) => tab.treemap_state.jump_to(node_id),
    };
    if let Some(target) = target {
        tab.treemap_state.selected = Some(target);
        tab.treemap_state.pinned = None;
        tab.treemap_state.aggregate = None;
    }
    false
}

fn volume_grid_columns(available_width: f32) -> usize {
    (((available_width + VOLUME_CARD_GAP) / (VOLUME_CARD_MIN_WIDTH + VOLUME_CARD_GAP)).floor()
        as usize)
        .clamp(1, 4)
}

fn active_index_after_close(tab_count: usize, active: usize, closed: usize) -> Option<usize> {
    if tab_count == 0 || closed >= tab_count {
        return None;
    }
    let remaining = tab_count - 1;
    if remaining == 0 {
        return None;
    }
    Some(if active > closed {
        active - 1
    } else if active == closed {
        closed.min(remaining - 1)
    } else {
        active.min(remaining - 1)
    })
}

fn truncate_volume_label(label: &str, max_characters: usize) -> String {
    if label.chars().count() <= max_characters {
        return label.to_owned();
    }
    let visible = max_characters.saturating_sub(1);
    format!("{}…", label.chars().take(visible).collect::<String>())
}

fn empty_volume_message(ui: &mut egui::Ui, title: &str, detail: &str) {
    egui::Frame::new()
        .fill(hud::PANEL)
        .stroke(egui::Stroke::new(1.0, hud::CYAN))
        .inner_margin(egui::Margin::same(22))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .monospace()
                    .strong()
                    .color(theme::TEXT),
            );
            ui.label(egui::RichText::new(detail).color(theme::MUTED));
        });
}

fn volume_card(
    ui: &mut egui::Ui,
    volume: &volume::VolumeInfo,
    volume_id: &str,
    volume_state: &str,
    width: f32,
) -> bool {
    use egui::{Align2, FontFamily, FontId, Sense, WidgetInfo, WidgetType};

    let (mut response, painter) =
        ui.allocate_painter(egui::vec2(width, VOLUME_CARD_HEIGHT), Sense::click());
    let keyboard_activated = response.has_focus()
        && ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
        });
    if keyboard_activated {
        response.mark_changed();
    }

    let used = volume.usage.used();
    let percentage = volume::used_percentage(volume.usage);
    let accessible_label = format!(
        "{volume_id}, {} {}, total {}, free {}, {volume_state}",
        volume.display_root,
        volume.label,
        volume::format_decimal_bytes(volume.usage.total),
        volume::format_decimal_bytes(volume.usage.free),
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &accessible_label));

    let rect = response.rect;
    let highlighted = response.hovered() || response.has_focus();
    let fill = if response.is_pointer_button_down_on() {
        egui::Color32::from_rgb(27, 23, 21)
    } else if highlighted {
        hud::PANEL_RAISED
    } else {
        hud::PANEL
    };
    hud::paint_cut_frame(
        &painter,
        rect,
        fill,
        egui::Stroke::new(
            if response.has_focus() { 2.0 } else { 1.0 },
            if highlighted {
                hud::ORANGE
            } else {
                hud::HAIRLINE
            },
        ),
        10.0,
    );
    hud::paint_state_square(
        &painter,
        rect.left_top() + egui::vec2(8.0, 8.0),
        if percentage >= 90 {
            hud::HudState::Warning
        } else {
            hud::HudState::Active
        },
    );
    painter.text(
        rect.left_top() + egui::vec2(18.0, 8.0),
        Align2::LEFT_TOP,
        volume_id,
        FontId::new(9.0, FontFamily::Monospace),
        hud::CYAN,
    );

    let inner = rect.shrink(18.0);
    let proportional = |size| FontId::new(size, FontFamily::Proportional);
    let monospace = |size| FontId::new(size, FontFamily::Monospace);
    painter.text(
        inner.left_top(),
        Align2::LEFT_TOP,
        &volume.display_root,
        monospace(20.0),
        hud::ORANGE,
    );
    painter.text(
        inner.right_top(),
        Align2::RIGHT_TOP,
        volume::format_decimal_bytes(volume.usage.total),
        monospace(14.0),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(inner.right(), inner.top() + 20.0),
        Align2::RIGHT_TOP,
        "TOTAL",
        monospace(9.0),
        theme::MUTED,
    );

    let max_label_characters = ((inner.width() / 7.5).floor() as usize).clamp(12, 42);
    painter.text(
        egui::pos2(inner.left(), inner.top() + 37.0),
        Align2::LEFT_TOP,
        truncate_volume_label(&volume.label, max_label_characters),
        proportional(12.0),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(inner.left(), inner.top() + 61.0),
        Align2::LEFT_TOP,
        format!("{percentage}% USED"),
        monospace(10.0),
        theme::MUTED,
    );
    painter.text(
        egui::pos2(inner.right(), inner.top() + 61.0),
        Align2::RIGHT_TOP,
        volume_state,
        monospace(10.0),
        if volume_state == "ERROR" {
            hud::MAGENTA
        } else if volume_state == "START SCAN" {
            hud::CYAN
        } else {
            hud::LIME
        },
    );

    let track = egui::Rect::from_min_max(
        egui::pos2(inner.left(), inner.top() + 82.0),
        egui::pos2(inner.right(), inner.top() + 90.0),
    );
    painter.rect_filled(track, 0.0, hud::HAIRLINE);
    let used_width = track.width() * volume::used_ratio(volume.usage);
    if used_width > 0.0 {
        painter.rect_filled(
            egui::Rect::from_min_size(track.min, egui::vec2(used_width, track.height())),
            0.0,
            hud::ORANGE,
        );
    }

    painter.text(
        egui::pos2(inner.left(), inner.top() + 105.0),
        Align2::LEFT_TOP,
        format!("USED {}", volume::format_decimal_bytes(used)),
        monospace(10.0),
        theme::TILE_MUTED,
    );
    painter.text(
        egui::pos2(inner.right(), inner.top() + 105.0),
        Align2::RIGHT_TOP,
        format!("FREE {}", volume::format_decimal_bytes(volume.usage.free)),
        monospace(10.0),
        theme::TILE_MUTED,
    );

    let activated = response.clicked() || keyboard_activated;
    response.on_hover_text(format!(
        "{} · {}\n{} total · {} free",
        volume.display_root,
        volume.label,
        volume::format_decimal_bytes(volume.usage.total),
        volume::format_decimal_bytes(volume.usage.free),
    ));
    activated
}

fn scan_batch_exhausted(processed: usize, elapsed: Duration) -> bool {
    processed > 0 && elapsed >= MAX_SCAN_WORK_PER_FRAME
}

impl eframe::App for VoidspaceApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.startup_window_sizer.apply(context);
        self.update_workers(context);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let idle = self.tabs.iter().all(|tab| !tab.scanning || tab.paused)
            && !self.overlays.owns_pointer()
            && !self.fileop_running;
        if let Some(frames) = self.ui_frame_diagnostic.record(Instant::now(), idle)
            && frames > 3
        {
            tracing::warn!(frames, "idle UI frame budget exceeded");
        }
        if self
            .typography
            .update_pixels_per_point(ui.ctx().pixels_per_point())
        {
            let _new_typography_epoch = self.typography.epoch();
            ui.ctx().request_repaint();
        }
        let tactical_arc_active_at_frame_start = self.overlays.transient()
            == Some(TransientOverlay::TacticalArc)
            && self.tactical_arc.is_some();
        let tactical_arc_input_outcome = tactical_arc_active_at_frame_start
            .then(|| {
                self.tactical_arc
                    .as_mut()
                    .and_then(|arc| arc.resolve_input(ui.ctx()))
            })
            .flatten();
        if self.overlays.owns_pointer() {
            ui.disable();
        }
        self.top_bar(ui);
        self.tab_bar(ui);
        self.status_bar(ui);
        self.workspace(ui);
        self.show_transient_overlays(ui.ctx(), tactical_arc_input_outcome);
        self.fileop_dialog(ui.ctx());
        self.show_passive_toast(ui.ctx());
    }
}

fn path_for_node(tab: &ScanTab, node_id: NodeId) -> PathBuf {
    let mut components = Vec::new();
    let mut current = Some(node_id);
    while let Some(id) = current {
        let Some(node) = tab.snapshot.node(id) else {
            break;
        };
        if id != tab.snapshot.root {
            #[cfg(windows)]
            components.push(node.name.to_os_string());
            #[cfg(not(windows))]
            components.push(std::ffi::OsString::from(node.name.display_escaped()));
        }
        current = node.parent;
    }
    let mut path = tab.root_path.clone();
    for component in components.into_iter().rev() {
        path.push(component);
    }
    path
}

fn empty_layout(root: NodeId) -> LayoutSnapshot {
    LayoutSnapshot {
        index_version: 0,
        root,
        nodes: Vec::new(),
        aggregates: Vec::new(),
    }
}

#[cfg(test)]
mod worker_budget_tests {
    use super::{MAX_SCAN_WORK_PER_FRAME, scan_batch_exhausted};
    use std::time::Duration;

    #[test]
    fn scan_batch_always_accepts_the_first_event() {
        assert!(!scan_batch_exhausted(0, Duration::from_secs(1)));
    }

    #[test]
    fn scan_batch_yields_after_its_ui_time_budget() {
        assert!(scan_batch_exhausted(1, MAX_SCAN_WORK_PER_FRAME));
        assert!(!scan_batch_exhausted(
            1,
            MAX_SCAN_WORK_PER_FRAME - Duration::from_nanos(1)
        ));
    }
}

#[cfg(test)]
mod navigation_bookmark_tests {
    use std::sync::Arc;

    use voidspace_index::{IndexSnapshot, NodeSnapshot};
    use voidspace_model::{FileIdentity, NodeFlags, NodeId, NodeKind, ScanId, VolumeId, WinName};

    use super::TreemapBookmark;
    use crate::{AggregateSelection, TreemapState};

    fn node(id: u32, parent: Option<u32>, children: Vec<u32>, name: &str) -> NodeSnapshot {
        NodeSnapshot {
            id: NodeId(id),
            parent: parent.map(NodeId),
            children: children.into_iter().map(NodeId).collect(),
            name: WinName::from(name),
            identity: FileIdentity::stable(VolumeId::local_for_test(1), u128::from(id), 1),
            kind: if name == "leaf" || name == "B" {
                NodeKind::File
            } else {
                NodeKind::Directory
            },
            flags: NodeFlags::default(),
            logical: 1,
            allocated: 1,
            physical_allocated: 1,
        }
    }

    fn snapshot(reordered: bool) -> IndexSnapshot {
        let nodes = if reordered {
            vec![
                node(0, None, vec![1, 2], "root"),
                node(1, Some(0), vec![], "B"),
                node(2, Some(0), vec![3], "A"),
                node(3, Some(2), vec![], "leaf"),
            ]
        } else {
            vec![
                node(0, None, vec![1, 2], "root"),
                node(1, Some(0), vec![3], "A"),
                node(2, Some(0), vec![], "B"),
                node(3, Some(1), vec![], "leaf"),
            ]
        };
        IndexSnapshot {
            scan_id: ScanId(1),
            generation: 1,
            index_version: 1,
            root: NodeId(0),
            nodes: Arc::new(nodes),
        }
    }

    #[test]
    fn full_rescan_restores_navigation_by_logical_path_not_transient_node_id() {
        let old = snapshot(false);
        let mut state = TreemapState::new(old.root);
        state
            .view_path
            .rebuild(NodeId(1), |id| old.node(id).and_then(|node| node.parent))
            .expect("old view path");
        state.selected = Some(NodeId(3));
        state.pinned = Some(NodeId(1));
        state.aggregate_views.push(AggregateSelection {
            parent: NodeId(0),
            depth: 1,
            members: vec![NodeId(2)],
        });

        let restored = TreemapBookmark::capture(&old, &state).restore(&snapshot(true));

        assert_eq!(restored.view_path.current(), NodeId(2));
        assert_eq!(restored.selected, Some(NodeId(3)));
        assert_eq!(restored.pinned, Some(NodeId(2)));
        assert_eq!(restored.aggregate_views[0].members, vec![NodeId(1)]);
    }
}

#[cfg(test)]
mod volume_picker_tests {
    use std::path::PathBuf;

    use super::{apply_volume_refresh, truncate_volume_label, volume_grid_columns};
    use crate::volume::{VolumeInfo, VolumeUsage};

    fn volume(root: &str) -> VolumeInfo {
        VolumeInfo {
            root_path: PathBuf::from(format!("{root}\\")),
            display_root: root.to_owned(),
            label: "Windows".to_owned(),
            usage: VolumeUsage {
                total: 2_000,
                free: 700,
            },
        }
    }

    #[test]
    fn responsive_grid_keeps_cards_above_the_minimum_width() {
        assert_eq!(volume_grid_columns(500.0), 1);
        assert_eq!(volume_grid_columns(800.0), 2);
        assert_eq!(volume_grid_columns(1_280.0), 4);
    }

    #[test]
    fn long_labels_are_truncated_without_splitting_unicode() {
        assert_eq!(truncate_volume_label("Windows", 12), "Windows");
        assert_eq!(
            truncate_volume_label("Рабочий накопитель", 10),
            "Рабочий н…"
        );
    }

    #[test]
    fn transient_refresh_failure_keeps_the_previous_cache() {
        let mut cache = vec![volume("C:")];
        let mut error = None;
        apply_volume_refresh(&mut cache, &mut error, Err("drive query timed out".into()));
        assert_eq!(cache, vec![volume("C:")]);
        assert_eq!(error.as_deref(), Some("drive query timed out"));

        apply_volume_refresh(&mut cache, &mut error, Ok(vec![volume("D:")]));
        assert_eq!(cache, vec![volume("D:")]);
        assert_eq!(error, None);
    }
}

#[cfg(test)]
mod tab_close_tests {
    use super::active_index_after_close;

    #[test]
    fn closing_the_active_tab_selects_the_nearest_survivor() {
        assert_eq!(active_index_after_close(3, 1, 1), Some(1));
        assert_eq!(active_index_after_close(3, 2, 2), Some(1));
    }

    #[test]
    fn closing_a_tab_before_the_active_one_repairs_the_index() {
        assert_eq!(active_index_after_close(4, 3, 1), Some(2));
        assert_eq!(active_index_after_close(4, 0, 3), Some(0));
    }

    #[test]
    fn closing_the_last_tab_returns_to_the_volume_picker() {
        assert_eq!(active_index_after_close(1, 0, 0), None);
    }
}

#[cfg(test)]
mod back_navigation_tests {
    use super::{
        BackDestination, back_destination, collapsed_breadcrumb_for_width,
        collapsed_breadcrumb_indices,
    };

    #[test]
    fn back_from_the_scan_root_opens_the_volume_picker() {
        assert_eq!(back_destination(1, 0), BackDestination::VolumePicker);
    }

    #[test]
    fn back_inside_the_treemap_returns_to_the_parent() {
        assert_eq!(back_destination(2, 0), BackDestination::Parent);
        assert_eq!(back_destination(1, 1), BackDestination::Parent);
    }

    #[test]
    fn deep_breadcrumb_preserves_root_and_current_with_one_middle_collapse() {
        assert_eq!(
            collapsed_breadcrumb_indices(8, 4),
            vec![Some(0), None, Some(5), Some(6), Some(7)]
        );
    }

    #[test]
    fn measured_breadcrumb_collapses_until_long_cells_fit() {
        let indices = collapsed_breadcrumb_for_width(&[90.0; 8], 360.0);
        assert_eq!(indices.first(), Some(&Some(0)));
        assert_eq!(indices.last(), Some(&Some(7)));
        let estimated = indices.len().saturating_sub(1) as f32 * 22.0
            + indices
                .iter()
                .map(|index| index.map_or(24.0, |index| [90.0; 8][index] + 20.0))
                .sum::<f32>();
        assert!(estimated <= 360.0);
    }
}

#[cfg(test)]
mod fileop_dispatch_tests {
    use super::{DeleteDispatch, delete_dispatch};
    use voidspace_fileops::OperationKind;

    #[test]
    fn recycle_executes_immediately_without_a_confirmation_dialog() {
        assert_eq!(
            delete_dispatch(OperationKind::Recycle),
            DeleteDispatch::Immediate
        );
    }

    #[test]
    fn permanent_delete_still_requires_confirmation() {
        assert_eq!(
            delete_dispatch(OperationKind::Permanent),
            DeleteDispatch::Confirm
        );
    }
}

#[cfg(test)]
mod status_bar_tests {
    use super::status_bar_geometry;

    #[test]
    fn status_bar_has_two_rows_and_vertical_breathing_room() {
        let geometry = status_bar_geometry();
        assert_eq!(geometry.height, 48.0);
        assert!(geometry.vertical_margin >= 7);
        assert!(geometry.content_height() >= 32.0);
    }
}

#[cfg(test)]
mod tactical_arc_frame_routing_tests {
    use std::sync::Arc;

    use eframe::App as _;
    use voidspace_index::{IndexSnapshot, NodeSnapshot};
    use voidspace_model::{FileIdentity, NodeFlags, NodeKind, ScanId, VolumeId, WinName};

    use super::*;

    fn node(
        id: u32,
        parent: Option<u32>,
        children: Vec<u32>,
        name: &str,
        kind: NodeKind,
    ) -> NodeSnapshot {
        NodeSnapshot {
            id: NodeId(id),
            parent: parent.map(NodeId),
            children: children.into_iter().map(NodeId).collect(),
            name: WinName::from(name),
            identity: FileIdentity::stable(VolumeId::local_for_test(1), u128::from(id), 1),
            kind,
            flags: NodeFlags::default(),
            logical: 1,
            allocated: 1,
            physical_allocated: 1,
        }
    }

    fn app_with_open_keyboard_arc(context: &egui::Context, viewport: egui::Rect) -> VoidspaceApp {
        let nodes = vec![
            node(0, None, vec![1], "root", NodeKind::Directory),
            node(1, Some(0), vec![2], "A", NodeKind::Directory),
            node(2, Some(1), vec![], "leaf", NodeKind::File),
        ];
        let snapshot = IndexSnapshot {
            scan_id: ScanId(1),
            generation: 1,
            index_version: 1,
            root: NodeId(0),
            nodes: Arc::new(nodes),
        };
        let root_identity = snapshot.node(NodeId(0)).unwrap().identity.clone();
        let mut treemap_state = TreemapState::new(snapshot.root);
        treemap_state
            .view_path
            .rebuild(NodeId(1), |id| {
                snapshot.node(id).and_then(|node| node.parent)
            })
            .unwrap();
        let (_event_tx, event_rx) = bounded(1);
        let (_watch_tx, watcher_events) = bounded(1);
        let tab = ScanTab {
            title: "C:".to_owned(),
            root_path: PathBuf::from(r"C:\"),
            generation: 1,
            index: Index::new(ScanId(1), 1, root_identity, WinName::from("root")),
            snapshot: snapshot.clone(),
            layout: empty_layout(snapshot.root),
            events: event_rx,
            watcher_events,
            scan: None,
            watcher: None,
            scanning: false,
            paused: false,
            files_seen: 3,
            treemap_state,
            pending_rescan: false,
            last_watch_event: None,
            errors: Vec::new(),
            volume_usage: None,
            pending_navigation: None,
        };
        let origin_focus = egui::Id::new("obscured-origin");
        let target = ContextTarget {
            scan_id: snapshot.scan_id,
            generation: snapshot.generation,
            node_id: NodeId(2),
            identity: snapshot.node(NodeId(2)).unwrap().identity.clone(),
            path: PathBuf::from(r"C:\A\leaf"),
            kind: NodeKind::File,
            root: PathBuf::from(r"C:\"),
            view_root: NodeId(1),
            display_name: "leaf".to_owned(),
            display_size: "1 B".to_owned(),
            origin_focus,
        };
        let tactical_arc = TacticalArcState::new(target, viewport.center(), viewport, true)
            .expect("test viewport fits tactical arc");
        let (fileop_tx, fileop_rx) = bounded(1);
        let (volume_refresh_tx, volume_refresh_rx) = bounded(1);
        let mut overlays = OverlayCoordinator::default();
        overlays.open_transient(TransientOverlay::TacticalArc, Some(origin_focus));
        context.memory_mut(|memory| memory.request_focus(origin_focus));
        VoidspaceApp {
            startup_window_sizer: StartupWindowSizer::new(),
            typography: theme::install(context),
            volume_switcher: VolumeSwitcherState::default(),
            tabs: vec![tab],
            active_tab: 0,
            next_scan_id: 2,
            scope_text: r"C:\".to_owned(),
            filter_text: String::new(),
            filter: None,
            filter_error: None,
            compact_filter_draft: String::new(),
            compact_filter_prior: String::new(),
            details_drawer: false,
            toast: None,
            fileop_dialog: None,
            fileop_tx,
            fileop_rx,
            fileop_running: false,
            volume_refresh_tx,
            volume_refresh_rx,
            available_volumes: Vec::new(),
            volume_refresh_started_at: Instant::now(),
            volume_refresh_in_flight: true,
            volume_discovery_complete: true,
            volume_discovery_error: None,
            volume_picker_visible: false,
            overlays,
            tactical_arc: Some(tactical_arc),
            volume_labels: VolumeDisplayRegistry::default(),
            status_detail_modules: Vec::new(),
            ui_frame_diagnostic: crate::diagnostics::UiFrameDiagnostic::default(),
            settings: Settings::default(),
        }
    }

    fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn run_frame(app: &mut VoidspaceApp, context: &egui::Context, input: egui::RawInput) {
        let mut frame = eframe::Frame::_new_kittest();
        let mut output = context.run_ui(input, |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
    }

    #[test]
    fn active_arc_resolves_tab_before_underlay_and_defers_dismiss_until_after_underlay() {
        let context = egui::Context::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let mut app = app_with_open_keyboard_arc(&context, viewport);
        let input = |events| egui::RawInput {
            screen_rect: Some(viewport),
            events,
            ..Default::default()
        };

        run_frame(&mut app, &context, input(Vec::new()));
        run_frame(
            &mut app,
            &context,
            input(vec![
                key_event(egui::Key::ArrowLeft, egui::Modifiers::ALT),
                key_event(egui::Key::Tab, egui::Modifiers::NONE),
            ]),
        );

        assert_eq!(
            app.tabs[0].treemap_state.view_path.as_slice(),
            &[NodeId(0), NodeId(1)],
            "Alt+Left must remain inert while the arc owns the underlay"
        );
        assert_eq!(
            context.memory(|memory| memory.focused()),
            Some(egui::Id::new("tactical-action").with(1))
        );
        assert_eq!(
            app.overlays.transient(),
            Some(TransientOverlay::TacticalArc)
        );

        let click = egui::pos2(120.0, 120.0);
        run_frame(
            &mut app,
            &context,
            input(vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]),
        );
        assert_eq!(
            app.tabs[0].treemap_state.view_path.as_slice(),
            &[NodeId(0), NodeId(1)],
            "the pending outside-click dismissal must not re-enable the underlay"
        );
        assert_eq!(app.overlays.transient(), None);
    }
}
