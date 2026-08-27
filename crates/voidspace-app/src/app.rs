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
    TreemapAction, TreemapState, ViewPath,
    settings::Settings,
    theme, treemap, volume,
    volume_switcher::{self, VolumeRootKey, VolumeSwitcherAction, VolumeSwitcherState},
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
    typography: theme::Typography,
    volume_switcher: VolumeSwitcherState,
    tabs: Vec<ScanTab>,
    active_tab: usize,
    next_scan_id: u64,
    scope_text: String,
    filter_text: String,
    filter: Option<Expr>,
    filter_error: Option<String>,
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
            typography,
            volume_switcher: VolumeSwitcherState::default(),
            tabs: Vec::new(),
            active_tab: 0,
            next_scan_id: 1,
            scope_text: default_scope,
            filter_text: String::new(),
            filter: None,
            filter_error: None,
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
            return;
        }
        self.scope_text = requested.display().to_string();
        if self.start_scan(requested) {
            self.volume_switcher.open = false;
            self.volume_switcher.issue = None;
            self.volume_picker_visible = false;
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
        if self.tabs.is_empty() || self.volume_switcher.open {
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
        tab.index = Index::new(
            voidspace_model::ScanId((tab_index + 1) as u64),
            tab.generation,
            root.identity,
            root.name,
        );
        tab.pending_navigation = Some(navigation);
        tab.volume_usage = volume::query(&tab.root_path);
        let (event_tx, event_rx) = bounded(65_536);
        tab.events = event_rx;
        match start(
            ScanRequest::new(
                (tab_index + 1) as u64,
                tab.generation,
                tab.root_path.clone(),
            ),
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
        let active_root = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| VolumeRootKey::from_scan_root(&tab.root_path));
        let was_switcher_open = self.volume_switcher.open;
        let mut switcher_action = VolumeSwitcherAction::None;
        egui::Panel::top("topbar")
            .exact_size(56.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .stroke(egui::Stroke::new(1.0, theme::LINE)),
            )
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [68.0, 36.0],
                        egui::Label::new(theme::brand_wordmark(&self.typography)),
                    );
                    let compact = ui.available_width() < 820.0;
                    let turbo_width = if compact { 112.0 } else { 132.0 };
                    let fixed = turbo_width + if compact { 20.0 } else { 30.0 };
                    let fields = (ui.available_width() - fixed).max(360.0);
                    let filter_width = if compact {
                        (fields * 0.42).clamp(150.0, 220.0)
                    } else {
                        (fields * 0.40).clamp(180.0, 340.0)
                    };
                    let scope_width = (fields - filter_width - 10.0).max(210.0);
                    ui.allocate_ui(egui::vec2(scope_width, 36.0), |ui| {
                        ui.set_width(scope_width);
                        switcher_action = volume_switcher::show(
                            ui,
                            &mut self.scope_text,
                            &mut self.volume_switcher,
                            &self.available_volumes,
                            active_root,
                            self.volume_refresh_in_flight,
                            self.volume_discovery_error.as_deref(),
                            &self.typography,
                        );
                    });
                    let filter_response = ui.add_sized(
                        [filter_width, 36.0],
                        egui::TextEdit::singleline(&mut self.filter_text)
                            .hint_text("size > 1GiB AND NOT attr:system")
                            .font(self.typography.font(theme::TypographyToken::DataNormal))
                            .margin(egui::Margin::symmetric(11, 8)),
                    );
                    if filter_response.changed() {
                        if self.filter_text.trim().is_empty() {
                            self.filter = None;
                            self.filter_error = None;
                        } else {
                            match parse(&self.filter_text) {
                                Ok(filter) => {
                                    self.filter = Some(filter);
                                    self.filter_error = None;
                                }
                                Err(error) => self.filter_error = Some(error.to_string()),
                            }
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
                        [turbo_width, 36.0],
                        egui::Button::new(turbo)
                            .fill(theme::ORANGE)
                            .sense(egui::Sense::hover()),
                    )
                    .on_hover_text("Voidspace is already running with administrator privileges");
                });
            });
        match switcher_action {
            VolumeSwitcherAction::None => {}
            VolumeSwitcherAction::Close => self.volume_switcher.open = false,
            VolumeSwitcherAction::OpenOrActivate(path) => self.open_or_activate_scan(path),
        }
        if self.volume_switcher.open
            && (!was_switcher_open
                || self.volume_refresh_started_at.elapsed() >= VOLUME_REFRESH_INTERVAL)
        {
            self.request_volume_refresh(root_ui.ctx());
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
        let mut close_requested = None;
        egui::Panel::top("tabs")
            .exact_size(38.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(10, 0)),
            )
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, tab) in self.tabs.iter().enumerate() {
                        let active = index == self.active_tab;
                        let label = if tab.scanning {
                            format!("{} · SCANNING", tab.title)
                        } else {
                            format!("{} · LIVE", tab.title)
                        };
                        let width = (label.chars().count() as f32 * 7.4 + 24.0).clamp(82.0, 230.0);
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let fill = if active {
                            theme::RAISED
                        } else {
                            theme::SURFACE
                        };
                        let response = ui.add_sized(
                            [width, 31.0],
                            egui::Button::new(
                                egui::RichText::new(label)
                                    .font(self.typography.font(theme::TypographyToken::UiControl))
                                    .color(if active { theme::TEXT } else { theme::MUTED }),
                            )
                            .fill(fill)
                            .stroke(egui::Stroke::NONE),
                        );
                        let close = ui
                            .add_sized(
                                [30.0, 31.0],
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
                                egui::Stroke::new(2.0, theme::ORANGE),
                            );
                        }
                        if response.clicked() {
                            self.active_tab = index;
                            self.volume_picker_visible = false;
                        }
                        if close.clicked() {
                            close_requested = Some(index);
                        }
                        ui.add_space(4.0);
                    }
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
                .color(theme::MUTED),
        );
        ui.add_space(8.0);
        let selected = tab
            .treemap_state
            .selected
            .unwrap_or_else(|| tab.treemap_state.view_path.current());
        if let Some(node) = tab.snapshot.node(selected) {
            ui.heading(node.name.display_escaped());
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
            if selected != tab.snapshot.root {
                ui.horizontal(|ui| {
                    if ui.button("RECYCLE").clicked() {
                        action = Some(InspectorAction::Recycle(selected_path.clone()));
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("DELETE FOREVER").color(theme::ORANGE),
                            )
                            .stroke(egui::Stroke::new(1.0, theme::ORANGE)),
                        )
                        .clicked()
                    {
                        action = Some(InspectorAction::Permanent(selected_path));
                    }
                });
            }
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
                                    egui::Grid::new("volume_picker_grid")
                                        .num_columns(columns)
                                        .spacing([VOLUME_CARD_GAP, VOLUME_CARD_GAP])
                                        .show(ui, |ui| {
                                            for (index, volume) in
                                                self.available_volumes.iter().enumerate()
                                            {
                                                if volume_card(ui, volume, card_width) {
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
        let mode = workspace_mode(root_ui.max_rect().width());
        let tab = &mut self.tabs[self.active_tab];
        let mut inspector_action = None;
        if mode == WorkspaceMode::Docked {
            egui::Panel::right("inspector")
                .exact_size(320.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::SURFACE)
                        .inner_margin(16.0)
                        .stroke(egui::Stroke::new(1.0, theme::LINE)),
                )
                .show(root_ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        inspector_action = Self::inspector(ui, tab);
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::MAP_BG).inner_margin(8.0))
            .show(root_ui, |ui| {
                if mode == WorkspaceMode::DrawerClosed && ui.button("DETAILS").clicked() {
                    self.details_drawer = true;
                }
                let mut show_volume_picker = false;
                if ui.ctx().input_mut(|input| {
                    input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)
                }) {
                    show_volume_picker |= apply_navigation(tab, NavigationIntent::Back);
                }
                let navigation = treemap_navigation(ui, tab);
                show_volume_picker |= apply_navigation(tab, navigation);
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
                    },
                );
                if !response.aggregate_still_valid {
                    tab.treemap_state.aggregate = None;
                }
                if let Some(target) = response.context_target {
                    tab.treemap_state.selected = Some(target);
                    tab.treemap_state.aggregate = None;
                }
                if let Some(action) = response.context_action {
                    let target = match action {
                        treemap::TreemapContextAction::Reveal(id)
                        | treemap::TreemapContextAction::Recycle(id)
                        | treemap::TreemapContextAction::Permanent(id) => id,
                    };
                    let path = path_for_node(tab, target);
                    inspector_action = Some(match action {
                        treemap::TreemapContextAction::Reveal(_) => InspectorAction::Reveal(path),
                        treemap::TreemapContextAction::Recycle(_) => InspectorAction::Recycle(path),
                        treemap::TreemapContextAction::Permanent(_) => {
                            InspectorAction::Permanent(path)
                        }
                    });
                }
                if let Some(action) = response.action {
                    tab.apply_treemap_action(action);
                }
                if show_volume_picker {
                    inspector_action = Some(InspectorAction::ShowVolumePicker);
                }
            });

        if self.details_drawer {
            let mut open = true;
            egui::Window::new("DETAILS")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_TOP, [-10.0, 104.0])
                .fixed_size([340.0, 460.0])
                .show(&context, |ui| {
                    inspector_action = Self::inspector(ui, tab);
                });
            self.details_drawer = open;
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
        let permanent = dialog.operation.kind == OperationKind::Permanent;
        let mut execute_now = false;
        let mut cancel = false;
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
                            theme::ORANGE
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
        } else if execute_now && let Some(dialog) = self.fileop_dialog.take() {
            let phrase = if permanent { "DELETE" } else { "" };
            self.execute_fileop(dialog.operation, phrase);
        }
    }

    fn status_bar(&mut self, root_ui: &mut egui::Ui) {
        let geometry = status_bar_geometry();
        egui::Panel::bottom("status")
            .exact_size(geometry.height)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(egui::Stroke::new(1.0, theme::LINE))
                    .inner_margin(egui::Margin::symmetric(14, geometry.vertical_margin)),
            )
            .show(root_ui, |ui| {
                ui.set_min_height(geometry.content_height());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        status_field(
                            ui,
                            &self.typography,
                            "SCAN",
                            if tab.scanning { "SCANNING" } else { "LIVE" },
                            if tab.scanning {
                                theme::ORANGE
                            } else {
                                theme::LIME
                            },
                        );
                        ui.separator();
                        status_field(
                            ui,
                            &self.typography,
                            "ENTRIES",
                            tab.files_seen.to_string(),
                            theme::TEXT,
                        );
                        ui.separator();
                        status_field(
                            ui,
                            &self.typography,
                            "INDEXED",
                            treemap::format_bytes(
                                tab.snapshot
                                    .node(tab.snapshot.root)
                                    .map_or(0, |node| node.allocated),
                            ),
                            theme::TEXT,
                        );
                        if let Some(usage) = tab.volume_usage {
                            ui.separator();
                            status_field(
                                ui,
                                &self.typography,
                                "DISK USED",
                                format!(
                                    "{} / {}",
                                    volume::format_decimal_bytes(usage.used()),
                                    volume::format_decimal_bytes(usage.total),
                                ),
                                theme::TEXT,
                            );
                        }
                        if tab
                            .watcher
                            .as_ref()
                            .is_some_and(|watcher| watcher.health().overflowed)
                        {
                            ui.separator();
                            status_field(ui, &self.typography, "WATCH", "RESYNC", theme::ORANGE);
                        }
                    } else {
                        status_field(ui, &self.typography, "SYSTEM", "READY", theme::MUTED);
                    }
                    if let Some(error) = &self.filter_error {
                        ui.separator();
                        status_field(ui, &self.typography, "FILTER", error, theme::ORANGE);
                    }
                    if self.fileop_running {
                        ui.separator();
                        status_field(ui, &self.typography, "FILE OP", "RUNNING", theme::MAGENTA);
                    }
                    ui.separator();
                    status_field(ui, &self.typography, "ENGINE", "TURBO ACTIVE", theme::LIME);
                    if let Some(toast) = &self.toast {
                        ui.separator();
                        status_field(ui, &self.typography, "NOTICE", toast, theme::ORANGE);
                    }
                });
            });
    }
}

