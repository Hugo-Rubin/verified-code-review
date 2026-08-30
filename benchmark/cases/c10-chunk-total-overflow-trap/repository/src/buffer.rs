//! Chunked upload buffers.

/// Bytes per chunk. Fixed by the wire protocol.
pub const CHUNK_SIZE: usize = 64 * 1024;

/// Largest number of chunks a single upload may declare.
pub const MAX_CHUNKS: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub enum BufferError {
    /// The declared chunk count exceeds `MAX_CHUNKS`.
    TooManyChunks,
    /// An upload must declare at least one chunk.
    NoChunks,
}

/// A staged upload.
///
/// # Invariant
///
/// `chunk_count` is always in `1..=MAX_CHUNKS`. [`Buffer::new`] is the only
/// constructor and rejects anything else, and no method changes the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    chunk_count: usize,
    label: String,
}

impl Buffer {
    pub fn new(chunk_count: usize, label: &str) -> Result<Self, BufferError> {
        if chunk_count == 0 {
            return Err(BufferError::NoChunks);
        }
        if chunk_count > MAX_CHUNKS {
            return Err(BufferError::TooManyChunks);
        }
        Ok(Self {
            chunk_count,
            label: label.to_string(),
        })
    }

    pub fn chunk_count(&self) -> usize {
        self.chunk_count
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Rename the buffer. The chunk count is unaffected.
    pub fn relabel(&mut self, label: &str) {
        self.label = label.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_chunks() {
        assert_eq!(Buffer::new(0, "a"), Err(BufferError::NoChunks));
    }

    #[test]
    fn rejects_more_than_the_maximum() {
        assert_eq!(
            Buffer::new(MAX_CHUNKS + 1, "a"),
            Err(BufferError::TooManyChunks)
        );
    }

    #[test]
    fn accepts_the_maximum() {
        assert_eq!(Buffer::new(MAX_CHUNKS, "a").unwrap().chunk_count(), MAX_CHUNKS);
    }

    #[test]
    fn relabel_leaves_the_chunk_count_alone() {
        let mut b = Buffer::new(8, "a").unwrap();
        b.relabel("b");
        assert_eq!(b.chunk_count(), 8);
        assert_eq!(b.label(), "b");
    }
}

