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
use voidspace_layout::{LayoutSnapshot, Rect as LayoutRect, SizeMode, ViewState, layout};
use voidspace_model::{DirtySet, EventEnvelope, EventPayload, NodeId};
use voidspace_scan::{ScanHandle, ScanRequest, describe_root, start};
use voidspace_watch::{WatchHandle, WatchRequest, WatchSignal, watch};

use crate::{settings::Settings, theme, treemap};

const MAX_SCAN_EVENTS_PER_FRAME: usize = 2_048;
const MAX_SCAN_WORK_PER_FRAME: Duration = Duration::from_millis(5);

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
    selected: Option<NodeId>,
    view_root: NodeId,
    history: Vec<NodeId>,
    pending_rescan: bool,
    last_watch_event: Option<Instant>,
    errors: Vec<String>,
    show_other_for: Option<(NodeId, u32)>,
    preview: treemap::PreviewState,
}

enum InspectorAction {
    Reveal(PathBuf),
    Recycle(PathBuf),
    Permanent(PathBuf),
    Rescan,
    Snapshot,
    Report(ReportFormat),
}

enum ArtifactAction {
    Snapshot,
    Report(ReportFormat),
}

struct FileOpDialog {
    operation: ConfirmableOperation,
    phrase: String,
}

pub struct VoidspaceApp {
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
    turbo_session: bool,
    settings: Settings,
}

