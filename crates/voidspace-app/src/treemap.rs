use std::{collections::HashMap, sync::Arc};

use egui::{Align2, Color32, FontId, Galley, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use voidspace_filter::{Expr, FilterContext, matches};
use voidspace_index::IndexSnapshot;
use voidspace_layout::{
    LabelFootprint, LayoutNode, LayoutSnapshot, Rect as LayoutRect, SizeMode, ViewState, layout,
};
use voidspace_model::{DirtySet, NodeId};

use crate::{
    theme,
    treemap_state::{AggregateSelection, TreemapAction},
};

const TILE_GAP: f32 = 4.0;
const TILE_INSET: f32 = TILE_GAP / 2.0;
const PREVIEW_HEADER: f32 = 38.0;

#[derive(Clone, Debug, PartialEq)]
struct LabelPlanKey {
    snapshot_version: u64,
    typography_epoch: u64,
    size_mode: SizeMode,
    bounds: [f32; 2],
}

impl LabelPlanKey {
    fn new(
        snapshot_version: u64,
        typography_epoch: u64,
        size_mode: SizeMode,
        bounds: [f32; 2],
    ) -> Self {
        Self {
            snapshot_version,
            typography_epoch,
            size_mode,
            bounds,
        }
    }

    fn matches(
        &self,
        snapshot_version: u64,
        typography_epoch: u64,
        size_mode: SizeMode,
        bounds: [f32; 2],
    ) -> bool {
        self == &Self::new(snapshot_version, typography_epoch, size_mode, bounds)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LabelTier {
    Large,
    Compact,
    SizeOnly,
}

#[derive(Clone, Copy, Debug)]
struct LabelMeasurements {
    large_width: f32,
    large_height: f32,
    compact_width: f32,
    compact_height: f32,
    size_width: f32,
    size_height: f32,
}

fn choose_label_tier(size: [f32; 2], metrics: LabelMeasurements) -> Option<LabelTier> {
    if size[0] >= metrics.large_width && size[1] >= metrics.large_height {
        Some(LabelTier::Large)
    } else if size[0] >= metrics.compact_width && size[1] >= metrics.compact_height {
        Some(LabelTier::Compact)
    } else if size[0] >= metrics.size_width && size[1] >= metrics.size_height {
        Some(LabelTier::SizeOnly)
    } else {
        None
    }
}

pub fn compact_bytes(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "K", "M", "G", "T", "P", "E"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else if value < 100.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.0}{}", UNITS[unit])
    }
}

fn footprint_labels(sorted_sizes: &[u64], real_budget: usize) -> Vec<String> {
    let keep_limit = sorted_sizes.len().min(real_budget).min(128);
    let mut labels = sorted_sizes[..keep_limit]
        .iter()
        .map(|size| compact_bytes(*size))
        .collect::<Vec<_>>();
    let mut suffix = sorted_sizes[keep_limit..]
        .iter()
        .copied()
        .fold(0_u64, u64::saturating_add);
    labels.push(compact_bytes(suffix));
    for size in sorted_sizes[..keep_limit].iter().rev() {
        suffix = suffix.saturating_add(*size);
        labels.push(compact_bytes(suffix));
    }
    labels
}

pub(crate) struct CandidateMetrics {
    footprint: LabelFootprint,
    compact_size_galleys: HashMap<String, Arc<Galley>>,
}

impl CandidateMetrics {
    pub(crate) fn footprint(&self) -> LabelFootprint {
        self.footprint
    }
}

pub(crate) fn candidate_metrics(
    ui: &egui::Ui,
    snapshot: &IndexSnapshot,
    root: NodeId,
    real_budget: usize,
    typography: &theme::Typography,
) -> CandidateMetrics {
    let mut sizes = snapshot
        .node(root)
        .into_iter()
        .flat_map(|node| node.children.iter())
        .filter_map(|id| snapshot.node(*id).map(|node| node.allocated))
        .filter(|size| *size > 0)
        .collect::<Vec<_>>();
    sizes.sort_unstable_by(|left, right| right.cmp(left));
    let font = typography.font(theme::TypographyToken::DataCompact);
    let compact_size_galleys = footprint_labels(&sizes, real_budget)
        .into_iter()
        .map(|text| {
            let galley = ui
                .painter()
                .layout_no_wrap(text.clone(), font.clone(), theme::TILE_MUTED);
            (text, galley)
        })
        .collect::<HashMap<_, _>>();
    let max_width = compact_size_galleys
        .values()
        .map(|galley| galley.size().x)
        .fold(0.0, f32::max);
    let max_height = compact_size_galleys
        .values()
        .map(|galley| galley.size().y)
        .fold(0.0, f32::max);
    CandidateMetrics {
        footprint: LabelFootprint::new(
            max_width + 2.0 * (TILE_INSET + 5.0) + 2.0,
            max_height + 2.0 * (TILE_INSET + 4.0) + 2.0,
        ),
        compact_size_galleys,
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TileIdentity {
    node_id: NodeId,
    render_depth: u8,
    aggregated: bool,
}

struct TileLabelPlan {
    identity: TileIdentity,
    tier: LabelTier,
    final_name: Option<String>,
    formatted_size: String,
    name_font: Option<FontId>,
    size_font: FontId,
    name_galley: Option<Arc<Galley>>,
    size_galley: Arc<Galley>,
    name_pos: Pos2,
    size_pos: Pos2,
    name_color: Color32,
    inner_rect: Rect,
}

struct TreemapLabelPlan {
    key: LabelPlanKey,
    tiles: HashMap<TileIdentity, TileLabelPlan>,
}

#[allow(clippy::too_many_arguments)]
fn build_label_plan(
    ui: &Ui,
    snapshot: &IndexSnapshot,
    layout: &LayoutSnapshot,
    clip: Rect,
    render_depth: u8,
    metrics: &CandidateMetrics,
    typography: &theme::Typography,
    filter: Option<&Expr>,
) -> TreemapLabelPlan {
    let mut tiles = HashMap::new();
    for node in layout.nodes.iter().filter(|node| node.depth == 1) {
        let rect = visual_rect(node.rect, clip);
        let index_node = snapshot.node(node.node_id);
        let matches_filter = node.aggregated
            || filter.is_none_or(|expression| {
                index_node.is_some_and(|entry| {
                    matches(
                        expression,
                        FilterContext {
                            node: entry,
                            path: &entry.name.display_escaped(),
                        },
                    )
                })
            });
        let raw_name = if node.aggregated {
            format!("OTHER · {} ITEMS", node.aggregate_count)
        } else {
            index_node
                .map(|entry| entry.name.display_escaped())
                .unwrap_or_else(|| "?".to_owned())
        };
        let formatted_size = compact_bytes(if node.aggregated {
            node.aggregate_size
        } else {
            index_node.map_or(0, |entry| entry.allocated)
        });
        let size_font = typography.font(theme::TypographyToken::DataCompact);
        let size_galley = metrics
            .compact_size_galleys
            .get(&formatted_size)
            .cloned()
            .unwrap_or_else(|| {
                ui.painter().layout_no_wrap(
                    formatted_size.clone(),
                    size_font.clone(),
                    theme::TILE_MUTED,
                )
            });
        let large_font = typography.font(theme::TypographyToken::TileNameLarge);
        let compact_font = typography.font(theme::TypographyToken::TileNameCompact);
        let large_probe =
            ui.painter()
                .layout_no_wrap("Ag".to_owned(), large_font.clone(), theme::TEXT);
        let compact_probe =
            ui.painter()
                .layout_no_wrap("Ag".to_owned(), compact_font.clone(), theme::TEXT);
        let measurements = LabelMeasurements {
            large_width: (size_galley.size().x + 18.0).max(96.0),
            large_height: large_probe.size().y + size_galley.size().y + 22.0,
            compact_width: (size_galley.size().x + 14.0).max(64.0),
            compact_height: compact_probe.size().y + size_galley.size().y + 15.0,
            size_width: size_galley.size().x + 12.0,
            size_height: size_galley.size().y + 10.0,
        };
        let Some(tier) = choose_label_tier([rect.width(), rect.height()], measurements) else {
            continue;
        };
        let inner_rect = Rect::from_min_max(
            rect.min + egui::vec2(6.0, 5.0),
            rect.max - egui::vec2(6.0, 5.0),
        );
        let name_color = if matches_filter {
            theme::TEXT
        } else {
            theme::MUTED
        };
        let (final_name, name_font, name_galley, name_pos, size_pos) = match tier {
            LabelTier::Large | LabelTier::Compact => {
                let font = if tier == LabelTier::Large {
                    large_font
                } else {
                    compact_font
                };
                let (name, galley) = ellipsized_galley(
                    ui.painter(),
                    &raw_name,
                    font.clone(),
                    name_color,
                    inner_rect.width(),
                );
                let name_pos = inner_rect.left_top();
                let size_pos = egui::pos2(
                    inner_rect.left(),
                    (name_pos.y + galley.size().y + 3.0)
                        .min(inner_rect.bottom() - size_galley.size().y),
                );
                (Some(name), Some(font), Some(galley), name_pos, size_pos)
            }
            LabelTier::SizeOnly => (
                None,
                None,
                None,
                inner_rect.left_top(),
                inner_rect.left_top(),
            ),
        };
        let identity = TileIdentity {
            node_id: node.node_id,
            render_depth,
            aggregated: node.aggregated,
        };
        tiles.insert(
            identity,
            TileLabelPlan {
                identity,
                tier,
                final_name,
                formatted_size,
                name_font,
                size_font,
                name_galley,
                size_galley,
                name_pos,
                size_pos,
                name_color,
                inner_rect,
            },
        );
    }
    TreemapLabelPlan {
        key: LabelPlanKey::new(
            layout.index_version,
            typography.epoch(),
            SizeMode::Allocated,
            [clip.width(), clip.height()],
        ),
        tiles,
    }
}

fn ellipsized_galley(
    painter: &egui::Painter,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> (String, Arc<Galley>) {
    let full = painter.layout_no_wrap(text.to_owned(), font.clone(), color);
    if full.size().x <= max_width {
        return (text.to_owned(), full);
    }
    let characters = text.chars().collect::<Vec<_>>();
    let mut low = 0;
    let mut high = characters.len();
    let mut best = "…".to_owned();
    let mut best_galley = painter.layout_no_wrap(best.clone(), font.clone(), color);
    while low <= high {
        let middle = low + (high - low) / 2;
        let candidate = format!("{}…", characters[..middle].iter().collect::<String>());
        let galley = painter.layout_no_wrap(candidate.clone(), font.clone(), color);
        if galley.size().x <= max_width {
            best = candidate;
            best_galley = galley;
            low = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    (best, best_galley)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Activation {
    Single,
    Double,
    KeyboardZoom,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HitKind {
    BaseDirectory,
    BaseLeaf,
    Nested {
        expandable: bool,
    },
    Aggregate {
        parent: NodeId,
        depth: u8,
        members: Vec<NodeId>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionHit {
    node_id: NodeId,
    kind: HitKind,
}

impl ActionHit {
    fn base_directory(node_id: NodeId) -> Self {
        Self {
            node_id,
            kind: HitKind::BaseDirectory,
        }
    }

    fn base_leaf(node_id: NodeId) -> Self {
        Self {
            node_id,
            kind: HitKind::BaseLeaf,
        }
    }

    fn nested(node_id: NodeId, expandable: bool) -> Self {
        Self {
            node_id,
            kind: HitKind::Nested { expandable },
        }
    }

    fn aggregate(parent: NodeId, depth: u8, members: Vec<NodeId>) -> Self {
        Self {
            node_id: parent,
            kind: HitKind::Aggregate {
                parent,
                depth,
                members,
            },
        }
    }
}

fn action_for_hit(hit: &ActionHit, activation: Activation) -> TreemapAction {
    match &hit.kind {
        HitKind::Aggregate {
            parent,
            depth,
            members,
        } => TreemapAction::OpenAggregate(AggregateSelection {
            parent: *parent,
            depth: *depth,
            members: members.clone(),
        }),
        HitKind::BaseDirectory
            if matches!(activation, Activation::Double | Activation::KeyboardZoom) =>
        {
            TreemapAction::Zoom(hit.node_id)
        }
        HitKind::Nested { expandable: true }
            if matches!(activation, Activation::Double | Activation::KeyboardZoom) =>
        {
            TreemapAction::Zoom(hit.node_id)
        }
        HitKind::BaseDirectory => TreemapAction::ActivateBaseDirectory(hit.node_id),
        HitKind::BaseLeaf => TreemapAction::ActivateBaseLeaf(hit.node_id),
        HitKind::Nested { .. } => TreemapAction::ActivateNested(hit.node_id),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreviewState {
    pub pinned: Option<NodeId>,
}

impl PreviewState {
    pub fn active(self, hovered: Option<NodeId>) -> Option<NodeId> {
        hovered.or(self.pinned)
    }

    pub fn apply_canvas_click(&mut self, pin_target: Option<NodeId>) {
        self.pinned = pin_target;
    }

    pub fn clear(&mut self) {
        self.pinned = None;
    }
}

pub struct TreemapResponse {
    pub clicked: Option<NodeId>,
    pub double_clicked: Option<NodeId>,
    pub aggregate_clicked: Option<(NodeId, u32)>,
    pub canvas_clicked: bool,
    pub pin_clicked: Option<NodeId>,
}

#[derive(Clone, Copy)]
struct VisibleHit {
    node_id: NodeId,
    rect: Rect,
    aggregated: bool,
    aggregate_count: u32,
}

pub fn show(
    ui: &mut Ui,
    snapshot: &IndexSnapshot,
    base_layout: &LayoutSnapshot,
    selected: Option<NodeId>,
    filter: Option<&Expr>,
    preview: PreviewState,
    typography: &theme::Typography,
    base_metrics: &CandidateMetrics,
) -> TreemapResponse {
    let desired = ui.available_size();
    let (response, painter) = ui.allocate_painter(desired, Sense::click());
    let bounds = response.rect;
    painter.rect_filled(bounds, 0.0, theme::MAP_BG);

    let pointer = ui
        .ctx()
        .pointer_hover_pos()
        .filter(|position| bounds.contains(*position));
    let base_nodes: Vec<_> = base_layout
        .nodes
        .iter()
        .filter(|node| node.depth == 1)
        .collect();
    let base_plan = build_label_plan(
        ui,
        snapshot,
        base_layout,
        bounds,
        1,
        base_metrics,
        typography,
        filter,
    );
    assert!(base_plan.key.matches(
        base_layout.index_version,
        typography.epoch(),
        SizeMode::Allocated,
        [bounds.width(), bounds.height()],
    ));
    paint_layout_overflow(&painter, base_layout, bounds, typography);
    let hovered_preview = pointer.and_then(|position| {
        base_nodes
            .iter()
            .rev()
            .copied()
            .find(|node| {
                !node.aggregated
                    && can_preview(snapshot, node)
                    && visual_rect(node.rect, bounds).contains(position)
            })
            .map(|node| node.node_id)
    });
    let pinned_visible = preview.pinned.filter(|pinned| {
        base_nodes
            .iter()
            .any(|node| node.node_id == *pinned && can_preview(snapshot, node))
    });
    let active_preview = PreviewState {
        pinned: pinned_visible,
    }
    .active(hovered_preview);

    let mut base_hits = Vec::with_capacity(base_nodes.len());
    for (rank, node) in base_nodes.iter().enumerate() {
        let rect = visual_rect(node.rect, bounds);
        if rect.width() < 1.0 || rect.height() < 1.0 {
            continue;
        }
        paint_tile(
            &painter,
            snapshot,
            node,
            rect,
            rank,
            selected,
            filter,
            active_preview == Some(node.node_id),
            base_plan
                .tiles
                .get(&TileIdentity {
                    node_id: node.node_id,
                    render_depth: 1,
                    aggregated: node.aggregated,
                })
                .expect("layout emitted a tile without a measured label"),
        );
        base_hits.push(VisibleHit {
            node_id: node.node_id,
            rect,
            aggregated: node.aggregated,
            aggregate_count: node.aggregate_count,
        });
    }

    let mut nested_hits = Vec::new();
    if let Some(preview_root) = active_preview
        && let Some(parent) = base_nodes
            .iter()
            .copied()
            .find(|node| node.node_id == preview_root && can_preview(snapshot, node))
    {
        let parent_rect = visual_rect(parent.rect, bounds);
        let content = Rect::from_min_max(
            Pos2::new(
                parent_rect.left() + TILE_GAP,
                parent_rect.top() + PREVIEW_HEADER,
            ),
            Pos2::new(
                parent_rect.right() - TILE_GAP,
                parent_rect.bottom() - TILE_GAP,
            ),
        );
        if content.width() >= 80.0 && content.height() >= 48.0 {
            let nested_metrics = candidate_metrics(ui, snapshot, preview_root, 512, typography);
            let nested_layout = layout(
                snapshot,
                &ViewState {
                    root: preview_root,
                    bounds: LayoutRect::new(
                        content.left(),
                        content.top(),
                        content.right(),
                        content.bottom(),
                    ),
                    size_mode: SizeMode::Allocated,
                    max_depth: 1,
                    min_area: 196.0,
                    min_label: nested_metrics.footprint(),
                    max_rectangles: 512,
                },
                &DirtySet::default(),
            );
            let nested_plan = build_label_plan(
                ui,
                snapshot,
                &nested_layout,
                content,
                2,
                &nested_metrics,
                typography,
                filter,
            );
            assert!(nested_plan.key.matches(
                nested_layout.index_version,
                typography.epoch(),
                SizeMode::Allocated,
                [content.width(), content.height()],
            ));
            paint_layout_overflow(&painter, &nested_layout, content, typography);
            for (rank, node) in nested_layout
                .nodes
                .iter()
                .filter(|node| node.depth == 1)
                .enumerate()
            {
                let rect = visual_rect(node.rect, content);
                if rect.width() < 1.0 || rect.height() < 1.0 {
                    continue;
                }
                paint_tile(
                    &painter,
                    snapshot,
                    node,
                    rect,
                    rank + 1,
                    selected,
                    filter,
                    false,
                    nested_plan
                        .tiles
                        .get(&TileIdentity {
                            node_id: node.node_id,
                            render_depth: 2,
                            aggregated: node.aggregated,
                        })
                        .expect("nested layout emitted a tile without a measured label"),
                );
                nested_hits.push(VisibleHit {
                    node_id: node.node_id,
                    rect,
                    aggregated: node.aggregated,
                    aggregate_count: node.aggregate_count,
                });
            }
        }
    }

    let interaction_position = response.interact_pointer_pos().or(pointer);
    let hit = interaction_position.and_then(|position| {
        nested_hits
            .iter()
            .rev()
            .chain(base_hits.iter().rev())
            .find(|hit| hit.rect.contains(position))
            .copied()
    });
    let canvas_clicked = response.clicked() || response.double_clicked();
    let pin_clicked = if canvas_clicked {
        interaction_position.and_then(|position| {
            active_preview
                .filter(|preview_root| {
                    base_nodes.iter().any(|node| {
                        node.node_id == *preview_root
                            && visual_rect(node.rect, bounds).contains(position)
                    })
                })
                .or_else(|| {
                    base_nodes
                        .iter()
                        .rev()
                        .copied()
                        .find(|node| {
                            !node.aggregated
                                && can_preview(snapshot, node)
                                && visual_rect(node.rect, bounds).contains(position)
                        })
                        .map(|node| node.node_id)
                })
        })
    } else {
        None
    };

    TreemapResponse {
        clicked: response
            .clicked()
            .then_some(hit.map(|hit| hit.node_id))
            .flatten(),
        double_clicked: response
            .double_clicked()
            .then_some(hit.filter(|hit| !hit.aggregated).map(|hit| hit.node_id))
            .flatten(),
        aggregate_clicked: canvas_clicked
            .then_some(
                hit.filter(|hit| hit.aggregated)
                    .map(|hit| (hit.node_id, hit.aggregate_count)),
            )
            .flatten(),
        canvas_clicked,
        pin_clicked,
    }
}

fn can_preview(snapshot: &IndexSnapshot, node: &LayoutNode) -> bool {
    !node.aggregated
        && node.rect.width() >= 150.0
        && node.rect.height() >= 100.0
        && snapshot
            .node(node.node_id)
            .is_some_and(|index_node| !index_node.children.is_empty())
}

fn visual_rect(rect: LayoutRect, clip: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min_x, rect.min_y),
        Pos2::new(rect.max_x, rect.max_y),
    )
    .intersect(clip)
    .shrink(TILE_INSET)
}

#[allow(clippy::too_many_arguments)]
fn paint_tile(
    painter: &egui::Painter,
    snapshot: &IndexSnapshot,
    node: &LayoutNode,
    rect: Rect,
    rank: usize,
    selected: Option<NodeId>,
    filter: Option<&Expr>,
    preview_active: bool,
    label: &TileLabelPlan,
) {
    let index_node = snapshot.node(node.node_id);
    let accent = accent_for(rank, node.aggregated);
    let matches_filter = node.aggregated
        || filter.is_none_or(|expression| {
            index_node.is_some_and(|index_node| {
                matches(
                    expression,
                    FilterContext {
                        node: index_node,
                        path: &index_node.name.display_escaped(),
                    },
                )
            })
        });
    let fill = if matches_filter {
        blend(
            accent,
            theme::TILE_BG,
            if preview_active { 0.26 } else { 0.18 },
        )
    } else {
        theme::FILTERED_TILE
    };
    let border = if selected == Some(node.node_id) {
        theme::ORANGE
    } else {
        blend(accent, theme::LINE, 0.58)
    };
    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(
            if selected == Some(node.node_id) || preview_active {
                2.0
            } else {
                1.0
            },
            border,
        ),
        StrokeKind::Inside,
    );

    debug_assert_eq!(label.identity.node_id, node.node_id);
    debug_assert!(!label.formatted_size.is_empty());
    debug_assert!(label.inner_rect.contains(label.size_pos));
    match label.tier {
        LabelTier::Large | LabelTier::Compact => {
            debug_assert!(label.final_name.is_some());
            debug_assert!(label.name_font.is_some());
            if let Some(name) = &label.name_galley {
                painter.galley(label.name_pos, name.clone(), label.name_color);
            }
            painter.galley(label.size_pos, label.size_galley.clone(), theme::TILE_MUTED);
        }
        LabelTier::SizeOnly => {
            debug_assert!(label.final_name.is_none());
            painter.galley(label.size_pos, label.size_galley.clone(), theme::TEXT);
        }
    }
}

fn paint_layout_overflow(
    painter: &egui::Painter,
    layout: &LayoutSnapshot,
    bounds: Rect,
    typography: &theme::Typography,
) -> bool {
    let Some(group) = layout
        .aggregates
        .iter()
        .find(|group| group.parent_id == layout.root && group.depth == 1)
    else {
        return false;
    };
    if layout.nodes.iter().any(|node| node.depth == 1) {
        return false;
    }
    painter.text(
        bounds.center(),
        Align2::CENTER_CENTER,
        format!("Not enough room · {}", format_bytes(group.size)),
        typography.font(theme::TypographyToken::DataNormal),
        theme::TILE_MUTED,
    );
    true
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn accent_for(rank: usize, aggregated: bool) -> Color32 {
    if aggregated {
        return theme::VIOLET;
    }
    match rank % 5 {
        0 => theme::ORANGE,
        1 => theme::CYAN,
        2 => theme::LIME,
        3 => theme::MAGENTA,
        _ => theme::VIOLET,
    }
}

fn blend(foreground: Color32, background: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |foreground: u8, background: u8| {
        (f32::from(background) + (f32::from(foreground) - f32::from(background)) * amount).round()
            as u8
    };
    Color32::from_rgb(
        channel(foreground.r(), background.r()),
        channel(foreground.g(), background.g()),
        channel(foreground.b(), background.b()),
    )
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use crate::TreemapState;

    #[test]
    fn recognized_double_click_supersedes_pin() {
        let hit = ActionHit::base_directory(NodeId(7));
        assert_eq!(
            action_for_hit(&hit, Activation::Single),
            crate::TreemapAction::ActivateBaseDirectory(NodeId(7))
        );
        assert_eq!(
            action_for_hit(&hit, Activation::Double),
            crate::TreemapAction::Zoom(NodeId(7))
        );
    }

    #[test]
    fn nested_leaf_preserves_pin_and_other_never_zooms() {
        let nested = ActionHit::nested(NodeId(8), false);
        assert_eq!(
            action_for_hit(&nested, Activation::Single),
            crate::TreemapAction::ActivateNested(NodeId(8))
        );
        assert_eq!(
            action_for_hit(&nested, Activation::KeyboardZoom),
            crate::TreemapAction::ActivateNested(NodeId(8))
        );

        let expandable_nested = ActionHit::nested(NodeId(11), true);
        assert_eq!(
            action_for_hit(&expandable_nested, Activation::KeyboardZoom),
            crate::TreemapAction::Zoom(NodeId(11))
        );

        let other = ActionHit::aggregate(NodeId(2), 1, vec![NodeId(9), NodeId(10)]);
        assert!(matches!(
            action_for_hit(&other, Activation::Double),
            crate::TreemapAction::OpenAggregate(_)
        ));
    }

    #[test]
    fn leaves_never_zoom_and_recognized_second_click_clears_the_first_click_pin() {
        let leaf = ActionHit::base_leaf(NodeId(12));
        assert_eq!(
            action_for_hit(&leaf, Activation::Double),
            crate::TreemapAction::ActivateBaseLeaf(NodeId(12))
        );

        let root = NodeId(1);
        let directory = NodeId(7);
        let hit = ActionHit::base_directory(directory);
        let mut state = TreemapState::new(root);
        state.apply(action_for_hit(&hit, Activation::Single));
        assert_eq!(state.pinned, Some(directory));
        state.apply(action_for_hit(&hit, Activation::Double));
        assert_eq!(state.selected, Some(directory));
        assert_eq!(state.pinned, None);
        assert_eq!(state.aggregate, None);
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn compact_sizes_are_short_and_unit_boundary_candidates_are_all_measured() {
        assert_eq!(compact_bytes(13 * 1024_u64.pow(3)), "13.0G");
        assert_eq!(compact_bytes(820 * 1024_u64.pow(2)), "820M");
        let labels = footprint_labels(
            &[
                717 * 1024_u64.pow(3),
                700 * 1024_u64.pow(2),
                323 * 1024_u64.pow(2),
            ],
            1,
        );
        assert!(labels.iter().any(|label| label == "1023M"));
    }

    #[test]
    fn label_tiers_never_choose_name_only() {
        let metrics = LabelMeasurements {
            large_width: 120.0,
            large_height: 36.0,
            compact_width: 72.0,
            compact_height: 30.0,
            size_width: 38.0,
            size_height: 14.0,
        };
        assert_eq!(
            choose_label_tier([160.0, 50.0], metrics),
            Some(LabelTier::Large)
        );
        assert_eq!(
            choose_label_tier([90.0, 34.0], metrics),
            Some(LabelTier::Compact)
        );
        assert_eq!(
            choose_label_tier([45.0, 20.0], metrics),
            Some(LabelTier::SizeOnly)
        );
        assert_eq!(choose_label_tier([30.0, 12.0], metrics), None);
    }

    #[test]
    fn label_plan_key_rejects_snapshot_typography_or_bounds_drift() {
        let key = LabelPlanKey::new(7, 3, SizeMode::Allocated, [900.0, 600.0]);
        assert!(key.matches(7, 3, SizeMode::Allocated, [900.0, 600.0]));
        assert!(!key.matches(8, 3, SizeMode::Allocated, [900.0, 600.0]));
        assert!(!key.matches(7, 4, SizeMode::Allocated, [900.0, 600.0]));
        assert!(!key.matches(7, 3, SizeMode::Allocated, [899.0, 600.0]));
    }
}
