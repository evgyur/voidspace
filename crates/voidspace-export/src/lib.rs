//! Versioned Voidspace snapshots and injection-safe reports.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use voidspace_index::{IndexSnapshot, NodeSnapshot};
use voidspace_model::{NodeId, NodeKind};

const MAGIC: &[u8; 8] = b"VOIDSPC\0";
const SNAPSHOT_VERSION: u16 = 1;
const MAX_SNAPSHOT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DECODED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_NODES: usize = 10_000_000;
const MAX_DEPTH: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportFormat {
    Csv,
    Json,
    Html,
    Text,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("snapshot is too large")]
    TooLarge,
    #[error("not a Voidspace snapshot")]
    BadMagic,
    #[error("unsupported snapshot version {0}")]
    UnsupportedVersion(u16),
    #[error("snapshot checksum mismatch")]
    Checksum,
    #[error("malformed snapshot: {0}")]
    Malformed(String),
    #[error("CSV encoding failed: {0}")]
    Csv(#[from] csv::Error),
    #[error("JSON encoding failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Serialize)]
struct ReportRow {
    path: String,
    name: String,
    kind: &'static str,
    allocated: u64,
    logical: u64,
}

pub fn save_snapshot(path: &Path, snapshot: &IndexSnapshot) -> Result<(), ExportError> {
    validate_snapshot(snapshot)?;
    let mut raw = Vec::new();
    ciborium::into_writer(snapshot, &mut raw)
        .map_err(|error| ExportError::Malformed(error.to_string()))?;
    if raw.len() as u64 > MAX_DECODED_BYTES {
        return Err(ExportError::TooLarge);
    }
    let checksum = blake3::hash(&raw);
    let compressed = zstd::stream::encode_all(raw.as_slice(), 6)?;
    if compressed.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(ExportError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(50 + compressed.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&SNAPSHOT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    bytes.extend_from_slice(checksum.as_bytes());
    bytes.extend_from_slice(&compressed);
    write_atomic(path, &bytes)
}

pub fn load_snapshot(path: &Path) -> Result<IndexSnapshot, ExportError> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_SNAPSHOT_BYTES + 50 {
        return Err(ExportError::TooLarge);
    }
    let mut header = [0_u8; 50];
    file.read_exact(&mut header)?;
    if &header[..8] != MAGIC {
        return Err(ExportError::BadMagic);
    }
    let version = u16::from_le_bytes([header[8], header[9]]);
    if version != SNAPSHOT_VERSION {
        return Err(ExportError::UnsupportedVersion(version));
    }
    let decoded_length = u64::from_le_bytes(header[10..18].try_into().expect("fixed header"));
    if decoded_length > MAX_DECODED_BYTES {
        return Err(ExportError::TooLarge);
    }
    let expected_checksum: [u8; 32] = header[18..50].try_into().expect("fixed header");
    let decoder = zstd::stream::read::Decoder::new(file)?;
    let mut raw = Vec::with_capacity(decoded_length.min(64 * 1024 * 1024) as usize);
    decoder.take(MAX_DECODED_BYTES + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 != decoded_length || raw.len() as u64 > MAX_DECODED_BYTES {
        return Err(ExportError::TooLarge);
    }
    if blake3::hash(&raw).as_bytes() != &expected_checksum {
        return Err(ExportError::Checksum);
    }
    let snapshot: IndexSnapshot = ciborium::from_reader(raw.as_slice())
        .map_err(|error| ExportError::Malformed(error.to_string()))?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub fn export_report(
    path: &Path,
    snapshot: &IndexSnapshot,
    format: ReportFormat,
) -> Result<(), ExportError> {
    validate_snapshot(snapshot)?;
    let rows = report_rows(snapshot);
    let bytes = match format {
        ReportFormat::Csv => csv_report(&rows)?,
        ReportFormat::Json => serde_json::to_vec_pretty(&rows)?,
        ReportFormat::Html => html_report(&rows).into_bytes(),
        ReportFormat::Text => text_report(&rows).into_bytes(),
    };
    write_atomic(path, &bytes)
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), ExportError> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ExportError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    let (_, temporary_path) = temp.keep().map_err(|error| error.error)?;
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(ExportError::Io(error));
    }
    Ok(())
}

fn validate_snapshot(snapshot: &IndexSnapshot) -> Result<(), ExportError> {
    if snapshot.nodes.len() > MAX_NODES {
        return Err(ExportError::TooLarge);
    }
    let ids: HashSet<_> = snapshot.nodes.iter().map(|node| node.id).collect();
    if ids.len() != snapshot.nodes.len() || !ids.contains(&snapshot.root) {
        return Err(ExportError::Malformed(
            "duplicate node id or missing root".into(),
        ));
    }
    for node in snapshot.nodes.iter() {
        if node.parent.is_some_and(|parent| !ids.contains(&parent))
            || node.children.iter().any(|child| !ids.contains(child))
        {
            return Err(ExportError::Malformed("dangling node reference".into()));
        }
        let mut current = node.parent;
        let mut seen = HashSet::new();
        for _ in 0..MAX_DEPTH {
            let Some(parent) = current else {
                break;
            };
            if !seen.insert(parent) {
                return Err(ExportError::Malformed("node cycle".into()));
            }
            current = snapshot.node(parent).and_then(|entry| entry.parent);
        }
        if current.is_some() {
            return Err(ExportError::Malformed("tree depth limit exceeded".into()));
        }
    }
    Ok(())
}

fn report_rows(snapshot: &IndexSnapshot) -> Vec<ReportRow> {
    let paths = build_paths(snapshot);
    snapshot
        .nodes
        .iter()
        .map(|node| ReportRow {
            path: paths.get(&node.id).cloned().unwrap_or_default(),
            name: node.name.display_escaped(),
            kind: kind_name(node.kind),
            allocated: node.allocated,
            logical: node.logical,
        })
        .collect()
}

fn build_paths(snapshot: &IndexSnapshot) -> HashMap<NodeId, String> {
    fn resolve(
        id: NodeId,
        nodes: &HashMap<NodeId, &NodeSnapshot>,
        paths: &mut HashMap<NodeId, String>,
    ) -> String {
        if let Some(path) = paths.get(&id) {
            return path.clone();
        }
        let Some(node) = nodes.get(&id) else {
            return String::new();
        };
        let name = node.name.display_escaped();
        let path = node.parent.map_or(name.clone(), |parent| {
            let prefix = resolve(parent, nodes, paths);
            format!("{prefix}/{name}")
        });
        paths.insert(id, path.clone());
        path
    }

    let nodes: HashMap<_, _> = snapshot.nodes.iter().map(|node| (node.id, node)).collect();
    let mut paths = HashMap::new();
    for node in snapshot.nodes.iter() {
        resolve(node.id, &nodes, &mut paths);
    }
    paths
}

fn csv_report(rows: &[ReportRow]) -> Result<Vec<u8>, ExportError> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["path", "name", "kind", "allocated", "logical"])?;
    for row in rows {
        writer.write_record([
            spreadsheet_safe(&row.path),
            spreadsheet_safe(&row.name),
            row.kind.into(),
            row.allocated.to_string(),
            row.logical.to_string(),
        ])?;
    }
    writer
        .into_inner()
        .map_err(|error| ExportError::Io(error.into_error()))
}

