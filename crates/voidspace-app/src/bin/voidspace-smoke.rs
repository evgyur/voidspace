use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use crossbeam_channel::bounded;
use voidspace_export::{ReportFormat, export_report, save_snapshot};
use voidspace_fileops::{
    CancellationToken, OperationDraft, OperationKind, confirm, execute, prepare,
};
use voidspace_index::Index;
use voidspace_model::EventPayload;
use voidspace_scan::{ScanRequest, describe_root, start};

fn main() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.get(1).is_some_and(|value| value == "--delete") {
        let allowed_root = PathBuf::from(arguments.get(2).context("missing allowed root")?);
        let target = PathBuf::from(arguments.get(3).context("missing delete target")?);
        return delete_smoke(&allowed_root, &target);
    }
    let root = PathBuf::from(arguments.get(1).context("missing scan root")?);
    let output = PathBuf::from(arguments.get(2).context("missing output directory")?);
    std::fs::create_dir_all(&output)?;
    let descriptor = describe_root(&root, 1)?;
    let mut index = Index::new(
        voidspace_model::ScanId(1),
        1,
        descriptor.identity,
        descriptor.name,
    );
    let (sender, receiver) = bounded(65_536);
    let handle = start(ScanRequest::new(1, 1, root.clone()), sender)?;
    loop {
        let event = receiver.recv().context("scanner disconnected")?;
        let finished = matches!(event.payload, EventPayload::BaselineFinished(_));
        index.apply(event)?;
        if finished {
            break;
        }
    }
    let stats = handle.join()?;
    let snapshot = index.snapshot();
    save_snapshot(&output.join("scan.voidspace"), &snapshot)?;
    for (name, format) in [
        ("report.csv", ReportFormat::Csv),
        ("report.json", ReportFormat::Json),
        ("report.html", ReportFormat::Html),
        ("report.txt", ReportFormat::Text),
    ] {
        export_report(&output.join(name), &snapshot, format)?;
    }
    let allocated = snapshot
        .node(snapshot.root)
        .map_or(0, |node| node.allocated);
    println!(
        "{}",
        serde_json::json!({
            "root": root,
            "files": stats.files,
            "directories": stats.directories,
            "allocated": allocated,
            "errors": stats.errors.len(),
        })
    );
    Ok(())
}

fn delete_smoke(allowed_root: &Path, target: &Path) -> anyhow::Result<()> {
    let allowed = std::fs::canonicalize(allowed_root)?;
    let target = std::fs::canonicalize(target)?;
    if target == allowed || !target.starts_with(&allowed) {
        bail!("delete target escaped the smoke root");
    }
    let prepared = prepare(OperationDraft {
        kind: OperationKind::Permanent,
        paths: vec![target],
    })?;
    let confirmed = confirm(prepared, "DELETE")?;
    let report = execute(confirmed, None, &CancellationToken::default())?;
    if !report.failed.is_empty() || report.deleted.len() != 1 {
        bail!("smoke deletion did not complete exactly once");
    }
    println!("{{\"deleted\":1}}");
    Ok(())
}
