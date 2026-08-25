use std::fs;

use tempfile::tempdir;
use voidspace_fileops::{
    CancellationToken, FileOpError, OperationDraft, OperationKind, confirm, execute, prepare,
};

#[test]
fn permanent_delete_requires_exact_phrase_and_removes_only_selected_subtree() {
    let sandbox = tempdir().unwrap();
    let keep = sandbox.path().join("keep.txt");
    let remove = sandbox.path().join("remove");
    fs::write(&keep, b"keep").unwrap();
    fs::create_dir(&remove).unwrap();
    fs::write(remove.join("gone.bin"), b"gone").unwrap();
    let canonical_remove = fs::canonicalize(&remove).unwrap();

    let prepared = prepare(OperationDraft {
        kind: OperationKind::Permanent,
        paths: vec![remove.clone()],
    })
    .unwrap();
    assert!(matches!(
        confirm(prepared.clone(), "delete"),
        Err(FileOpError::ConfirmationRejected)
    ));
    let confirmed = confirm(prepared, "DELETE").unwrap();
    let report = execute(confirmed, None, &CancellationToken::default()).unwrap();

    assert_eq!(report.deleted, vec![canonical_remove]);
    assert!(!remove.exists());
    assert!(keep.exists());
}

#[test]
fn changed_manifest_fails_closed() {
    let sandbox = tempdir().unwrap();
    let remove = sandbox.path().join("remove");
    fs::create_dir(&remove).unwrap();
    fs::write(remove.join("first.bin"), b"first").unwrap();
    let prepared = prepare(OperationDraft {
        kind: OperationKind::Permanent,
        paths: vec![remove.clone()],
    })
    .unwrap();
    let confirmed = confirm(prepared, "DELETE").unwrap();
    fs::write(remove.join("appeared-after-confirmation.bin"), b"new").unwrap();

    assert!(matches!(
        execute(confirmed, None, &CancellationToken::default()),
        Err(FileOpError::ManifestChanged)
    ));
    assert!(remove.exists());
}

#[test]
fn filesystem_root_is_rejected() {
    let root = std::path::PathBuf::from(r"C:\");
    assert!(matches!(
        prepare(OperationDraft {
            kind: OperationKind::Permanent,
            paths: vec![root],
        }),
        Err(FileOpError::RootRejected(_))
    ));
}