fn status_field(
    ui: &mut egui::Ui,
    typography: &theme::Typography,
    label: &str,
    value: impl Into<String>,
    value_color: egui::Color32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(
            egui::RichText::new(label)
                .font(typography.font(theme::TypographyToken::StatusLabel))
                .color(theme::MUTED),
        );
        ui.label(
            egui::RichText::new(value.into())
                .font(typography.font(theme::TypographyToken::StatusValue))
                .color(value_color),
        );
    });
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

fn treemap_navigation(ui: &mut egui::Ui, tab: &ScanTab) -> NavigationIntent {
    let mut intent = NavigationIntent::None;
    ui.horizontal(|ui| {
        if ui.add(egui::Button::new("← BACK")).clicked() {
            intent = NavigationIntent::Back;
        }
        for (index, node_id) in tab
            .treemap_state
            .view_path
            .as_slice()
            .iter()
            .copied()
            .enumerate()
        {
            if index > 0 {
                ui.label(egui::RichText::new("›").color(theme::MUTED));
            }
            let label = tab
                .snapshot
                .node(node_id)
                .map(|node| node.name.display_escaped())
                .unwrap_or_else(|| "?".into());
            if index + 1 == tab.treemap_state.view_path.as_slice().len() {
                ui.label(egui::RichText::new(label).color(theme::ORANGE).strong());
            } else if ui.link(label).clicked() {
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
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
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

fn volume_card(ui: &mut egui::Ui, volume: &volume::VolumeInfo, width: f32) -> bool {
    use egui::{Align2, FontFamily, FontId, Sense, StrokeKind, WidgetInfo, WidgetType};

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
        "{} {}, total {}, free {}",
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
        theme::RAISED
    } else {
        theme::SURFACE
    };
    painter.rect_filled(rect, 3.0, fill);
    painter.rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(
            if response.has_focus() { 2.0 } else { 1.0 },
            if highlighted {
                theme::ORANGE
            } else {
                theme::LINE
            },
        ),
        StrokeKind::Inside,
    );

    let inner = rect.shrink(18.0);
    let proportional = |size| FontId::new(size, FontFamily::Proportional);
    let monospace = |size| FontId::new(size, FontFamily::Monospace);
    painter.text(
        inner.left_top(),
        Align2::LEFT_TOP,
        &volume.display_root,
        proportional(27.0),
        theme::ORANGE,
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
        proportional(13.0),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(inner.left(), inner.top() + 61.0),
        Align2::LEFT_TOP,
        format!("{percentage}% USED"),
        monospace(10.0),
        theme::MUTED,
    );

    let track = egui::Rect::from_min_max(
        egui::pos2(inner.left(), inner.top() + 82.0),
        egui::pos2(inner.right(), inner.top() + 90.0),
    );
    painter.rect_filled(track, 1.0, theme::LINE);
    let used_width = track.width() * volume::used_ratio(volume.usage);
    if used_width > 0.0 {
        painter.rect_filled(
            egui::Rect::from_min_size(track.min, egui::vec2(used_width, track.height())),
            1.0,
            theme::ORANGE,
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
        self.update_workers(context);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self
            .typography
            .update_pixels_per_point(ui.ctx().pixels_per_point())
        {
            let _new_typography_epoch = self.typography.epoch();
            ui.ctx().request_repaint();
        }
        self.top_bar(ui);
        self.tab_bar(ui);
        self.status_bar(ui);
        self.workspace(ui);
        self.fileop_dialog(ui.ctx());
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
    use super::{BackDestination, back_destination};

    #[test]
    fn back_from_the_scan_root_opens_the_volume_picker() {
        assert_eq!(back_destination(1, 0), BackDestination::VolumePicker);
    }

    #[test]
    fn back_inside_the_treemap_returns_to_the_parent() {
        assert_eq!(back_destination(2, 0), BackDestination::Parent);
        assert_eq!(back_destination(1, 1), BackDestination::Parent);
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
