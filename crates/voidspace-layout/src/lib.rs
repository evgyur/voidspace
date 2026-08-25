//! Stable squarified treemap layout.

mod hit_test;
mod squarify;

pub use hit_test::*;
pub use squarify::*;

use serde::{Deserialize, Serialize};
use voidspace_index::IndexSnapshot;
use voidspace_model::{DirtySet, NodeId};

pub const LAYOUT_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SizeMode {
    Allocated,
    Logical,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewState {
    pub root: NodeId,
    pub bounds: Rect,
    pub size_mode: SizeMode,
    pub max_depth: u8,
    pub min_area: f32,
    pub max_rectangles: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutNode {
    pub node_id: NodeId,
    pub parent_id: Option<NodeId>,
    pub rect: Rect,
    pub depth: u8,
    pub aggregated: bool,
    pub aggregate_count: u32,
    pub aggregate_size: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutSnapshot {
    pub index_version: u64,
    pub root: NodeId,
    pub nodes: Vec<LayoutNode>,
}

pub fn layout(snapshot: &IndexSnapshot, view: &ViewState, _dirty: &DirtySet) -> LayoutSnapshot {
    let mut output = vec![LayoutNode {
        node_id: view.root,
        parent_id: snapshot.node(view.root).and_then(|node| node.parent),
        rect: view.bounds,
        depth: 0,
        aggregated: false,
        aggregate_count: 0,
        aggregate_size: 0,
    }];
    layout_children(snapshot, view, view.root, view.bounds, 1, &mut output);
    LayoutSnapshot {
        index_version: snapshot.index_version,
        root: view.root,
        nodes: output,
    }
}

fn layout_children(
    snapshot: &IndexSnapshot,
    view: &ViewState,
    parent: NodeId,
    bounds: Rect,
    depth: u8,
    output: &mut Vec<LayoutNode>,
) {
    if depth > view.max_depth || output.len() >= view.max_rectangles {
        return;
    }
    let Some(parent_node) = snapshot.node(parent) else {
        return;
    };
    let mut children: Vec<_> = parent_node
        .children
        .iter()
        .filter_map(|id| snapshot.node(*id).map(|node| (*id, node)))
        .filter(|(_, node)| match view.size_mode {
            SizeMode::Allocated => node.allocated > 0,
            SizeMode::Logical => node.logical > 0,
        })
        .collect();
    children.sort_by(|(left_id, left), (right_id, right)| {
        let left_size = match view.size_mode {
            SizeMode::Allocated => left.allocated,
            SizeMode::Logical => left.logical,
        };
        let right_size = match view.size_mode {
            SizeMode::Allocated => right.allocated,
            SizeMode::Logical => right.logical,
        };
        right_size.cmp(&left_size).then(left_id.cmp(right_id))
    });
    let child_weights: Vec<u64> = children
        .iter()
        .map(|(_, node)| match view.size_mode {
            SizeMode::Allocated => node.allocated,
            SizeMode::Logical => node.logical,
        })
        .collect();
    let total_weight: u128 = child_weights.iter().map(|weight| u128::from(*weight)).sum();
    let minimum_standalone_area = view.min_area.max(900.0);
    let keep_count = child_weights
        .iter()
        .enumerate()
        .take_while(|(index, weight)| {
            let predicted = if total_weight == 0 {
                0.0
            } else {
                bounds.area() * (**weight as f32 / total_weight as f32)
            };
            *index == 0 || (predicted >= minimum_standalone_area && *index < 128)
        })
        .count();
    let aggregate_count = children.len().saturating_sub(keep_count);
    let aggregate_size: u64 = child_weights[keep_count..]
        .iter()
        .fold(0_u64, |sum, weight| sum.saturating_add(*weight));
    let has_aggregate = aggregate_count > 0 && aggregate_size > 0;
    let (content_bounds, aggregate_rect) = if has_aggregate {
        split_other_on_right(bounds, aggregate_size, total_weight)
    } else {
        (bounds, None)
    };
    let rectangles = layout_weights(&child_weights[..keep_count], content_bounds);
    for (((node_id, _), rect), _) in children
        .into_iter()
        .take(keep_count)
        .zip(rectangles.iter().copied())
        .zip(0..view.max_rectangles.saturating_sub(output.len()))
    {
        if rect.area() < view.min_area {
            continue;
        }
        output.push(LayoutNode {
            node_id,
            parent_id: Some(parent),
            rect,
            depth,
            aggregated: false,
            aggregate_count: 0,
            aggregate_size: 0,
        });
        let child_bounds = if rect.height() > 32.0 && rect.width() > 8.0 {
            Rect::new(
                rect.min_x + 2.0,
                rect.min_y + 24.0,
                rect.max_x - 2.0,
                rect.max_y - 2.0,
            )
        } else {
            rect
        };
        layout_children(snapshot, view, node_id, child_bounds, depth + 1, output);
    }
    if aggregate_count > 0
        && aggregate_size > 0
        && output.len() < view.max_rectangles
        && let Some(rect) = aggregate_rect
    {
        output.push(LayoutNode {
            node_id: parent,
            parent_id: Some(parent),
            rect,
            depth,
            aggregated: true,
            aggregate_count: aggregate_count.min(u32::MAX as usize) as u32,
            aggregate_size,
        });
    }
}

fn split_other_on_right(
    bounds: Rect,
    aggregate_size: u64,
    total_weight: u128,
) -> (Rect, Option<Rect>) {
    if aggregate_size == 0 || total_weight == 0 || bounds.width() < 8.0 {
        return (bounds, None);
    }

    // The aggregate is a stable right-hand column: the visual reading order remains
    // left-to-right and hundreds of tiny items never collapse into horizontal stripes.
    let proportional_width = bounds.width() * (aggregate_size as f32 / total_weight as f32);
    let minimum_width = 72.0_f32.min(bounds.width() * 0.35);
    let other_width = proportional_width
        .max(minimum_width)
        .min(bounds.width() * 0.65)
        .min(bounds.width() - 2.0);
    let split_x = bounds.max_x - other_width;
    (
        Rect::new(bounds.min_x, bounds.min_y, split_x, bounds.max_y),
        Some(Rect::new(split_x, bounds.min_y, bounds.max_x, bounds.max_y)),
    )
}

#[cfg(test)]
mod aggregation_tests {
    use super::*;

    #[test]
    fn other_is_always_a_right_hand_column() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 500.0);
        let (content, other) = split_other_on_right(bounds, 100, 1_000);
        let other = other.expect("aggregate rectangle");

        assert_eq!(content.max_x, other.min_x);
        assert_eq!(other.max_x, bounds.max_x);
        assert_eq!(other.min_y, bounds.min_y);
        assert_eq!(other.max_y, bounds.max_y);
        assert!(other.width() >= 72.0);
    }
}
