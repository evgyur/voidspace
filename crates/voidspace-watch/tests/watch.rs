use std::{fs, time::Duration};

use crossbeam_channel::bounded;
use tempfile::tempdir;
use voidspace_watch::{WatchRequest, WatchSignal, common_ancestor, watch};

#[test]
fn computes_minimal_common_ancestor() {
    let root = std::path::Path::new("C:\\root");
    let paths = vec![root.join("a").join("one"), root.join("a").join("two")];
    assert_eq!(common_ancestor(root, &paths), root.join("a"));
}

#[test]
fn reports_a_local_mutation() {
    let temp = tempdir().unwrap();
    let (tx, rx) = bounded(32);
    let _handle = watch(
        WatchRequest {
            root: temp.path().to_path_buf(),
        },
        tx,
    )
    .unwrap();
    fs::write(temp.path().join("created.txt"), b"live").unwrap();
    let signal = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(matches!(signal, WatchSignal::Changed { .. }));
}
