//! A minimal sharded routing crate.

pub mod router;
pub mod shard;

pub use router::{Router, RouterError};
pub use shard::Shard;
