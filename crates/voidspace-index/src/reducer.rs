use std::collections::{HashMap, HashSet};

use thiserror::Error;
use voidspace_model::{
    DirtySet, EventEnvelope, EventPayload, FileIdentity, MODEL_SCHEMA_VERSION, NodeFlags, NodeId,
    NodeKind, ProducerId, ScanId, UpsertNode, WinName,
};

use crate::{Arena, IndexSnapshot, Node, create_snapshot};

const MAX_PENDING_PARENT_EVENTS: usize = 65_536;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReduceError {
    #[error("event belongs to another scan")]
    WrongScan,
    #[error("event belongs to generation {actual}, expected {expected}")]
    WrongGeneration { expected: u64, actual: u64 },
    #[error("unsupported model schema {0}")]
    UnsupportedSchema(u16),
    #[error("pending-parent buffer is full")]
    PendingOverflow,
    #[error("aggregate overflow")]
    AggregateOverflow,
}

pub struct Index {
    scan_id: ScanId,
    generation: u64,
    index_version: u64,
    root: NodeId,
    arena: Arena,
    identity_nodes: HashMap<FileIdentity, Vec<NodeId>>,
    children: HashMap<(NodeId, WinName), NodeId>,
    last_sequence: HashMap<ProducerId, u64>,
    pending: HashMap<FileIdentity, Vec<UpsertNode>>,
    pending_len: usize,
}

