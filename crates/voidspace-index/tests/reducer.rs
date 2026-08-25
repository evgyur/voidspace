use voidspace_index::Index;
use voidspace_model::{
    EventEnvelope, EventPayload, FileIdentity, NodeFlags, NodeKind, ProducerId, RemoveNode, ScanId,
    SizeMetrics, SourceRevision, UpsertNode, VolumeId, WinName,
};

fn identity(value: u128) -> FileIdentity {
    FileIdentity::stable(VolumeId::local_for_test(7), value, 1)
}

fn remove(sequence: u64, parent: FileIdentity, id: FileIdentity, name: WinName) -> EventEnvelope {
    EventEnvelope {
        schema_version: 1,
        scan_id: ScanId(1),
        generation: 1,
        branch_epoch: None,
        producer: ProducerId(1),
        sequence,
        observed_at_qpc: sequence,
        cause_operation: None,
        payload: EventPayload::RemoveNode(RemoveNode {
            parent,
            identity: id,
            name,
        }),
    }
}

fn upsert(
    sequence: u64,
    parent: FileIdentity,
    id: FileIdentity,
    name: WinName,
    bytes: u64,
) -> EventEnvelope {
    EventEnvelope {
        schema_version: 1,
        scan_id: ScanId(1),
        generation: 1,
        branch_epoch: None,
        producer: ProducerId(1),
        sequence,
        observed_at_qpc: sequence,
        cause_operation: None,
        payload: EventPayload::UpsertNode(UpsertNode {
            parent,
            identity: id,
            name,
            kind: NodeKind::File,
            sizes: SizeMetrics::new(bytes, bytes),
            flags: NodeFlags::empty(),
            revision: SourceRevision::new(ProducerId(1), 1, sequence),
        }),
    }
}

#[test]
fn preserves_unpaired_surrogate_and_propagates_size_delta() {
    let root_identity = identity(1);
    let mut index = Index::new(ScanId(1), 1, root_identity.clone(), WinName::from("C:"));
    let name = WinName::from_units(vec![0x0061, 0xD800]).unwrap();

    let dirty = index
        .apply(upsert(1, root_identity, identity(2), name.clone(), 4096))
        .unwrap();
    let child = dirty.changed_nodes[0];

    assert_eq!(index.node(child).unwrap().name, name);
    assert_eq!(index.node(child).unwrap().allocated, 4096);
    assert_eq!(index.node(index.root()).unwrap().allocated, 4096);
}

#[test]
fn stages_child_until_parent_arrives() {
    let root_identity = identity(1);
    let parent_identity = identity(2);
    let mut index = Index::new(ScanId(1), 1, root_identity.clone(), WinName::from("C:"));

    let pending = index
        .apply(upsert(
            1,
            parent_identity.clone(),
            identity(3),
            WinName::from("late.txt"),
            10,
        ))
        .unwrap();
    assert!(pending.changed_nodes.is_empty());

    index
        .apply(upsert(
            2,
            root_identity,
            parent_identity,
            WinName::from("parent"),
            0,
        ))
        .unwrap();

    assert!(
        index
            .find_child(index.root(), &WinName::from("parent"))
            .is_some()
    );
    assert_eq!(index.snapshot().nodes.len(), 3);
}

#[test]
fn duplicate_sequence_is_idempotent() {
    let root_identity = identity(1);
    let child_identity = identity(2);
    let mut index = Index::new(ScanId(1), 1, root_identity.clone(), WinName::from("C:"));
    let event = upsert(1, root_identity, child_identity, WinName::from("x"), 10);
    index.apply(event.clone()).unwrap();
    let second = index.apply(event).unwrap();
    assert!(second.is_empty());
    assert_eq!(index.node(index.root()).unwrap().allocated, 10);
}

#[test]
fn hard_links_count_allocation_once_and_promote_owner() {
    let root_identity = identity(1);
    let shared_identity = identity(2);
    let mut index = Index::new(ScanId(1), 1, root_identity.clone(), WinName::from("C:"));
    index
        .apply(upsert(
            1,
            root_identity.clone(),
            shared_identity.clone(),
            WinName::from("a.bin"),
            100,
        ))
        .unwrap();
    index
        .apply(upsert(
            2,
            root_identity.clone(),
            shared_identity.clone(),
            WinName::from("b.bin"),
            100,
        ))
        .unwrap();
    assert_eq!(index.node(index.root()).unwrap().logical, 200);
    assert_eq!(index.node(index.root()).unwrap().allocated, 100);

    index
        .apply(remove(
            3,
            root_identity,
            shared_identity,
            WinName::from("a.bin"),
        ))
        .unwrap();
    assert_eq!(index.node(index.root()).unwrap().logical, 100);
    assert_eq!(index.node(index.root()).unwrap().allocated, 100);
}
