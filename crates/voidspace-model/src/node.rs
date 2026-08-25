use bitflags::bitflags;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    File,
    Directory,
    Stream,
    FreeSpace,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SizeMetrics {
    pub logical: u64,
    pub allocated: u64,
}

impl SizeMetrics {
    pub const fn new(logical: u64, allocated: u64) -> Self {
        Self { logical, allocated }
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct NodeFlags: u32 {
        const RESTRICTED = 1 << 0;
        const REPARSE = 1 << 1;
        const SHARED_ALLOCATION = 1 << 2;
        const TOMBSTONE = 1 << 3;
        const SPARSE = 1 << 4;
        const COMPRESSED = 1 << 5;
    }
}
