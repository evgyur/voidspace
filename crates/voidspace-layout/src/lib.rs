//! Stable squarified treemap layout.

mod hit_test;
mod squarify;

pub use hit_test::*;
pub use squarify::*;

use serde::{Deserialize, Serialize};
use voidspace_index::IndexSnapshot;
use voidspace_model::{DirtySet, NodeId};

pub const LAYOUT_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SizeMode {
    Allocated,
    Logical,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabelFootprint {
    pub width: f32,
    pub height: f32,
}

impl LabelFootprint {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn fits(self, rect: Rect) -> bool {
        rect.width() >= self.width && rect.height() >= self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewState {
    pub root: NodeId,
    pub bounds: Rect,
    pub size_mode: SizeMode,
    pub max_depth: u8,
    pub min_area: f32,
    pub min_label: LabelFootprint,
    /// Global budget for real, non-root, non-aggregate rectangles.
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
    #[serde(default)]
    pub aggregates: Vec<AggregateGroup>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AggregateGroup {
    pub parent_id: NodeId,
    pub depth: u8,
    pub member_ids: Vec<NodeId>,
    pub size: u64,
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
    let mut aggregates = Vec::new();
    let mut real_budget = view.max_rectangles;
    layout_children(
        snapshot,
        view,
        view.root,
        view.bounds,
        1,
        &mut real_budget,
        &mut output,
        &mut aggregates,
    );
    LayoutSnapshot {
        index_version: snapshot.index_version,
        root: view.root,
        nodes: output,
        aggregates,
    }
}

fn layout_children(
    snapshot: &IndexSnapshot,
    view: &ViewState,
    parent: NodeId,
    bounds: Rect,
    depth: u8,
    real_budget: &mut usize,
    output: &mut Vec<LayoutNode>,
    aggregates: &mut Vec<AggregateGroup>,
) {
    if depth > view.max_depth {
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
    let partition = partition_children(
        &child_weights,
        bounds,
        view.min_label,
        view.min_area,
        *real_budget,
    );
    let keep_count = partition.aggregate_start;
    let kept = children
        .iter()
        .take(keep_count)
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    *real_budget = real_budget.saturating_sub(keep_count);
    let mut descendant_budget = *real_budget;

    for (node_id, rect) in kept.into_iter().zip(partition.rectangles.iter().copied()) {
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
        layout_children(
            snapshot,
            view,
            node_id,
            child_bounds,
            depth + 1,
            &mut descendant_budget,
            output,
            aggregates,
        );
    }
    *real_budget = descendant_budget;

    if partition.aggregate_count > 0 {
        let member_ids = children[partition.aggregate_start..]
            .iter()
            .map(|(node_id, _)| *node_id)
            .collect::<Vec<_>>();
        aggregates.push(AggregateGroup {
            parent_id: parent,
            depth,
            member_ids: member_ids.clone(),
            size: partition.aggregate_size,
        });
        if let Some(rect) = partition.aggregate_rect {
            output.push(LayoutNode {
                node_id: parent,
                parent_id: Some(parent),
                rect,
                depth,
                aggregated: true,
                aggregate_count: member_ids.len().min(u32::MAX as usize) as u32,
                aggregate_size: partition.aggregate_size,
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ChildPartition {
    rectangles: Vec<Rect>,
    aggregate_rect: Option<Rect>,
    aggregate_start: usize,
    aggregate_count: usize,
    aggregate_size: u64,
}

fn partition_children(
    weights: &[u64],
    bounds: Rect,
    min_label: LabelFootprint,
    min_area: f32,
    real_budget: usize,
) -> ChildPartition {
    let mut keep = weights.len().min(real_budget).min(128);
    loop {
        let aggregate_size = weights[keep..]
            .iter()
            .copied()
            .fold(0_u64, u64::saturating_add);
        let has_other = keep < weights.len() && aggregate_size > 0;
        let (content, aggregate_rect) = if has_other && keep == 0 {
            (bounds, Some(bounds))
        } else if has_other {
            split_other_on_right(
                bounds,
                aggregate_size,
                weights
                    .iter()
                    .copied()
                    .fold(0_u128, |sum, value| sum.saturating_add(u128::from(value))),
            )
        } else {
            (bounds, None)
        };
        let rectangles = layout_weights(&weights[..keep], content);
        let real_fit = rectangles
            .iter()
            .copied()
            .all(|rect| min_label.fits(rect) && rect.area() >= min_area);
        let aggregate_fit = aggregate_rect.is_none_or(|rect| min_label.fits(rect));
        if real_fit && aggregate_fit {
            return ChildPartition {
                rectangles,
                aggregate_rect,
                aggregate_start: keep,
                aggregate_count: weights.len() - keep,
                aggregate_size,
            };
        }
        if keep == 0 {
            return ChildPartition {
                rectangles: Vec::new(),
                aggregate_rect: None,
                aggregate_start: 0,
                aggregate_count: weights.len(),
                aggregate_size,
            };
        }
        keep -= 1;
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

#[cfg(test)]
mod footprint_tests {
    use super::*;
    use proptest::prelude::*;

    const BOUNDS: Rect = Rect::new(0.0, 0.0, 400.0, 240.0);
    const LABEL: LabelFootprint = LabelFootprint::new(54.0, 22.0);

    #[test]
    fn undersized_rectangles_move_to_one_suffix_aggregate() {
        let weights = [600, 220, 80, 40, 20, 10];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 128);

        assert!(partition.rectangles.iter().all(|rect| LABEL.fits(*rect)));
        assert_eq!(partition.aggregate_start, partition.rectangles.len());
        assert_eq!(
            partition.aggregate_count,
            weights.len() - partition.aggregate_start
        );
        assert_eq!(
            partition.aggregate_size,
            weights[partition.aggregate_start..]
                .iter()
                .copied()
                .fold(0_u64, u64::saturating_add)
        );
    }

    #[test]
    fn zero_real_tile_budget_still_produces_structural_other() {
        let weights = [8, 5, 3];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 0);

        assert!(partition.rectangles.is_empty());
        assert_eq!(partition.aggregate_start, 0);
        assert_eq!(partition.aggregate_count, 3);
        assert_eq!(partition.aggregate_size, 16);
        assert!(
            partition
                .aggregate_rect
                .is_some_and(|rect| LABEL.fits(rect))
        );
    }

    #[test]
    fn one_real_tile_budget_keeps_the_largest_and_aggregates_the_exact_suffix() {
        let weights = [90, 60, 30, 10];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 1);

        assert_eq!(partition.aggregate_start, 1);
        assert_eq!(partition.aggregate_count, 3);
        assert_eq!(partition.aggregate_size, 100);
    }

    #[test]
    fn canvas_smaller_than_the_label_footprint_emits_no_child_rectangle() {
        let tiny = Rect::new(0.0, 0.0, 30.0, 12.0);
        let partition = partition_children(&[8, 5, 3], tiny, LABEL, 196.0, 0);
        assert!(partition.rectangles.is_empty());
        assert!(partition.aggregate_rect.is_none());
        assert_eq!(partition.aggregate_count, 3);
        assert_eq!(partition.aggregate_size, 16);
    }

    #[test]
    fn conservation_uses_saturating_u64_arithmetic() {
        let weights = [u64::MAX, 10, 5];
        let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, 1);
        let visible = weights[..partition.aggregate_start]
            .iter()
            .copied()
            .fold(0_u64, u64::saturating_add);

        assert_eq!(
            visible.saturating_add(partition.aggregate_size),
            weights.iter().copied().fold(0_u64, u64::saturating_add)
        );
    }

    proptest! {
        #[test]
        fn partition_is_labeled_budgeted_and_conservative(
            mut weights in prop::collection::vec(1_u64..1_000_000, 1..200),
            budget in 0_usize..64,
        ) {
            weights.sort_unstable_by(|left, right| right.cmp(left));
            let partition = partition_children(&weights, BOUNDS, LABEL, 196.0, budget);
            prop_assert!(partition.rectangles.len() <= budget);
            prop_assert!(partition.rectangles.iter().copied().all(|rect| LABEL.fits(rect)));
            prop_assert_eq!(partition.aggregate_start, partition.rectangles.len());
            let visible = weights[..partition.aggregate_start]
                .iter().copied().fold(0_u64, u64::saturating_add);
            let total = weights.iter().copied().fold(0_u64, u64::saturating_add);
            prop_assert_eq!(visible.saturating_add(partition.aggregate_size), total);

            let mut all_rectangles = partition.rectangles.clone();
            all_rectangles.extend(partition.aggregate_rect);
            prop_assert!(all_rectangles.iter().all(|rect| BOUNDS.contains(*rect)));
            for left in 0..all_rectangles.len() {
                for right in (left + 1)..all_rectangles.len() {
                    let overlap_width = all_rectangles[left].max_x.min(all_rectangles[right].max_x)
                        - all_rectangles[left].min_x.max(all_rectangles[right].min_x);
                    let overlap_height = all_rectangles[left].max_y.min(all_rectangles[right].max_y)
                        - all_rectangles[left].min_y.max(all_rectangles[right].min_y);
                    prop_assert!(overlap_width <= f32::EPSILON || overlap_height <= f32::EPSILON);
                }
            }
            prop_assert_eq!(partition_children(&weights, BOUNDS, LABEL, 196.0, budget), partition);
        }
    }
}