impl VoidspaceApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        theme::install(&context.egui_ctx);
        let settings = Settings::load();
        let default_scope = settings.last_scope.clone();
        let (fileop_tx, fileop_rx) = bounded(4);
        let mut app = Self {
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
            turbo_session: std::env::args().any(|argument| argument == "--turbo"),
            settings,
        };
        let arguments: Vec<String> = std::env::args().collect();
        if let Some(index) = arguments.iter().position(|argument| argument == "--scan")
            && let Some(scope) = arguments.get(index + 1)
        {
            app.scope_text = scope.clone();
            app.start_scan(PathBuf::from(scope));
        }
        app
    }

    fn start_scan(&mut self, root_path: PathBuf) {
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
                return;
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
                return;
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
        self.tabs.push(ScanTab {
            title,
            root_path,
            generation,
            view_root: snapshot.root,
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
            selected: None,
            history: Vec::new(),
            pending_rescan: false,
            last_watch_event: None,
            errors: Vec::new(),
            show_other_for: None,
            preview: treemap::PreviewState::default(),
        });
        self.active_tab = self.tabs.len() - 1;
    }

    fn update_workers(&mut self, context: &egui::Context) {
        while let Ok(result) = self.fileop_rx.try_recv() {
            self.fileop_running = false;
            match result {
                Ok(report) if report.failed.is_empty() => {
                    self.toast = Some(format!("Removed {} item(s)", report.deleted.len()));
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.pending_rescan = true;
                        tab.last_watch_event = Some(Instant::now());
                    }
                }
                Ok(report) => {
                    self.toast = Some(format!(
                        "Removed {}; {} failed",
                        report.deleted.len(),
                        report.failed.len()
                    ));
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
            if !dirty.is_empty() {
                tab.snapshot = tab.index.snapshot();
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
        tab.index = Index::new(
            voidspace_model::ScanId((tab_index + 1) as u64),
            tab.generation,
            root.identity,
            root.name,
        );
        tab.snapshot = tab.index.snapshot();
        tab.view_root = tab.snapshot.root;
        tab.selected = None;
        tab.show_other_for = None;
        tab.preview.clear();
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
            Err(error) => tab.errors.push(error.to_string()),
        }
    }

    fn top_bar(&mut self, root_ui: &mut egui::Ui) {
        let mut normal_scan = false;
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
                        egui::Label::new(
                            egui::RichText::new("VOIDSPACE")
                                .strong()
                                .color(theme::ORANGE)
                                .size(11.0),
                        ),
                    );
                    let compact = ui.available_width() < 820.0;
                    let privilege_width = if compact { 0.0 } else { 66.0 };
                    let fixed = 112.0 + privilege_width + if compact { 20.0 } else { 30.0 };
                    let fields = (ui.available_width() - fixed).max(280.0);
                    let filter_width = if compact {
                        (fields * 0.42).clamp(150.0, 220.0)
                    } else {
                        (fields * 0.40).clamp(180.0, 340.0)
                    };
                    let scope_width = (fields - filter_width - 10.0).max(130.0);
                    let scope = ui.add_sized(
                        [scope_width, 36.0],
                        egui::TextEdit::singleline(&mut self.scope_text)
                            .hint_text("C:\\ · press Enter to scan")
                            .margin(egui::Margin::symmetric(11, 8)),
                    );
                    if scope.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        normal_scan = true;
                    }
                    let filter_response = ui.add_sized(
                        [filter_width, 36.0],
                        egui::TextEdit::singleline(&mut self.filter_text)
                            .hint_text("size > 1GiB AND NOT attr:system")
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
                    if ui
                        .add_sized(
                            [112.0, 36.0],
                            egui::Button::new(
                                egui::RichText::new("TURBO / F5").strong().color(theme::BG),
                            )
                            .fill(theme::ORANGE),
                        )
                        .clicked()
                    {
                        self.launch_turbo();
                    }
                    if !compact {
                        egui::Frame::new()
                            .stroke(egui::Stroke::new(1.0, theme::ORANGE))
                            .inner_margin(egui::Margin::symmetric(9, 7))
                            .show(ui, |ui| {
                                ui.monospace(if self.turbo_session { "ADMIN" } else { "USER" });
                            });
                    }
                });
            });
        if normal_scan {
            self.start_scan(PathBuf::from(self.scope_text.trim()));
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

    fn tab_bar(&mut self, root_ui: &mut egui::Ui) {
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
                        let width = (label.chars().count() as f32 * 7.4 + 30.0).clamp(92.0, 260.0);
                        let response = ui.add_sized(
                            [width, 31.0],
                            egui::Button::new(egui::RichText::new(label).color(if active {
                                theme::TEXT
                            } else {
                                theme::MUTED
                            }))
                            .fill(if active {
                                theme::RAISED
                            } else {
                                theme::SURFACE
                            })
                            .stroke(egui::Stroke::NONE),
                        );
                        if active {
                            ui.painter().line_segment(
                                [response.rect.left_bottom(), response.rect.right_bottom()],
                                egui::Stroke::new(2.0, theme::ORANGE),
                            );
                        }
                        if response.clicked() {
                            self.active_tab = index;
                        }
                    }
                });
            });
    }

    fn inspector(ui: &mut egui::Ui, tab: &mut ScanTab) -> Option<InspectorAction> {
        let mut action = None;
        ui.label(
            egui::RichText::new("OBJECT / 01")
                .monospace()
                .size(10.0)
                .color(theme::MUTED),
        );
        ui.add_space(8.0);
        let selected = tab.selected.unwrap_or(tab.view_root);
        if let Some(node) = tab.snapshot.node(selected) {
            ui.heading(node.name.display_escaped());
            let selected_path = path_for_node(tab, selected);
            ui.label(
                egui::RichText::new(selected_path.display().to_string())
                    .size(12.0)
                    .color(theme::MUTED),
            );
            ui.add_space(18.0);
            for (label, value) in [
                ("Allocated", treemap::format_bytes(node.allocated)),
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
                    tab.history.push(tab.view_root);
                    tab.view_root = selected;
                    tab.preview.clear();
                }
                if ui
                    .add_enabled(!tab.history.is_empty(), egui::Button::new("BACK"))
                    .clicked()
                    && let Some(previous) = tab.history.pop()
                {
                    tab.view_root = previous;
                    tab.preview.clear();
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
                egui::RichText::new("Hover a tile to reveal children · click to pin")
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.add_space(10.0);
            if let Some((other_parent, other_count)) = tab.show_other_for
                && other_parent == selected
            {
                ui.separator();
                ui.label(
                    egui::RichText::new("OTHER · SMALL ITEMS")
                        .strong()
                        .color(theme::ORANGE),
                );
                let mut children: Vec<_> = node
                    .children
                    .iter()
                    .filter_map(|child| tab.snapshot.node(*child))
                    .collect();
                children.sort_by_key(|child| child.allocated);
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for child in children.into_iter().take(other_count as usize) {
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
        action
    }

    fn workspace(&mut self, root_ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::BG))
                .show(root_ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("SEE WHERE THE SPACE WENT.")
                                    .size(32.0)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                egui::RichText::new("Choose a folder or volume, then scan.")
                                    .size(15.0)
                                    .color(theme::MUTED),
                            );
                        });
                    });
                });
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
                let available = ui.available_rect_before_wrap();
                tab.layout = layout(
                    &tab.snapshot,
                    &ViewState {
                        root: tab.view_root,
                        bounds: LayoutRect::new(
                            available.left(),
                            available.top(),
                            available.right(),
                            available.bottom(),
                        ),
                        size_mode: SizeMode::Allocated,
                        max_depth: 1,
                        min_area: 196.0,
                        max_rectangles: 1024,
                    },
                    &DirtySet::default(),
                );
                let response = treemap::show(
                    ui,
                    &tab.snapshot,
                    &tab.layout,
                    tab.selected,
                    self.filter.as_ref(),
                    tab.preview,
                );
                if response.canvas_clicked {
                    tab.preview.apply_canvas_click(response.pin_clicked);
                }
                if let Some(selected) = response.clicked {
                    tab.selected = Some(selected);
                    tab.show_other_for = response.aggregate_clicked;
                }
                if let Some(selected) = response.double_clicked
                    && tab
                        .snapshot
                        .node(selected)
                        .is_some_and(|node| !node.children.is_empty())
                {
                    tab.history.push(tab.view_root);
                    tab.view_root = selected;
                    tab.selected = Some(selected);
                    tab.preview.clear();
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
        }
    }

    fn prepare_fileop(&mut self, path: PathBuf, kind: OperationKind) {
        match prepare(OperationDraft {
            kind,
            paths: vec![path],
        }) {
            Ok(operation) => {
                self.fileop_dialog = Some(FileOpDialog {
                    operation,
                    phrase: String::new(),
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
            match confirm(dialog.operation, phrase) {
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
    }

    fn launch_turbo(&mut self) {
        match std::env::current_exe() {
            Ok(executable) => {
                let scope = self.scope_text.replace('"', "");
                let arguments = format!("--turbo --scan \"{scope}\"");
                match voidspace_elevated::launch_elevated(&executable, &arguments) {
                    Ok(()) => {
                        self.toast = Some("Launching privileged Turbo traversal through UAC".into())
                    }
                    Err(error) => self.toast = Some(format!("Turbo launch failed: {error}")),
                }
            }
            Err(error) => self.toast = Some(format!("Cannot locate Voidspace: {error}")),
        }
    }

    fn status_bar(&mut self, root_ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .exact_size(30.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        ui.label(
                            egui::RichText::new(if tab.scanning { "SCANNING" } else { "LIVE" })
                                .strong()
                                .color(if tab.scanning {
                                    theme::ORANGE
                                } else {
                                    theme::LIME
                                }),
                        );
                        ui.separator();
                        ui.label(format!("{} entries", tab.files_seen));
                        ui.separator();
                        ui.label(treemap::format_bytes(
                            tab.snapshot
                                .node(tab.snapshot.root)
                                .map_or(0, |node| node.allocated),
                        ));
                        if tab
                            .watcher
                            .as_ref()
                            .is_some_and(|watcher| watcher.health().overflowed)
                        {
                            ui.separator();
                            ui.label(egui::RichText::new("WATCH RESYNC").color(theme::ORANGE));
                        }
                    } else {
                        ui.label(egui::RichText::new("READY").color(theme::MUTED));
                    }
                    if let Some(error) = &self.filter_error {
                        ui.separator();
                        ui.label(egui::RichText::new(error).color(theme::ORANGE));
                    }
                    if self.fileop_running {
                        ui.separator();
                        ui.label(egui::RichText::new("FILE OPERATION").color(theme::MAGENTA));
                    }
                    if self.turbo_session {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("TURBO FALLBACK")
                                .strong()
                                .color(theme::LIME),
                        );
                    }
                    if let Some(toast) = &self.toast {
                        ui.separator();
                        ui.label(egui::RichText::new(toast).color(theme::ORANGE));
                    }
                });
            });
    }
}

fn scan_batch_exhausted(processed: usize, elapsed: Duration) -> bool {
    processed > 0 && elapsed >= MAX_SCAN_WORK_PER_FRAME
}

impl eframe::App for VoidspaceApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.launch_turbo();
        }
        self.update_workers(context);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
