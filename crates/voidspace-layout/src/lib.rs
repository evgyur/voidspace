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
    let weights: Vec<u64> = children
        .iter()
        .map(|(_, node)| match view.size_mode {
            SizeMode::Allocated => node.allocated,
            SizeMode::Logical => node.logical,
        })
        .collect();
    let rectangles = layout_weights(&weights, bounds);
    for (((node_id, _), rect), _) in children
        .into_iter()
        .zip(rectangles)
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
        });
        layout_children(snapshot, view, node_id, rect, depth + 1, output);
    }
}
