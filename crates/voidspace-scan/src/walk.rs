use std::{
    collections::{VecDeque, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{Receiver, SendTimeoutError, Sender, bounded};
use thiserror::Error;
use voidspace_model::{
    BaselineFinished, DirectoryEnumerated, EventEnvelope, EventPayload, FileIdentity, NodeFlags,
    NodeKind, ProducerId, ScanId, SizeMetrics, SourceRevision, UpsertNode, VolumeId, WinName,
};

const PRODUCER: ProducerId = ProducerId(1);

#[derive(Clone, Debug)]
pub struct ScanRequest {
    pub scan_id: ScanId,
    pub generation: u64,
    pub root: PathBuf,
    pub follow_reparse_points: bool,
}

impl ScanRequest {
    pub fn new(scan_id: u64, generation: u64, root: PathBuf) -> Self {
        Self {
            scan_id: ScanId(scan_id),
            generation,
            root,
            follow_reparse_points: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RootDescriptor {
    pub path: PathBuf,
    pub identity: FileIdentity,
    pub name: WinName,
}

#[derive(Clone, Debug, Default)]
pub struct ScanStats {
    pub files: u64,
    pub directories: u64,
    pub logical_bytes: u64,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("cannot inspect scan root: {0}")]
    Root(#[source] std::io::Error),
    #[error("scan worker disconnected")]
    WorkerDisconnected,
    #[error("scan did not stop before timeout")]
    JoinTimeout,
    #[error("scan worker panicked")]
    WorkerPanicked,
}

struct Control {
    cancelled: AtomicBool,
    paused: Mutex<bool>,
    wake: Condvar,
}

pub struct ScanHandle {
    control: Arc<Control>,
    done: Receiver<ScanStats>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl ScanHandle {
    pub fn pause(&self) {
        *self.control.paused.lock().expect("pause mutex poisoned") = true;
    }

    pub fn resume(&self) {
        *self.control.paused.lock().expect("pause mutex poisoned") = false;
        self.control.wake.notify_all();
    }

    pub fn cancel(&self) {
        self.control.cancelled.store(true, Ordering::Release);
        self.control.wake.notify_all();
    }

    pub fn join(&self) -> Result<ScanStats, ScanError> {
        let stats = self
            .done
            .recv()
            .map_err(|_| ScanError::WorkerDisconnected)?;
        self.join_worker()?;
        Ok(stats)
    }

    pub fn join_timeout(&self, timeout: Duration) -> Result<ScanStats, ScanError> {
        let stats = self
            .done
            .recv_timeout(timeout)
            .map_err(|_| ScanError::JoinTimeout)?;
        self.join_worker()?;
        Ok(stats)
    }

    fn join_worker(&self) -> Result<(), ScanError> {
        if let Some(worker) = self.worker.lock().expect("worker mutex poisoned").take() {
            worker.join().map_err(|_| ScanError::WorkerPanicked)?;
        }
        Ok(())
    }
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn describe_root(path: &Path, generation: u64) -> Result<RootDescriptor, ScanError> {
    let metadata = fs::metadata(path).map_err(ScanError::Root)?;
    let identity = identity_for(path, &metadata, generation);
    let name = path
        .file_name()
        .map(win_name)
        .unwrap_or_else(|| WinName::from(path.to_string_lossy().into_owned()));
    Ok(RootDescriptor {
        path: path.to_path_buf(),
        identity,
        name,
    })
}

pub fn start(request: ScanRequest, sink: Sender<EventEnvelope>) -> Result<ScanHandle, ScanError> {
    let root = describe_root(&request.root, request.generation)?;
    let control = Arc::new(Control {
        cancelled: AtomicBool::new(false),
        paused: Mutex::new(false),
        wake: Condvar::new(),
    });
    let (done_tx, done_rx) = bounded(1);
    let worker_control = Arc::clone(&control);
    let worker = thread::Builder::new()
        .name(format!("voidspace-scan-{}", request.scan_id.0))
        .spawn(move || {
            let stats = run_scan(request, root, sink, &worker_control);
            let _ = done_tx.send(stats);
        })
        .map_err(ScanError::Root)?;
    Ok(ScanHandle {
        control,
        done: done_rx,
        worker: Mutex::new(Some(worker)),
    })
}

fn run_scan(
    request: ScanRequest,
    root: RootDescriptor,
    sink: Sender<EventEnvelope>,
    control: &Control,
) -> ScanStats {
    let mut stats = ScanStats::default();
    let mut sequence = 1_u64;
    if !emit(
        &sink,
        envelope(&request, sequence, EventPayload::BaselineStarted),
        control,
    ) {
        stats.cancelled = true;
        return stats;
    }
    sequence += 1;
    let mut pending = VecDeque::from([(root.path.clone(), root.identity.clone())]);

    while let Some((directory_path, directory_identity)) = pending.pop_front() {
        if !wait_if_paused(control) {
            stats.cancelled = true;
            break;
        }
        let read_dir = match fs::read_dir(&directory_path) {
            Ok(entries) => entries,
            Err(error) => {
                stats
                    .errors
                    .push(format!("{}: {error}", directory_path.display()));
                continue;
            }
        };
        let mut child_identities = Vec::new();
        for entry in read_dir {
            if control.cancelled.load(Ordering::Acquire) {
                stats.cancelled = true;
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    stats.errors.push(error.to_string());
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    stats.errors.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            let file_type = metadata.file_type();
            let is_reparse = file_type.is_symlink() || is_reparse_metadata(&metadata);
            let kind = if file_type.is_dir() {
                NodeKind::Directory
            } else {
                NodeKind::File
            };
            let identity = identity_for(&path, &metadata, request.generation);
            let logical = if kind == NodeKind::File {
                metadata.len()
            } else {
                0
            };
            let allocated = if logical == 0 {
                0
            } else {
                logical.saturating_add(4095) & !4095
            };
            let mut flags = flags_for(&metadata);
            flags.set(NodeFlags::REPARSE, is_reparse);
            let event = UpsertNode {
                parent: directory_identity.clone(),
                identity: identity.clone(),
                name: win_name(&entry.file_name()),
                kind,
                sizes: SizeMetrics::new(logical, allocated),
                flags,
                revision: SourceRevision::new(PRODUCER, request.generation, sequence),
            };
            if !emit(
                &sink,
                envelope(&request, sequence, EventPayload::UpsertNode(event)),
                control,
            ) {
                stats.cancelled = true;
                break;
            }
            sequence += 1;
            child_identities.push(identity.clone());
            if kind == NodeKind::Directory && (!is_reparse || request.follow_reparse_points) {
                stats.directories += 1;
                pending.push_back((path, identity));
            } else if kind == NodeKind::File {
                stats.files += 1;
                stats.logical_bytes = stats.logical_bytes.saturating_add(logical);
            }
        }
        child_identities.sort();
        if stats.cancelled {
            break;
        }
        let enumerated = DirectoryEnumerated {
            directory: directory_identity,
            enumeration_epoch: request.generation,
            sorted_child_identities: child_identities,
            fingerprint: [0; 32],
        };
        if !emit(
            &sink,
            envelope(
                &request,
                sequence,
                EventPayload::DirectoryEnumerated(enumerated),
            ),
            control,
        ) {
            stats.cancelled = true;
            break;
        }
        sequence += 1;
    }

    if !stats.cancelled {
        let _ = emit(
            &sink,
            envelope(
                &request,
                sequence,
                EventPayload::BaselineFinished(BaselineFinished {
                    captured_cursor: None,
                    root_fingerprint: [0; 32],
                }),
            ),
            control,
        );
    }
    stats
}

fn wait_if_paused(control: &Control) -> bool {
    let mut paused = control.paused.lock().expect("pause mutex poisoned");
    while *paused && !control.cancelled.load(Ordering::Acquire) {
        paused = control.wake.wait(paused).expect("pause mutex poisoned");
    }
    !control.cancelled.load(Ordering::Acquire)
}

fn emit(sink: &Sender<EventEnvelope>, mut event: EventEnvelope, control: &Control) -> bool {
    loop {
        if control.cancelled.load(Ordering::Acquire) {
            return false;
        }
        match sink.send_timeout(event, Duration::from_millis(25)) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(returned)) => event = returned,
            Err(SendTimeoutError::Disconnected(_)) => return false,
        }
    }
}

fn envelope(request: &ScanRequest, sequence: u64, payload: EventPayload) -> EventEnvelope {
    EventEnvelope {
        schema_version: 1,
        scan_id: request.scan_id,
        generation: request.generation,
        branch_epoch: None,
        producer: PRODUCER,
        sequence,
        observed_at_qpc: sequence,
        cause_operation: None,
        payload,
    }
}

fn identity_for(path: &Path, _metadata: &fs::Metadata, generation: u64) -> FileIdentity {
    let mut hasher = DefaultHasher::new();
    hash_path(path, &mut hasher);
    FileIdentity::stable(
        VolumeId::Session(generation as u128),
        u128::from(hasher.finish()),
        generation,
    )
}

#[cfg(windows)]
fn hash_path(path: &Path, hasher: &mut DefaultHasher) {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .for_each(|unit| unit.hash(hasher));
}

#[cfg(not(windows))]
fn hash_path(path: &Path, hasher: &mut DefaultHasher) {
    path.as_os_str().to_string_lossy().hash(hasher);
}

fn win_name(value: &std::ffi::OsStr) -> WinName {
    #[cfg(windows)]
    {
        WinName::from_os_str(value).expect("filesystem returned invalid Win32 name")
    }
    #[cfg(not(windows))]
    {
        WinName::from(value.to_string_lossy().into_owned())
    }
}

#[cfg(windows)]
fn flags_for(metadata: &fs::Metadata) -> NodeFlags {
    use std::os::windows::fs::MetadataExt;
    const READONLY: u32 = 0x1;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    const SPARSE: u32 = 0x200;
    const COMPRESSED: u32 = 0x800;
    let attributes = metadata.file_attributes();
    let mut flags = NodeFlags::empty();
    flags.set(NodeFlags::READONLY, attributes & READONLY != 0);
    flags.set(NodeFlags::HIDDEN, attributes & HIDDEN != 0);
    flags.set(NodeFlags::SYSTEM, attributes & SYSTEM != 0);
    flags.set(NodeFlags::SPARSE, attributes & SPARSE != 0);
    flags.set(NodeFlags::COMPRESSED, attributes & COMPRESSED != 0);
    flags
}

#[cfg(not(windows))]
fn flags_for(_metadata: &fs::Metadata) -> NodeFlags {
    NodeFlags::empty()
}

#[cfg(windows)]
fn is_reparse_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_metadata(_metadata: &fs::Metadata) -> bool {
    false
}
