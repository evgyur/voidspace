use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use egui::{Align2, Color32, FontId, Galley, Pos2, Rect, Sense, Stroke, Ui};
use voidspace_filter::{Expr, FilterContext, matches};
use voidspace_index::IndexSnapshot;
use voidspace_layout::{
    LabelFootprint, LayoutNode, LayoutSnapshot, Rect as LayoutRect, SizeMode, ViewState, layout,
};
use voidspace_model::{DirtySet, NodeId};

use crate::{
    hud, theme,
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct LabelStack {
    size_pos: Pos2,
    name_rect: Rect,
}

fn label_stack(inner_rect: Rect, size: egui::Vec2) -> LabelStack {
    let size_pos = inner_rect.left_top();
    let name_top = (size_pos.y + size.y + 2.0).min(inner_rect.bottom());
    LabelStack {
        size_pos,
        name_rect: Rect::from_min_max(
            egui::pos2(inner_rect.left(), name_top),
            inner_rect.right_bottom(),
        ),
    }
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
    render_depth: usize,
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
    render_depth: usize,
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
            theme::TILE_MUTED
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
                let stack = label_stack(inner_rect, size_galley.size());
                let galley = ui.painter().layout(
                    raw_name.clone(),
                    font.clone(),
                    name_color,
                    stack.name_rect.width(),
                );
                (
                    Some(raw_name),
                    Some(font),
                    Some(galley),
                    stack.name_rect.left_top(),
                    stack.size_pos,
                )
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
        depth: usize,
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

    fn aggregate(parent: NodeId, depth: usize, members: Vec<NodeId>) -> Self {
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
        } => {
            let selection = AggregateSelection {
                parent: *parent,
                depth: *depth,
                members: members.clone(),
            };
            if matches!(activation, Activation::Double | Activation::KeyboardZoom) {
                TreemapAction::ZoomAggregate(selection)
            } else {
                TreemapAction::OpenAggregate(selection)
            }
        }
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
        HitKind::Nested { expandable: true } => TreemapAction::ActivateBaseDirectory(hit.node_id),
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
        self.pinned.or(hovered)
    }

    pub fn apply_canvas_click(&mut self, pin_target: Option<NodeId>) {
        self.pinned = pin_target;
    }

    pub fn clear(&mut self) {
        self.pinned = None;
    }
}

pub struct TreemapResponse {
    pub action: Option<TreemapAction>,
    pub context_target: Option<(NodeId, egui::Id, bool)>,
    pub aggregate_still_valid: bool,
}

#[derive(Clone, Debug)]
struct VisibleHit {
    node_id: NodeId,
    rect: Rect,
    depth: usize,
    aggregated: bool,
    expandable: bool,
    name: String,
    formatted_size: String,
    aggregate_members: Vec<NodeId>,
}

