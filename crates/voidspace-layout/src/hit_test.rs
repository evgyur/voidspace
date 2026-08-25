use voidspace_model::NodeId;

use crate::LayoutSnapshot;

pub fn hit_test(layout: &LayoutSnapshot, x: f32, y: f32) -> Option<NodeId> {
    layout
        .nodes
        .iter()
        .rev()
        .find(|node| node.rect.contains_point(x, y))
        .map(|node| node.node_id)
}
