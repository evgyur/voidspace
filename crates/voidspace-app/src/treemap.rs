use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use voidspace_filter::{Expr, FilterContext, matches};
use voidspace_index::IndexSnapshot;
use voidspace_layout::LayoutSnapshot;
use voidspace_model::NodeId;

use crate::theme;

pub struct TreemapResponse {
    pub clicked: Option<NodeId>,
    pub double_clicked: Option<NodeId>,
    pub aggregate_clicked: Option<(NodeId, u32)>,
}

pub fn show(
    ui: &mut Ui,
    snapshot: &IndexSnapshot,
    layout: &LayoutSnapshot,
    selected: Option<NodeId>,
    filter: Option<&Expr>,
) -> TreemapResponse {
    let desired = ui.available_size();
    let (response, painter) = ui.allocate_painter(desired, Sense::click());
    let bounds = response.rect;
    painter.rect_filled(bounds, 0.0, theme::BG);

    for node in layout.nodes.iter().filter(|node| node.depth > 0) {
        let index_node = snapshot.node(node.node_id);
        let rect = Rect::from_min_max(
            Pos2::new(node.rect.min_x, node.rect.min_y),
            Pos2::new(node.rect.max_x, node.rect.max_y),
        )
        .intersect(bounds);
        if rect.width() < 1.0 || rect.height() < 1.0 {
            continue;
        }
        let base = if node.aggregated {
            theme::VIOLET
        } else {
            color_for(node.node_id)
        };
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
            base.gamma_multiply(0.24)
        } else {
            Color32::from_rgba_unmultiplied(30, 32, 36, 90)
        };
        painter.rect_filled(rect, 0.0, fill);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(
                if selected == Some(node.node_id) {
                    2.0
                } else {
                    1.0
                },
                if selected == Some(node.node_id) {
                    theme::ORANGE
                } else {
                    base.gamma_multiply(0.8)
                },
            ),
            StrokeKind::Inside,
        );
        if rect.width() > 100.0 && rect.height() > 44.0 {
            let name = if node.aggregated {
                format!("OTHER · {} ITEMS", node.aggregate_count)
            } else {
                index_node
                    .map(|node| node.name.display_escaped())
                    .unwrap_or_default()
            };
            let clipped = if name.chars().count() > 34 {
                format!("{}…", name.chars().take(33).collect::<String>())
            } else {
                name
            };
            painter.text(
                rect.left_top() + egui::vec2(8.0, 7.0),
                Align2::LEFT_TOP,
                clipped,
                FontId::proportional(13.0),
                if matches_filter {
                    theme::TEXT
                } else {
                    theme::MUTED
                },
            );
            if rect.width() > 130.0 && rect.height() > 64.0 {
                painter.text(
                    rect.left_top() + egui::vec2(8.0, 24.0),
                    Align2::LEFT_TOP,
                    format_bytes(if node.aggregated {
                        node.aggregate_size
                    } else {
                        index_node.map_or(0, |node| node.allocated)
                    }),
                    FontId::monospace(10.0),
                    theme::MUTED,
                );
            }
        }
    }

    let hit_node = response.interact_pointer_pos().and_then(|position| {
        layout
            .nodes
            .iter()
            .rev()
            .find(|node| node.rect.contains_point(position.x, position.y))
    });
    let hit = hit_node.map(|node| node.node_id);
    let aggregate_hit = hit_node
        .filter(|node| node.aggregated)
        .map(|node| (node.node_id, node.aggregate_count));
    TreemapResponse {
        clicked: response.clicked().then_some(hit).flatten(),
        double_clicked: response
            .double_clicked()
            .then_some(
                hit_node
                    .filter(|node| !node.aggregated)
                    .map(|node| node.node_id),
            )
            .flatten(),
        aggregate_clicked: (response.clicked() || response.double_clicked())
            .then_some(aggregate_hit)
            .flatten(),
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

fn color_for(node: NodeId) -> Color32 {
    match node.0 % 5 {
        0 => theme::ORANGE,
        1 => theme::CYAN,
        2 => theme::LIME,
        3 => theme::MAGENTA,
        _ => theme::VIOLET,
    }
}
