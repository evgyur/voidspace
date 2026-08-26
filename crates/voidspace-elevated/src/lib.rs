//! Typed, bounded protocol and UAC launcher for the Voidspace helper.

use std::{
    collections::HashSet,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RequestId(pub u64);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerClaim {
    pub pid: u32,
    pub executable: PathBuf,
    pub session_id: u32,
    pub nonce: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RequestKind {
    Probe,
    TurboStart { root: PathBuf },
    PrivilegedScan { root: PathBuf },
    PermanentDelete { paths: Vec<PathBuf>, phrase: String },
    Cancel { target: RequestId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub version: u16,
    pub id: RequestId,
    pub sequence: u64,
    pub peer: PeerClaim,
    pub kind: RequestKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TurboMode {
    UsnJournal,
    PrivilegedTraversalFallback { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Ready { elevated: bool },
    TurboAccepted { mode: TurboMode },
    Accepted,
    Rejected { reason: String },
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("frame exceeds the {MAX_FRAME_BYTES} byte protocol limit")]
    OversizedFrame,
    #[error("truncated protocol frame")]
    Truncated,
    #[error("malformed JSON payload: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error("unsupported protocol version {0}")]
    Version(u16),
    #[error("duplicate request id")]
    DuplicateRequest,
    #[error("request sequence must be strictly monotonic")]
    NonMonotonicSequence,
    #[error("peer identity does not match the launched client")]
    WrongPeer,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn write_frame(mut writer: impl Write, value: &impl Serialize) -> Result<(), ProtocolError> {
    let payload = serde_json::to_vec(value)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::OversizedFrame);
    }
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame<T: for<'de> Deserialize<'de>>(mut reader: impl Read) -> Result<T, ProtocolError> {
    let mut header = [0_u8; 4];
    reader
        .read_exact(&mut header)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => ProtocolError::Truncated,
            _ => ProtocolError::Io(error),
        })?;
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(ProtocolError::OversizedFrame);
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => ProtocolError::Truncated,
            _ => ProtocolError::Io(error),
        })?;
    Ok(serde_json::from_slice(&payload)?)
}

pub struct ProtocolGuard {
    expected_peer: PeerClaim,
    seen: HashSet<RequestId>,
    last_sequence: u64,
}

impl ProtocolGuard {
    pub fn new(expected_peer: PeerClaim) -> Self {
        Self {
            expected_peer,
            seen: HashSet::new(),
            last_sequence: 0,
        }
    }

    pub fn accept(&mut self, request: &Request) -> Result<(), ProtocolError> {
        if request.version != PROTOCOL_VERSION {
            return Err(ProtocolError::Version(request.version));
        }
        if request.peer != self.expected_peer {
            return Err(ProtocolError::WrongPeer);
        }
        if self.seen.contains(&request.id) {
            return Err(ProtocolError::DuplicateRequest);
        }
        if request.sequence <= self.last_sequence {
            return Err(ProtocolError::NonMonotonicSequence);
        }
        self.last_sequence = request.sequence;
        self.seen.insert(request.id);
        Ok(())
    }
}

pub fn turbo_mode_for(root: &Path) -> TurboMode {
    TurboMode::PrivilegedTraversalFallback {
        reason: format!(
            "USN acceleration is unavailable for {}; using fail-safe privileged traversal",
            root.display()
        ),
    }
}

fn quote_windows_argument(argument: &str) -> String {
    let mut quoted = String::with_capacity(argument.len() + 2);
    quoted.push('"');
    let mut backslashes = 0_usize;

    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }

        if character == '"' {
            quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
        } else {
            quoted.extend(std::iter::repeat_n('\\', backslashes));
        }
        backslashes = 0;
        quoted.push(character);
    }

    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

pub fn windows_command_line(arguments: &[&str]) -> String {
    arguments
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
pub fn launch_elevated(executable: &Path, arguments: &[&str]) -> Result<(), std::io::Error> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::PCWSTR,
    };

    let wide = |value: &std::ffi::OsStr| {
        value
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let verb = wide(std::ffi::OsStr::new("runas"));
    let file = wide(executable.as_os_str());
    let command_line = windows_command_line(arguments);
    let args = wide(std::ffi::OsStr::new(&command_line));
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(args.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn launch_elevated(_executable: &Path, _arguments: &[&str]) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "UAC is Windows-only",
    ))
}
