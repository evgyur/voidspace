use serde::{Deserialize, Serialize};

use crate::{
    FileIdentity, NodeFlags, NodeId, NodeKind, OperationId, ProducerId, ScanId, SizeMetrics,
    WinName,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRevision {
    pub producer: ProducerId,
    pub generation: u64,
    pub sequence: u64,
}

impl SourceRevision {
    pub const fn new(producer: ProducerId, generation: u64, sequence: u64) -> Self {
        Self {
            producer,
            generation,
            sequence,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub schema_version: u16,
    pub scan_id: ScanId,
    pub generation: u64,
    pub branch_epoch: Option<u64>,
    pub producer: ProducerId,
    pub sequence: u64,
    pub observed_at_qpc: u64,
    pub cause_operation: Option<OperationId>,
    pub payload: EventPayload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventPayload {
    BaselineStarted,
    UpsertNode(UpsertNode),
    RemoveNode(RemoveNode),
    DirectoryEnumerated(DirectoryEnumerated),
    BaselineFinished(BaselineFinished),
    Invalidate(Invalidate),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpsertNode {
    pub parent: FileIdentity,
    pub identity: FileIdentity,
    pub name: WinName,
    pub kind: NodeKind,
    pub sizes: SizeMetrics,
    pub flags: NodeFlags,
    pub revision: SourceRevision,
}

impl UpsertNode {
    pub fn simple(
        parent: FileIdentity,
        identity: FileIdentity,
        name: WinName,
        kind: NodeKind,
        sizes: SizeMetrics,
        revision: SourceRevision,
    ) -> Self {
        Self {
            parent,
            identity,
            name,
            kind,
            sizes,
            flags: NodeFlags::empty(),
            revision,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoveNode {
    pub parent: FileIdentity,
    pub identity: FileIdentity,
    pub name: WinName,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectoryEnumerated {
    pub directory: FileIdentity,
    pub enumeration_epoch: u64,
    pub sorted_child_identities: Vec<FileIdentity>,
    pub fingerprint: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaselineFinished {
    pub captured_cursor: Option<u64>,
    pub root_fingerprint: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invalidate {
    pub minimal_scope: FileIdentity,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DirtySet {
    pub index_version: u64,
    pub changed_nodes: Vec<NodeId>,
    pub changed_ancestors: Vec<NodeId>,
    pub removed_nodes: Vec<NodeId>,
    pub layout_roots: Vec<NodeId>,
}

impl DirtySet {
    pub fn is_empty(&self) -> bool {
        self.changed_nodes.is_empty()
            && self.changed_ancestors.is_empty()
            && self.removed_nodes.is_empty()
    }

    pub fn merge(&mut self, mut other: Self) {
        self.index_version = self.index_version.max(other.index_version);
        self.changed_nodes.append(&mut other.changed_nodes);
        self.changed_ancestors.append(&mut other.changed_ancestors);
        self.removed_nodes.append(&mut other.removed_nodes);
        self.layout_roots.append(&mut other.layout_roots);
        self.changed_nodes.sort_unstable();
        self.changed_nodes.dedup();
        self.changed_ancestors.sort_unstable();
        self.changed_ancestors.dedup();
        self.removed_nodes.sort_unstable();
        self.removed_nodes.dedup();
        self.layout_roots.sort_unstable();
        self.layout_roots.dedup();
    }
}