impl VisibleHit {
    fn action_hit(&self) -> ActionHit {
        if self.aggregated {
            ActionHit::aggregate(self.node_id, self.depth, self.aggregate_members.clone())
        } else if self.depth > 1 {
            ActionHit::nested(self.node_id, self.expandable)
        } else if self.expandable {
            ActionHit::base_directory(self.node_id)
        } else {
            ActionHit::base_leaf(self.node_id)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenderedAggregate {
    parent: NodeId,
    depth: usize,
    members: Vec<NodeId>,
}

fn aggregate_is_still_valid(
    open_aggregate: Option<&AggregateSelection>,
    rendered_aggregates: &[RenderedAggregate],
) -> bool {
    open_aggregate.is_none_or(|open| {
        rendered_aggregates.iter().any(|rendered| {
            rendered.parent == open.parent
                && rendered.depth == open.depth
                && rendered.members == open.members
        })
    })
}

pub(crate) fn preview_chain(
    root: NodeId,
    target: NodeId,
    mut parent_of: impl FnMut(NodeId) -> Option<NodeId>,
) -> Option<Vec<NodeId>> {
    if target == root {
        return None;
    }
    let mut reverse = vec![target];
    let mut current = target;
    while current != root {
        current = parent_of(current)?;
        if reverse.contains(&current) {
            return None;
        }
        reverse.push(current);
    }
    reverse.reverse();
    reverse.remove(0);
    Some(reverse)
}

fn nested_layer_accepts_click(parent_chain_index: Option<usize>) -> bool {
    parent_chain_index.is_some()
}

pub(crate) struct ShowRequest<'a> {
    pub snapshot: &'a IndexSnapshot,
    pub base_layout: &'a LayoutSnapshot,
    pub selected: Option<NodeId>,
    pub filter: Option<&'a Expr>,
    pub preview: PreviewState,
    pub typography: &'a theme::Typography,
    pub base_metrics: &'a CandidateMetrics,
    pub open_aggregate: Option<&'a AggregateSelection>,
    pub interactive: bool,
}

pub fn show(ui: &mut Ui, request: ShowRequest<'_>) -> TreemapResponse {
    let ShowRequest {
        snapshot,
        base_layout,
        selected,
        filter,
        preview,
        typography,
        base_metrics,
        open_aggregate,
        interactive,
    } = request;
    let desired = ui.available_size();
    let (response, painter) = ui.allocate_painter(desired, Sense::click());
    let bounds = response.rect;
    painter.rect_filled(bounds, 0.0, theme::MAP_BG);
    let grid_step = 26.0;
    if grid_step * ui.ctx().pixels_per_point() >= 12.0 {
        let mut x = bounds.left();
        while x <= bounds.right() {
            painter.line_segment(
                [Pos2::new(x, bounds.top()), Pos2::new(x, bounds.bottom())],
                Stroke::new(0.5, hud::GRID),
            );
            x += grid_step;
        }
        let mut y = bounds.top();
        while y <= bounds.bottom() {
            painter.line_segment(
                [Pos2::new(bounds.left(), y), Pos2::new(bounds.right(), y)],
                Stroke::new(0.5, hud::GRID),
            );
            y += grid_step;
        }
    }

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
    let hovered_base = pointer.and_then(|position| {
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
    let persistent_target = preview
        .pinned
        .or_else(|| open_aggregate.map(|aggregate| aggregate.parent));
    let persistent_chain = persistent_target
        .and_then(|target| {
            preview_chain(base_layout.root, target, |id| {
                snapshot.node(id).and_then(|node| node.parent)
            })
        })
        .filter(|chain| {
            chain.first().is_some_and(|first| {
                base_nodes
                    .iter()
                    .any(|node| node.node_id == *first && can_preview(snapshot, node))
            })
        })
        .unwrap_or_default();
    let active_base = persistent_chain.first().copied().or(hovered_base);

    let mut rendered_aggregates = base_layout
        .aggregates
        .iter()
        .filter(|group| group.depth == 1)
        .map(|group| RenderedAggregate {
            parent: group.parent_id,
            depth: 1,
            members: group.member_ids.clone(),
        })
        .collect::<Vec<_>>();
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
            active_base == Some(node.node_id),
            preview.pinned == Some(node.node_id),
            hovered_base == Some(node.node_id),
            base_plan
                .tiles
                .get(&TileIdentity {
                    node_id: node.node_id,
                    render_depth: 1,
                    aggregated: node.aggregated,
                })
                .expect("layout emitted a tile without a measured label"),
        );
        let label = base_plan
            .tiles
            .get(&TileIdentity {
                node_id: node.node_id,
                render_depth: 1,
                aggregated: node.aggregated,
            })
            .expect("base hit has no label plan");
        base_hits.push(VisibleHit {
            node_id: node.node_id,
            rect,
            depth: 1,
            aggregated: node.aggregated,
            expandable: !node.aggregated
                && snapshot
                    .node(node.node_id)
                    .is_some_and(|entry| !entry.children.is_empty()),
            name: tile_accessible_name(snapshot, node),
            formatted_size: label.formatted_size.clone(),
            aggregate_members: aggregate_members(base_layout, node.node_id),
        });
    }

    let mut nested_hits = Vec::new();
    let mut active_parent = active_base.and_then(|id| {
        base_hits
            .iter()
            .find(|hit| hit.node_id == id && hit.expandable)
            .cloned()
    });
    let mut persistent_index = active_parent.as_ref().and_then(|parent| {
        persistent_chain
            .first()
            .is_some_and(|id| *id == parent.node_id)
            .then_some(0)
    });
    let mut render_depth = 2_usize;
    let mut expanded = HashSet::new();
    while let Some(parent) = active_parent.take() {
        if !expanded.insert(parent.node_id) {
            break;
        }
        let parent_rect = parent.rect;
        if preview.pinned == Some(parent.node_id) {
            painter.text(
                parent_rect.right_top() + egui::vec2(-8.0, 8.0),
                Align2::RIGHT_TOP,
                "PINNED",
                typography.font(theme::TypographyToken::DataMicro),
                theme::LIME,
            );
        }
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
            let nested_metrics = candidate_metrics(ui, snapshot, parent.node_id, 512, typography);
            let nested_layout = layout(
                snapshot,
                &ViewState {
                    root: parent.node_id,
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
                render_depth,
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
            rendered_aggregates.extend(
                nested_layout
                    .aggregates
                    .iter()
                    .filter(|group| group.depth == 1)
                    .map(|group| RenderedAggregate {
                        parent: group.parent_id,
                        depth: render_depth,
                        members: group.member_ids.clone(),
                    }),
            );
            let layer_hits = nested_layout
                .nodes
                .iter()
                .filter(|node| node.depth == 1)
                .filter_map(|node| {
                    let rect = visual_rect(node.rect, content);
                    if rect.width() < 1.0 || rect.height() < 1.0 {
                        return None;
                    }
                    let label = nested_plan
                        .tiles
                        .get(&TileIdentity {
                            node_id: node.node_id,
                            render_depth,
                            aggregated: node.aggregated,
                        })
                        .expect("nested hit has no label plan");
                    Some(VisibleHit {
                        node_id: node.node_id,
                        rect,
                        depth: render_depth,
                        aggregated: node.aggregated,
                        expandable: !node.aggregated
                            && snapshot
                                .node(node.node_id)
                                .is_some_and(|entry| !entry.children.is_empty()),
                        name: tile_accessible_name(snapshot, node),
                        formatted_size: label.formatted_size.clone(),
                        aggregate_members: aggregate_members(&nested_layout, node.node_id),
                    })
                })
                .collect::<Vec<_>>();
            let hovered_child = pointer.and_then(|position| {
                layer_hits
                    .iter()
                    .rev()
                    .find(|hit| {
                        hit.expandable
                            && hit.rect.width() >= 80.0
                            && hit.rect.height() >= 48.0
                            && hit.rect.contains(position)
                    })
                    .map(|hit| hit.node_id)
            });
            let persistent_child = persistent_index
                .and_then(|index| persistent_chain.get(index + 1))
                .copied();
            let next_active = persistent_child.or(hovered_child);

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
                    rank + render_depth,
                    selected,
                    filter,
                    next_active == Some(node.node_id),
                    preview.pinned == Some(node.node_id),
                    hovered_child == Some(node.node_id),
                    nested_plan
                        .tiles
                        .get(&TileIdentity {
                            node_id: node.node_id,
                            render_depth,
                            aggregated: node.aggregated,
                        })
                        .expect("nested layout emitted a tile without a measured label"),
                );
            }

            let next_persistent_index = persistent_index.and_then(|index| {
                persistent_child
                    .is_some_and(|child| Some(child) == next_active)
                    .then_some(index + 1)
            });
            if nested_layer_accepts_click(persistent_index) {
                nested_hits.extend(layer_hits.iter().cloned());
            }
            active_parent = next_active.and_then(|id| {
                layer_hits
                    .iter()
                    .find(|hit| hit.node_id == id && hit.expandable)
                    .cloned()
            });
            persistent_index = next_persistent_index;
            render_depth = render_depth.saturating_add(1);
        }
    }

    if !interactive {
        return TreemapResponse {
            action: None,
            context_target: None,
            aggregate_still_valid: aggregate_is_still_valid(open_aggregate, &rendered_aggregates),
        };
    }
    let base_responses = hit_responses(ui, base_layout.root, base_hits);
    let nested_responses = hit_responses(ui, base_layout.root, nested_hits);
    let mut action = None;
    let mut context_target = None;
    for (hit, tile_response) in nested_responses
        .iter()
        .rev()
        .chain(base_responses.iter().rev())
    {
        if let Some(node_id) = file_action_target(hit) {
            let keyboard_context = tile_response.has_focus()
                && ui.input(|input| input.modifiers.shift && input.key_pressed(egui::Key::F10));
            if tile_response.secondary_clicked() || keyboard_context {
                context_target = Some((node_id, tile_response.id, keyboard_context));
            }
        }
        let keyboard_zoom = tile_response.has_focus()
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter));
        let keyboard_activate = tile_response.has_focus()
            && ui.input(|input| {
                input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
            });
        let activation = if tile_response.double_clicked() {
            Some(Activation::Double)
        } else if keyboard_zoom {
            Some(Activation::KeyboardZoom)
        } else if tile_response.clicked() || keyboard_activate {
            Some(Activation::Single)
        } else {
            None
        };
        if let Some(activation) = activation {
            action = Some(action_for_hit(&hit.action_hit(), activation));
            break;
        }
    }
    if action.is_none()
        && (response.clicked() || ui.input(|input| input.key_pressed(egui::Key::Escape)))
    {
        action = Some(TreemapAction::ClearPreview);
    }

    TreemapResponse {
        action,
        context_target,
        aggregate_still_valid: aggregate_is_still_valid(open_aggregate, &rendered_aggregates),
    }
}

