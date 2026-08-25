use std::{fs, time::Duration};

use crossbeam_channel::bounded;
use tempfile::tempdir;
use voidspace_model::EventPayload;
use voidspace_scan::{ScanRequest, describe_root, start};

#[test]
fn streams_a_complete_tree_in_protocol_order() {
    let temp = tempdir().unwrap();
    fs::create_dir(temp.path().join("nested")).unwrap();
    fs::write(temp.path().join("a.bin"), vec![1_u8; 128]).unwrap();
    fs::write(temp.path().join("nested").join("b.txt"), b"hello").unwrap();
    let root = describe_root(temp.path(), 1).unwrap();
    let (tx, rx) = bounded(128);
    let handle = start(ScanRequest::new(1, 1, temp.path().to_path_buf()), tx).unwrap();
    assert!(handle.join().unwrap().errors.is_empty());

    let events: Vec<_> = rx.try_iter().collect();
    assert!(matches!(
        events.first().unwrap().payload,
        EventPayload::BaselineStarted
    ));
    assert!(matches!(
        events.last().unwrap().payload,
        EventPayload::BaselineFinished(_)
    ));
    assert!(events.iter().any(|event| matches!(
        &event.payload,
        EventPayload::UpsertNode(node) if node.parent == root.identity && node.name.display_escaped() == "a.bin"
    )));
}

#[test]
fn cancellation_is_cooperative() {
    let temp = tempdir().unwrap();
    for index in 0..500 {
        fs::write(temp.path().join(format!("{index}.bin")), [0_u8; 16]).unwrap();
    }
    let (tx, _rx) = bounded(4);
    let handle = start(ScanRequest::new(2, 1, temp.path().to_path_buf()), tx).unwrap();
    handle.cancel();
    let stats = handle.join_timeout(Duration::from_secs(2)).unwrap();
    assert!(stats.cancelled);
}
