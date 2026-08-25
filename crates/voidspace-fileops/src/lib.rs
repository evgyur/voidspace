//! Fail-closed local filesystem operations for Voidspace.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::UNIX_EPOCH,
};

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OperationKind {
    Recycle,
    Permanent,
}

#[derive(Clone, Debug)]
pub struct OperationDraft {
    pub kind: OperationKind,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: PathBuf,
    pub is_directory: bool,
    pub length: u64,
    pub modified_ticks: u128,
    pub readonly: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfirmableOperation {
    pub kind: OperationKind,
    pub roots: Vec<PathBuf>,
    pub manifest: Vec<ManifestEntry>,
    pub manifest_hash: [u8; 32],
    pub total_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct ConfirmedOperation(ConfirmableOperation);

impl ConfirmedOperation {
    pub fn details(&self) -> &ConfirmableOperation {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Progress {
    Started { items: usize },
    Item { path: PathBuf },
    Finished,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OperationReport {
    pub deleted: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Error)]
pub enum FileOpError {
    #[error("no paths were selected")]
    EmptySelection,
    #[error("volume or filesystem root cannot be modified: {0}")]
    RootRejected(PathBuf),
    #[error("reparse points and symbolic links are not accepted: {0}")]
    ReparseRejected(PathBuf),
    #[error("path is missing or inaccessible: {path}: {source}")]
    Inspect {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("permanent deletion requires the exact phrase DELETE")]
    ConfirmationRejected,
    #[error("the filesystem changed after confirmation; prepare the operation again")]
    ManifestChanged,
    #[error("Recycle Bin operation failed: {0}")]
    Recycle(String),
    #[error("Explorer launch failed: {0}")]
    Explorer(#[source] std::io::Error),
}

pub fn prepare(draft: OperationDraft) -> Result<ConfirmableOperation, FileOpError> {
    if draft.paths.is_empty() {
        return Err(FileOpError::EmptySelection);
    }
    let roots = normalize_roots(&draft.paths)?;
    let manifest = build_manifest(&roots)?;
    let total_bytes = manifest.iter().map(|entry| entry.length).sum();
    let manifest_hash = hash_manifest(&manifest);
    Ok(ConfirmableOperation {
        kind: draft.kind,
        roots,
        manifest,
        manifest_hash,
        total_bytes,
    })
}

pub fn confirm(
    operation: ConfirmableOperation,
    confirmation_phrase: &str,
) -> Result<ConfirmedOperation, FileOpError> {
    if operation.kind == OperationKind::Permanent && confirmation_phrase != "DELETE" {
        return Err(FileOpError::ConfirmationRejected);
    }
    Ok(ConfirmedOperation(operation))
}

pub fn execute(
    operation: ConfirmedOperation,
    progress: Option<&Sender<Progress>>,
    cancellation: &CancellationToken,
) -> Result<OperationReport, FileOpError> {
    let operation = operation.0;
    let current = build_manifest(&operation.roots)?;
    if current != operation.manifest || hash_manifest(&current) != operation.manifest_hash {
        return Err(FileOpError::ManifestChanged);
    }

    let _ = progress.map(|sink| {
        sink.send(Progress::Started {
            items: operation.roots.len(),
        })
    });
    let mut report = OperationReport::default();
    for root in &operation.roots {
        if cancellation.is_cancelled() {
            report.cancelled = true;
            break;
        }
        let _ = progress.map(|sink| sink.send(Progress::Item { path: root.clone() }));
        let result = match operation.kind {
            OperationKind::Recycle => trash::delete(root).map_err(|error| error.to_string()),
            OperationKind::Permanent => delete_permanently(root).map_err(|error| error.to_string()),
        };
        match result {
            Ok(()) => report.deleted.push(root.clone()),
            Err(error) => report.failed.push((root.clone(), error)),
        }
    }
    let _ = progress.map(|sink| sink.send(Progress::Finished));
    Ok(report)
}

pub fn reveal_in_explorer(path: &Path) -> Result<(), FileOpError> {
    Command::new("explorer.exe")
        .arg("/select,")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(FileOpError::Explorer)
}

fn normalize_roots(paths: &[PathBuf]) -> Result<Vec<PathBuf>, FileOpError> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for raw in paths {
        reject_root(raw)?;
        reject_reparse(raw)?;
        let path = fs::canonicalize(raw).map_err(|source| FileOpError::Inspect {
            path: raw.clone(),
            source,
        })?;
        reject_root(&path)?;
        if seen.insert(path.clone()) {
            roots.push(path);
        }
    }
    roots.sort();
    let mut deduplicated = Vec::<PathBuf>::new();
    for candidate in roots {
        if !deduplicated
            .iter()
            .any(|parent| candidate.starts_with(parent))
        {
            deduplicated.push(candidate);
        }
    }
    Ok(deduplicated)
}

fn reject_root(path: &Path) -> Result<(), FileOpError> {
    if path.parent().is_none()
        || path
            .parent()
            .is_some_and(|parent| parent.as_os_str().is_empty())
    {
        return Err(FileOpError::RootRejected(path.to_path_buf()));
    }
    Ok(())
}

fn reject_reparse(path: &Path) -> Result<(), FileOpError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| FileOpError::Inspect {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || is_windows_reparse(&metadata) {
        return Err(FileOpError::ReparseRejected(path.to_path_buf()));
    }
    Ok(())
}

fn build_manifest(roots: &[PathBuf]) -> Result<Vec<ManifestEntry>, FileOpError> {
    let mut entries = Vec::new();
    for root in roots {
        collect_manifest(root, &mut entries)?;
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn collect_manifest(path: &Path, entries: &mut Vec<ManifestEntry>) -> Result<(), FileOpError> {
    reject_reparse(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| FileOpError::Inspect {
        path: path.to_path_buf(),
        source,
    })?;
    let is_directory = metadata.is_dir();
    let modified_ticks = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    entries.push(ManifestEntry {
        path: path.to_path_buf(),
        is_directory,
        length: metadata.len(),
        modified_ticks,
        readonly: metadata.permissions().readonly(),
    });
    if is_directory {
        let children = fs::read_dir(path).map_err(|source| FileOpError::Inspect {
            path: path.to_path_buf(),
            source,
        })?;
        for child in children {
            let child = child.map_err(|source| FileOpError::Inspect {
                path: path.to_path_buf(),
                source,
            })?;
            collect_manifest(&child.path(), entries)?;
        }
    }
    Ok(())
}

fn hash_manifest(manifest: &[ManifestEntry]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for entry in manifest {
        hasher.update(entry.path.to_string_lossy().as_bytes());
        hasher.update(&[entry.is_directory as u8]);
        hasher.update(&entry.length.to_le_bytes());
        hasher.update(&entry.modified_ticks.to_le_bytes());
        hasher.update(&[entry.readonly as u8]);
    }
    *hasher.finalize().as_bytes()
}

fn delete_permanently(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        if metadata.permissions().readonly() {
            clear_readonly(path, &metadata)?;
        }
        fs::remove_file(path)
    }
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly(path: &Path, metadata: &fs::Metadata) -> std::io::Result<()> {
    let mut permissions = metadata.permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions)
}

#[cfg(not(windows))]
fn clear_readonly(_path: &Path, _metadata: &fs::Metadata) -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn is_windows_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_windows_reparse(_metadata: &fs::Metadata) -> bool {
    false
}
