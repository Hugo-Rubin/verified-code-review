//! Frames, and the only way to obtain one.

/// Bytes of fixed header every frame carries: two for the kind, two for the
/// declared payload length.
pub const HEADER_LEN: usize = 4;

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Fewer bytes than a header.
    Truncated,
}

/// A frame that has been through [`Frame::parse`].
///
/// `bytes` is private and `parse` is the only constructor in the crate, so
/// every `Frame` that exists has already been length-checked. Nothing mutates
/// `bytes` afterwards, so `bytes.len() >= HEADER_LEN` holds for the whole life
/// of the value: code that has a `Frame` in hand may read the header without
/// re-checking, and the header accessors do.
#[derive(Debug, Clone)]
pub struct Frame {
    bytes: Vec<u8>,
}

impl Frame {
    /// Take a frame off the wire. Anything shorter than a header is refused.
    pub fn parse(bytes: &[u8]) -> Result<Self, FrameError> {
        if bytes.len() < HEADER_LEN {
            return Err(FrameError::Truncated);
        }
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// The frame as it arrived.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Total size of the frame, header included.
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_shorter_than_a_header_is_refused() {
        for n in 0..HEADER_LEN {
            let short = vec![0u8; n];
            assert!(matches!(Frame::parse(&short), Err(FrameError::Truncated)));
        }
    }

    #[test]
    fn a_header_only_frame_is_accepted() {
        let f = Frame::parse(&[0, 1, 0, 0]).unwrap();
        assert_eq!(f.size(), HEADER_LEN);
    }

    #[test]
    fn parse_keeps_the_bytes_as_they_arrived() {
        let f = Frame::parse(&[0, 7, 0, 2, 9, 9]).unwrap();
        assert_eq!(f.bytes(), &[0, 7, 0, 2, 9, 9]);
    }
}