fn html_report(rows: &[ReportRow]) -> String {
    let mut output = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Voidspace report</title><style>body{background:#070809;color:#f4f4f5;font:14px Segoe UI,sans-serif;padding:24px}table{border-collapse:collapse;width:100%}th,td{border-bottom:1px solid #27272a;padding:8px;text-align:left}th{color:#ff5a2f}td:nth-last-child(-n+2){font-family:monospace}</style><h1>Voidspace report</h1><table><thead><tr><th>Path</th><th>Type</th><th>Allocated</th><th>Logical</th></tr></thead><tbody>",
    );
    for row in rows {
        output.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            html_escape(&row.path),
            html_escape(row.kind),
            row.allocated,
            row.logical
        ));
    }
    output.push_str("</tbody></table>");
    output
}

fn text_report(rows: &[ReportRow]) -> String {
    let mut output = String::from("ALLOCATED\tLOGICAL\tTYPE\tPATH\n");
    for row in rows {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            row.allocated,
            row.logical,
            row.kind,
            row.path.replace('\r', "\\r").replace('\n', "\\n")
        ));
    }
    output
}

fn spreadsheet_safe(value: &str) -> String {
    if value
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r' | '\n'))
    {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::File => "file",
        NodeKind::Directory => "directory",
        NodeKind::Stream => "stream",
        NodeKind::FreeSpace => "free-space",
        NodeKind::Unknown => "unknown",
    }
}