fn file_action_target(hit: &VisibleHit) -> Option<NodeId> {
    (!hit.aggregated).then_some(hit.node_id)
}

fn hit_responses(
    ui: &mut Ui,
    root: NodeId,
    hits: Vec<VisibleHit>,
) -> Vec<(VisibleHit, egui::Response)> {
    hits.into_iter()
        .map(|hit| {
            let response = ui.interact(
                hit.rect,
                ui.id().with((root, hit.node_id, hit.depth, hit.aggregated)),
                Sense::click(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!(
                        "{} · {}{}",
                        hit.name,
                        hit.formatted_size,
                        if hit.expandable { " · expandable" } else { "" }
                    ),
                )
            });
            let hint = if hit.aggregated {
                "Click: inspect · Double-click: open OTHER"
            } else if hit.expandable {
                "Click: expand · Double-click: zoom"
            } else {
                "Click: inspect"
            };
            (hit, response.on_hover_text(hint))
        })
        .collect()
}

fn tile_accessible_name(snapshot: &IndexSnapshot, node: &LayoutNode) -> String {
    if node.aggregated {
        format!("OTHER · {} ITEMS", node.aggregate_count)
    } else {
        snapshot
            .node(node.node_id)
            .map(|entry| entry.name.display_escaped())
            .unwrap_or_else(|| "?".to_owned())
    }
}