impl Index {
    pub fn new(
        scan_id: ScanId,
        generation: u64,
        root_identity: FileIdentity,
        root_name: WinName,
    ) -> Self {
        let mut arena = Arena::default();
        let root = arena.insert(Node {
            id: NodeId(0),
            parent: None,
            children: Vec::new(),
            name: root_name,
            identity: root_identity.clone(),
            kind: NodeKind::Directory,
            flags: NodeFlags::empty(),
            own_logical: 0,
            own_allocated: 0,
            physical_allocated: 0,
            logical: 0,
            allocated: 0,
        });
        Self {
            scan_id,
            generation,
            index_version: 0,
            root,
            arena,
            identity_nodes: HashMap::from([(root_identity, vec![root])]),
            children: HashMap::new(),
            last_sequence: HashMap::new(),
            pending: HashMap::new(),
            pending_len: 0,
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.arena.get(id)
    }

    pub fn find_child(&self, parent: NodeId, name: &WinName) -> Option<NodeId> {
        self.children.get(&(parent, name.clone())).copied()
    }

    pub fn snapshot(&self) -> IndexSnapshot {
        create_snapshot(
            &self.arena,
            self.scan_id,
            self.generation,
            self.index_version,
            self.root,
        )
    }

    pub fn apply(&mut self, event: EventEnvelope) -> Result<DirtySet, ReduceError> {
        self.validate(&event)?;
        if self
            .last_sequence
            .get(&event.producer)
            .is_some_and(|last| event.sequence <= *last)
        {
            return Ok(DirtySet::default());
        }
        self.last_sequence.insert(event.producer, event.sequence);

        let mut dirty = match event.payload {
            EventPayload::UpsertNode(upsert) => self.apply_upsert(upsert)?,
            EventPayload::RemoveNode(remove) => self.remove_child(&remove.parent, &remove.name)?,
            EventPayload::Invalidate(invalidate) => {
                let mut dirty = DirtySet::default();
                if let Some(node) = self.first_identity_node(&invalidate.minimal_scope) {
                    dirty.layout_roots.push(node);
                }
                dirty
            }
            EventPayload::BaselineStarted
            | EventPayload::DirectoryEnumerated(_)
            | EventPayload::BaselineFinished(_) => DirtySet::default(),
        };

        if !dirty.is_empty() || !dirty.layout_roots.is_empty() {
            self.index_version = self.index_version.saturating_add(1);
            dirty.index_version = self.index_version;
        }
        Ok(dirty)
    }

    fn validate(&self, event: &EventEnvelope) -> Result<(), ReduceError> {
        if event.schema_version != MODEL_SCHEMA_VERSION {
            return Err(ReduceError::UnsupportedSchema(event.schema_version));
        }
        if event.scan_id != self.scan_id {
            return Err(ReduceError::WrongScan);
        }
        if event.generation != self.generation {
            return Err(ReduceError::WrongGeneration {
                expected: self.generation,
                actual: event.generation,
            });
        }
        Ok(())
    }

    fn apply_upsert(&mut self, upsert: UpsertNode) -> Result<DirtySet, ReduceError> {
        let Some(parent_id) = self.first_identity_node(&upsert.parent) else {
            if self.pending_len >= MAX_PENDING_PARENT_EVENTS {
                return Err(ReduceError::PendingOverflow);
            }
            self.pending
                .entry(upsert.parent.clone())
                .or_default()
                .push(upsert);
            self.pending_len += 1;
            return Ok(DirtySet::default());
        };
        self.insert_or_update(parent_id, upsert)
    }

    fn insert_or_update(
        &mut self,
        parent_id: NodeId,
        upsert: UpsertNode,
    ) -> Result<DirtySet, ReduceError> {
        if let Some(existing_id) = self.find_child(parent_id, &upsert.name) {
            let existing_identity = self
                .arena
                .get(existing_id)
                .expect("child map references live node")
                .identity
                .clone();
            if existing_identity == upsert.identity {
                return self.update_existing(existing_id, upsert);
            }
            self.remove_node(existing_id)?;
        }

        let is_shared = upsert.kind == NodeKind::File
            && self
                .identity_nodes
                .get(&upsert.identity)
                .is_some_and(|nodes| !nodes.is_empty());
        let counted_allocated = if is_shared { 0 } else { upsert.sizes.allocated };
        let mut flags = upsert.flags;
        flags.set(NodeFlags::SHARED_ALLOCATION, is_shared);
        let identity = upsert.identity.clone();
        let name = upsert.name.clone();
        let id = self.arena.insert(Node {
            id: NodeId(0),
            parent: Some(parent_id),
            children: Vec::new(),
            name: upsert.name,
            identity: identity.clone(),
            kind: upsert.kind,
            flags,
            own_logical: upsert.sizes.logical,
            own_allocated: counted_allocated,
            physical_allocated: upsert.sizes.allocated,
            logical: upsert.sizes.logical,
            allocated: counted_allocated,
        });
        self.arena
            .get_mut(parent_id)
            .expect("parent identity references live node")
            .children
            .push(id);
        self.children.insert((parent_id, name), id);
        self.identity_nodes
            .entry(identity.clone())
            .or_default()
            .push(id);

        let mut dirty = DirtySet {
            changed_nodes: vec![id],
            layout_roots: vec![parent_id],
            ..DirtySet::default()
        };
        dirty.changed_ancestors = self.propagate(
            Some(parent_id),
            i128::from(upsert.sizes.logical),
            i128::from(counted_allocated),
        )?;

        if let Some(pending) = self.pending.remove(&identity) {
            self.pending_len -= pending.len();
            for child in pending {
                dirty.merge(self.insert_or_update(id, child)?);
            }
        }
        Ok(dirty)
    }

    fn update_existing(&mut self, id: NodeId, upsert: UpsertNode) -> Result<DirtySet, ReduceError> {
        let node = self.arena.get_mut(id).expect("existing child is live");
        let logical_delta = i128::from(upsert.sizes.logical) - i128::from(node.own_logical);
        let allocated_delta = if node.flags.contains(NodeFlags::SHARED_ALLOCATION) {
            0
        } else {
            i128::from(upsert.sizes.allocated) - i128::from(node.own_allocated)
        };
        node.kind = upsert.kind;
        node.flags =
            upsert.flags | (node.flags & (NodeFlags::SHARED_ALLOCATION | NodeFlags::TOMBSTONE));
        node.own_logical = upsert.sizes.logical;
        node.physical_allocated = upsert.sizes.allocated;
        node.own_allocated = add_signed(node.own_allocated, allocated_delta)?;
        node.logical = add_signed(node.logical, logical_delta)?;
        node.allocated = add_signed(node.allocated, allocated_delta)?;
        let parent = node.parent;

        Ok(DirtySet {
            index_version: 0,
            changed_nodes: vec![id],
            changed_ancestors: self.propagate(parent, logical_delta, allocated_delta)?,
            removed_nodes: Vec::new(),
            layout_roots: parent.into_iter().collect(),
        })
    }

    fn remove_child(
        &mut self,
        parent_identity: &FileIdentity,
        name: &WinName,
    ) -> Result<DirtySet, ReduceError> {
        let Some(parent) = self.first_identity_node(parent_identity) else {
            return Ok(DirtySet::default());
        };
        let Some(id) = self.find_child(parent, name) else {
            return Ok(DirtySet::default());
        };
        let ancestors = self.remove_node(id)?;
        Ok(DirtySet {
            removed_nodes: vec![id],
            changed_ancestors: ancestors,
            layout_roots: vec![parent],
            ..DirtySet::default()
        })
    }

    fn remove_node(&mut self, id: NodeId) -> Result<Vec<NodeId>, ReduceError> {
        let Some(node) = self.arena.get(id).cloned() else {
            return Ok(Vec::new());
        };
        if let Some(parent) = node.parent {
            if let Some(parent_node) = self.arena.get_mut(parent) {
                parent_node.children.retain(|child| *child != id);
            }
            self.children.remove(&(parent, node.name.clone()));
        }
        let mut removed_owners = Vec::new();
        self.remove_subtree_entries(id, &mut removed_owners);
        let mut changed = self.propagate(
            node.parent,
            -i128::from(node.logical),
            -i128::from(node.allocated),
        )?;
        for (identity, allocation) in removed_owners {
            let Some(replacement) = self.first_identity_node(&identity) else {
                continue;
            };
            let replacement_node = self
                .arena
                .get_mut(replacement)
                .expect("identity map references live replacement");
            if replacement_node.own_allocated != 0 {
                continue;
            }
            let promoted = allocation.min(replacement_node.physical_allocated);
            replacement_node.own_allocated = promoted;
            replacement_node.allocated = replacement_node
                .allocated
                .checked_add(promoted)
                .ok_or(ReduceError::AggregateOverflow)?;
            let parent = replacement_node.parent;
            changed.extend(self.propagate(parent, 0, i128::from(promoted))?);
        }
        changed.sort_unstable();
        changed.dedup();
        Ok(changed)
    }

    fn remove_subtree_entries(
        &mut self,
        id: NodeId,
        removed_owners: &mut Vec<(FileIdentity, u64)>,
    ) {
        let Some(node) = self.arena.take(id) else {
            return;
        };
        for child in node.children.clone() {
            self.remove_subtree_entries(child, removed_owners);
        }
        if node.own_allocated > 0 && node.kind == NodeKind::File {
            removed_owners.push((node.identity.clone(), node.own_allocated));
        }
        if let Some(parent) = node.parent {
            self.children.remove(&(parent, node.name.clone()));
        }
        if let Some(nodes) = self.identity_nodes.get_mut(&node.identity) {
            nodes.retain(|node_id| *node_id != id);
            if nodes.is_empty() {
                self.identity_nodes.remove(&node.identity);
            }
        }
    }

    fn first_identity_node(&self, identity: &FileIdentity) -> Option<NodeId> {
        self.identity_nodes
            .get(identity)
            .and_then(|nodes| nodes.first())
            .copied()
    }

    fn propagate(
        &mut self,
        mut current: Option<NodeId>,
        logical_delta: i128,
        allocated_delta: i128,
    ) -> Result<Vec<NodeId>, ReduceError> {
        let mut changed = Vec::new();
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id) {
                break;
            }
            let node = self.arena.get_mut(id).expect("ancestor is live");
            node.logical = add_signed(node.logical, logical_delta)?;
            node.allocated = add_signed(node.allocated, allocated_delta)?;
            current = node.parent;
            changed.push(id);
        }
        Ok(changed)
    }
}

fn add_signed(value: u64, delta: i128) -> Result<u64, ReduceError> {
    let next = i128::from(value)
        .checked_add(delta)
        .ok_or(ReduceError::AggregateOverflow)?;
    u64::try_from(next).map_err(|_| ReduceError::AggregateOverflow)
}
