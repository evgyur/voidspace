use std::path::{Path, PathBuf};
use std::time::Instant;

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
    if arguments
        .get(1)
        .is_some_and(|value| value == "--hud-benchmark")
    {
        return hud_benchmark();
    }
    if arguments
        .get(1)
        .is_some_and(|value| value == "--typography")
    {
        println!(
            "{}",
            voidspace_app::release_typography_diagnostic().map_err(anyhow::Error::msg)?
        );
        return Ok(());
    }
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

fn hud_benchmark() -> anyhow::Result<()> {
    const WARM_UP: usize = 60;
    const MEASURED: usize = 600;
    let context = egui::Context::default();
    let render = || {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1920.0, 1080.0),
            )),
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| {
            let painter = ui.painter();
            let origin = ui.max_rect().min;
            for index in 0..1024 {
                let column = index % 32;
                let row = index / 32;
                let rect = egui::Rect::from_min_size(
                    origin + egui::vec2(column as f32 * 58.0, row as f32 * 32.0),
                    egui::vec2(56.0, 30.0),
                );
                voidspace_app::hud::paint_cut_frame(
                    painter,
                    rect,
                    voidspace_app::hud::PANEL,
                    egui::Stroke::new(1.0, voidspace_app::hud::HAIRLINE),
                    6.0,
                );
            }
        });
    };
    for _ in 0..WARM_UP {
        render();
    }
    let mut samples = Vec::with_capacity(MEASURED);
    for _ in 0..MEASURED {
        let started = Instant::now();
        render();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    let median_ms = samples[samples.len() / 2];
    let p95_ms = samples[((samples.len() - 1) as f64 * 0.95).round() as usize];
    println!(
        "{}",
        serde_json::json!({
            "fixture_tiles": 1024,
            "warm_up_frames": WARM_UP,
            "measured_frames": MEASURED,
            "median_ms": median_ms,
            "p95_ms": p95_ms,
            "idle_autonomous_repaint": voidspace_app::hud::hud_requires_autonomous_repaint(
                voidspace_app::hud::HudMotionState::Idle
            ),
        })
    );
    if p95_ms >= 16.7 {
        bail!("HUD render p95 {p95_ms:.3} ms exceeded 16.7 ms");
    }
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
