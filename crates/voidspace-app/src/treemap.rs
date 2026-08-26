use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use voidspace_filter::{Expr, FilterContext, matches};
use voidspace_index::IndexSnapshot;
use voidspace_layout::{
    LayoutNode, LayoutSnapshot, Rect as LayoutRect, SizeMode, ViewState, layout,
};
use voidspace_model::{DirtySet, NodeId};

use crate::theme;

const TILE_GAP: f32 = 4.0;
const TILE_INSET: f32 = TILE_GAP / 2.0;
const PREVIEW_HEADER: f32 = 38.0;

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
            false,
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
                    min_label: voidspace_layout::LabelFootprint::new(54.0, 22.0),
                    max_rectangles: 512,
                },
                &DirtySet::default(),
            );
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
                    true,
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
    nested: bool,
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

    let name = if node.aggregated {
        format!("OTHER · {} ITEMS", node.aggregate_count)
    } else {
        index_node
            .map(|node| node.name.display_escaped())
            .unwrap_or_default()
    };
    let minimum_width = if nested { 74.0 } else { 96.0 };
    let minimum_height = if nested { 30.0 } else { 42.0 };
    if rect.width() < minimum_width || rect.height() < minimum_height {
        return;
    }
    let max_chars = ((rect.width() - 16.0) / 7.2).max(4.0) as usize;
    let clipped = if name.chars().count() > max_chars {
        let take = max_chars.saturating_sub(1);
        format!("{}…", name.chars().take(take).collect::<String>())
    } else {
        name
    };
    painter.text(
        rect.left_top() + egui::vec2(9.0, 8.0),
        Align2::LEFT_TOP,
        clipped,
        FontId::proportional(if nested { 12.0 } else { 14.0 }),
        if matches_filter {
            theme::TEXT
        } else {
            theme::MUTED
        },
    );
    if rect.width() >= 126.0 && rect.height() >= 60.0 {
        painter.text(
            rect.left_top() + egui::vec2(9.0, if nested { 25.0 } else { 27.0 }),
            Align2::LEFT_TOP,
            format_bytes(if node.aggregated {
                node.aggregate_size
            } else {
                index_node.map_or(0, |node| node.allocated)
            }),
            FontId::monospace(10.0),
            theme::TILE_MUTED,
        );
    }
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
