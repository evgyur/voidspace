//! Arena-backed filesystem index and reducer.

mod arena;
mod reducer;
mod snapshot;

pub use arena::*;
pub use reducer::*;
pub use snapshot::*;

pub const INDEX_SCHEMA_VERSION: u16 = 1;
