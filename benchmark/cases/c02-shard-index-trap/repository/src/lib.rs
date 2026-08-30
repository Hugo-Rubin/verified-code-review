//! A minimal sharded routing crate.

pub mod health;
pub mod router;
pub mod shard;

pub use health::{summary, Summary};
pub use router::{Router, RouterError};
pub use shard::Shard;
