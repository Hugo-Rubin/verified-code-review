//! Upload size reporting.

use crate::buffer::{Buffer, CHUNK_SIZE};

#[derive(Debug, PartialEq, Eq)]
pub struct SizeReport {
    pub label: String,
    pub chunks: usize,
    pub total_bytes: usize,
}

/// Total staged size of `buffer`, in bytes.
pub fn total_bytes(buffer: &Buffer) -> usize {
    buffer.chunk_count() * CHUNK_SIZE
}

/// Summarise a buffer's staged size.
pub fn size_report(buffer: &Buffer) -> SizeReport {
    SizeReport {
        label: buffer.label().to_string(),
        chunks: buffer.chunk_count(),
        total_bytes: total_bytes(buffer),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplies_chunks_by_the_chunk_size() {
        let b = Buffer::new(4, "upload").unwrap();
        assert_eq!(total_bytes(&b), 4 * 64 * 1024);
    }

    #[test]
    fn reports_label_and_counts() {
        let b = Buffer::new(2, "upload").unwrap();
        assert_eq!(
            size_report(&b),
            SizeReport {
                label: "upload".to_string(),
                chunks: 2,
                total_bytes: 131_072,
            }
        );
    }
}