fn aggregate_members(layout: &LayoutSnapshot, parent: NodeId) -> Vec<NodeId> {
    layout
        .aggregates
        .iter()
        .find(|group| group.parent_id == parent && group.depth == 1)
        .map(|group| group.member_ids.clone())
        .unwrap_or_default()
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
    preview_pinned: bool,
    hovered: bool,
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
    } else if preview_pinned {
        theme::LIME
    } else {
        blend(accent, theme::LINE, 0.58)
    };
    let stroke = Stroke::new(
        if selected == Some(node.node_id) || preview_active {
            2.0
        } else {
            1.0
        },
        border,
    );
    if !node.aggregated && rect.width() >= 48.0 && rect.height() >= 34.0 {
        hud::paint_cut_frame(painter, rect, fill, stroke, 6.0);
    } else {
        painter.rect_filled(rect, 0.0, fill);
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    }
    if selected == Some(node.node_id) {
        hud::paint_corner_brackets(painter, rect.shrink(3.0), theme::ORANGE);
    }
    if hovered && rect.width() >= 64.0 && rect.height() >= 64.0 {
        hud::paint_reticle(painter, rect.center(), hud::CYAN);
    }

    debug_assert_eq!(label.identity.node_id, node.node_id);
    debug_assert!(!label.formatted_size.is_empty());
    debug_assert!(label.inner_rect.contains(label.size_pos));
    let _measured_size_font = &label.size_font;
    match label.tier {
        LabelTier::Large | LabelTier::Compact => {
            debug_assert!(label.final_name.is_some());
            debug_assert!(label.name_font.is_some());
            painter.galley(label.size_pos, label.size_galley.clone(), theme::TEXT);
            if let Some(name) = &label.name_galley {
                painter.with_clip_rect(label.inner_rect).galley(
                    label.name_pos,
                    name.clone(),
                    label.name_color,
                );
            }
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
    fn nested_leaf_preserves_pin_and_other_opens_as_a_virtual_level() {
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
            action_for_hit(&expandable_nested, Activation::Single),
            crate::TreemapAction::ActivateBaseDirectory(NodeId(11))
        );
        assert_eq!(
            action_for_hit(&expandable_nested, Activation::KeyboardZoom),
            crate::TreemapAction::Zoom(NodeId(11))
        );

        let other = ActionHit::aggregate(NodeId(2), 1, vec![NodeId(9), NodeId(10)]);
        assert!(matches!(
            action_for_hit(&other, Activation::Single),
            crate::TreemapAction::OpenAggregate(_)
        ));
        assert!(matches!(
            action_for_hit(&other, Activation::Double),
            crate::TreemapAction::ZoomAggregate(_)
        ));
    }

    #[test]
    fn pinned_preview_chain_reaches_any_descendant_depth() {
        let root = NodeId(1);
        let level_one = NodeId(2);
        let level_two = NodeId(3);
        let level_three = NodeId(4);
        let chain = preview_chain(root, level_three, |id| match id {
            NodeId(2) => Some(root),
            NodeId(3) => Some(level_one),
            NodeId(4) => Some(level_two),
            _ => None,
        });

        assert_eq!(chain, Some(vec![level_one, level_two, level_three]));
    }

    #[test]
    fn hover_only_children_do_not_steal_the_parent_click() {
        assert!(!nested_layer_accepts_click(None));
        assert!(nested_layer_accepts_click(Some(0)));
        assert!(nested_layer_accepts_click(Some(7)));
    }

    #[test]
    fn context_file_actions_are_available_only_for_real_nodes() {
        let real = VisibleHit {
            node_id: NodeId(41),
            rect: Rect::NOTHING,
            depth: 1,
            aggregated: false,
            expandable: false,
            name: "real".into(),
            formatted_size: "1.0G".into(),
            aggregate_members: Vec::new(),
        };
        let aggregate = VisibleHit {
            aggregated: true,
            ..real.clone()
        };

        assert_eq!(file_action_target(&real), Some(NodeId(41)));
        assert_eq!(file_action_target(&aggregate), None);
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
    fn label_stack_places_size_before_the_wrapped_name() {
        let inner = Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(130.0, 90.0));
        let stack = label_stack(inner, egui::vec2(42.0, 11.0));

        assert_eq!(stack.size_pos, inner.left_top());
        assert!(stack.name_rect.top() > stack.size_pos.y);
        assert_eq!(stack.name_rect.left(), inner.left());
        assert_eq!(stack.name_rect.right(), inner.right());
        assert_eq!(stack.name_rect.bottom(), inner.bottom());
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

#[cfg(test)]
mod aggregate_validity_tests {
    use super::*;

    #[test]
    fn only_exact_ordered_aggregate_membership_remains_valid() {
        let open = AggregateSelection {
            parent: NodeId(2),
            depth: 1,
            members: vec![NodeId(9), NodeId(10)],
        };
        let exact = RenderedAggregate {
            parent: NodeId(2),
            depth: 1,
            members: vec![NodeId(9), NodeId(10)],
        };
        assert!(aggregate_is_still_valid(
            Some(&open),
            std::slice::from_ref(&exact)
        ));
        assert!(!aggregate_is_still_valid(
            Some(&open),
            &[RenderedAggregate {
                members: vec![NodeId(10), NodeId(9)],
                ..exact.clone()
            }]
        ));
        assert!(!aggregate_is_still_valid(
            Some(&open),
            &[RenderedAggregate {
                depth: 2,
                ..exact.clone()
            }]
        ));
        assert!(!aggregate_is_still_valid(Some(&open), &[]));
        assert!(aggregate_is_still_valid(None, &[]));
    }
}
