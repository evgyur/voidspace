use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voidspace_model::{FileIdentity, NodeFlags, NodeId, NodeKind, ScanId, WinName};

use crate::Arena;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub name: WinName,
    pub identity: FileIdentity,
    pub kind: NodeKind,
    pub flags: NodeFlags,
    pub logical: u64,
    pub allocated: u64,
    pub physical_allocated: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub scan_id: ScanId,
    pub generation: u64,
    pub index_version: u64,
    pub root: NodeId,
    pub nodes: Arc<Vec<NodeSnapshot>>,
}

impl IndexSnapshot {
    pub fn node(&self, id: NodeId) -> Option<&NodeSnapshot> {
        self.nodes
            .binary_search_by_key(&id, |node| node.id)
            .ok()
            .map(|index| &self.nodes[index])
    }
}

pub(crate) fn create_snapshot(
    arena: &Arena,
    scan_id: ScanId,
    generation: u64,
    index_version: u64,
    root: NodeId,
) -> IndexSnapshot {
    let nodes = arena
        .iter()
        .map(|node| NodeSnapshot {
            id: node.id,
            parent: node.parent,
            children: node.children.clone(),
            name: node.name.clone(),
            identity: node.identity.clone(),
            kind: node.kind,
            flags: node.flags,
            logical: node.logical,
            allocated: node.allocated,
            physical_allocated: node.physical_allocated,
        })
        .collect();
    IndexSnapshot {
        scan_id,
        generation,
        index_version,
        root,
        nodes: Arc::new(nodes),
    }
}
