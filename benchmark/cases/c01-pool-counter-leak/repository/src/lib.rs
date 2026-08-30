//! A minimal connection-pooling crate.

pub mod pool;

pub use pool::{Conn, Pool, PoolError};
