use serde::{Deserialize, Serialize};
use voidspace_model::{FileIdentity, NodeFlags, NodeId, NodeKind, WinName};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub name: WinName,
    pub identity: FileIdentity,
    pub kind: NodeKind,
    pub flags: NodeFlags,
    pub own_logical: u64,
    pub own_allocated: u64,
    pub physical_allocated: u64,
    pub logical: u64,
    pub allocated: u64,
}

impl Node {
    pub fn is_directory(&self) -> bool {
        self.kind == NodeKind::Directory
    }
}

#[derive(Clone, Debug, Default)]
pub struct Arena {
    nodes: Vec<Option<Node>>,
}

impl Arena {
    pub fn insert(&mut self, mut node: Node) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("node arena exceeded u32"));
        node.id = id;
        self.nodes.push(Some(node));
        id
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize)?.as_ref()
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0 as usize)?.as_mut()
    }

    pub fn take(&mut self, id: NodeId) -> Option<Node> {
        self.nodes.get_mut(id.0 as usize)?.take()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter_map(Option::as_ref)
    }

    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|node| node.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
