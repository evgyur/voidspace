//! Shared, platform-neutral types for Voidspace.

mod event;
mod identity;
mod name;
mod node;

pub use event::*;
pub use identity::*;
pub use name::*;
pub use node::*;

pub const MODEL_SCHEMA_VERSION: u16 = 1;
