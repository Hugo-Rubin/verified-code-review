//! Chunked upload staging.

pub mod buffer;

pub use buffer::{Buffer, BufferError, CHUNK_SIZE, MAX_CHUNKS};
