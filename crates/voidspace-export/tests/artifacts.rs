use std::sync::Arc;

use tempfile::tempdir;
use voidspace_export::{ExportError, ReportFormat, export_report, load_snapshot, save_snapshot};
use voidspace_index::{IndexSnapshot, NodeSnapshot};
use voidspace_model::{FileIdentity, NodeFlags, NodeId, NodeKind, ScanId, VolumeId, WinName};

fn snapshot(name: WinName) -> IndexSnapshot {
    let identity = FileIdentity::stable(VolumeId::Session(1), 1, 1);
    IndexSnapshot {
        scan_id: ScanId(1),
        generation: 1,
        index_version: 1,
        root: NodeId(0),
        nodes: Arc::new(vec![NodeSnapshot {
            id: NodeId(0),
            parent: None,
            children: Vec::new(),
            name,
            identity,
            kind: NodeKind::Directory,
            flags: NodeFlags::empty(),
            logical: 12,
            allocated: 4096,
            physical_allocated: 0,
        }]),
    }
}

#[test]
fn snapshot_round_trip_preserves_invalid_utf16_and_detects_corruption() {
    let sandbox = tempdir().unwrap();
    let path = sandbox.path().join("scan.voidspace");
    let expected = snapshot(WinName::from_units(vec![b'a' as u16, 0xD800]).unwrap());
    save_snapshot(&path, &expected).unwrap();
    let loaded = load_snapshot(&path).unwrap();
    assert_eq!(loaded.nodes[0].name.units(), &[b'a' as u16, 0xD800]);

    let mut bytes = std::fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 0x55;
    std::fs::write(&path, bytes).unwrap();
    assert!(matches!(
        load_snapshot(&path),
        Err(ExportError::Io(_)) | Err(ExportError::Checksum)
    ));
}

#[test]
fn csv_and_html_escape_hostile_names() {
    let sandbox = tempdir().unwrap();
    let snapshot = snapshot(WinName::from("=<script>alert(1)</script>"));
    let csv = sandbox.path().join("report.csv");
    let html = sandbox.path().join("report.html");
    export_report(&csv, &snapshot, ReportFormat::Csv).unwrap();
    export_report(&html, &snapshot, ReportFormat::Html).unwrap();
    let csv_text = std::fs::read_to_string(csv).unwrap();
    let html_text = std::fs::read_to_string(html).unwrap();
    assert!(csv_text.contains("'=<script>"));
    assert!(!html_text.contains("=<script>"));
    assert!(html_text.contains("=&lt;script&gt;"));
}
