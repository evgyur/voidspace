use serde::{Deserialize, Serialize};

use crate::WinName;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ScanId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ProducerId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct OperationId(pub u128);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum FileSystemKind {
    Ntfs,
    Refs,
    Exfat,
    Fat,
    Other(String),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum VolumeId {
    Local {
        volume_guid: WinName,
        serial: u64,
        file_system: FileSystemKind,
    },
    Unc {
        share: WinName,
        serial: Option<u64>,
        session: u128,
    },
    Session(u128),
}

impl VolumeId {
    pub fn local_for_test(serial: u64) -> Self {
        Self::Local {
            volume_guid: WinName::from("test-volume"),
            serial,
            file_system: FileSystemKind::Ntfs,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FileIdentity {
    pub volume: VolumeId,
    pub file_id: u128,
    pub generation: u64,
}

impl FileIdentity {
    pub fn stable(volume: VolumeId, file_id: u128, generation: u64) -> Self {
        Self {
            volume,
            file_id,
            generation,
        }
    }
}
