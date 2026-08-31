//! Encoding and decoding of one frame.
//!
//! A frame is `HEADER_BYTES` of header followed by the payload. The header
//! carries the payload length as a big-endian `u32`, then four reserved bytes.

/// Size of the fixed header that precedes every payload.
pub const HEADER_BYTES: usize = 8;

/// The transport refuses to carry a frame larger than this, in either
/// direction.
pub const MAX_FRAME_BYTES: usize = 4096;

/// The largest payload that still fits inside `MAX_FRAME_BYTES` once the
/// header is accounted for.
pub const MAX_PAYLOAD_BYTES: usize = MAX_FRAME_BYTES - HEADER_BYTES;

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The caller offered more payload than a frame can carry.
    PayloadTooLarge { len: usize, limit: usize },
    /// The peer sent a frame the transport will not carry.
    FrameTooLarge { len: usize },
    /// The frame ended before the header or the declared payload did.
    Truncated,
}

/// Encode `payload` into a single frame.
pub fn encode(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(FrameError::PayloadTooLarge {
            len: payload.len(),
            limit: MAX_PAYLOAD_BYTES,
        });
    }

    let mut out = Vec::with_capacity(HEADER_BYTES + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(payload);
    Ok(out)
}

/// Decode a frame back into its payload.
pub fn decode(frame: &[u8]) -> Result<Vec<u8>, FrameError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::FrameTooLarge { len: frame.len() });
    }
    if frame.len() < HEADER_BYTES {
        return Err(FrameError::Truncated);
    }

    let len = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    let body = frame
        .get(HEADER_BYTES..HEADER_BYTES + len)
        .ok_or(FrameError::Truncated)?;
    Ok(body.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_small_payload() {
        let frame = encode(b"hello").unwrap();
        assert_eq!(frame.len(), HEADER_BYTES + 5);
        assert_eq!(decode(&frame), Ok(b"hello".to_vec()));
    }

    #[test]
    fn round_trips_an_empty_payload() {
        let frame = encode(b"").unwrap();
        assert_eq!(decode(&frame), Ok(Vec::new()));
    }

    #[test]
    fn refuses_an_oversized_payload() {
        let huge = vec![0u8; 10_000];
        assert!(matches!(
            encode(&huge),
            Err(FrameError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn decode_refuses_an_oversized_frame() {
        let huge = vec![0u8; 5_000];
        assert!(matches!(
            decode(&huge),
            Err(FrameError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn decode_refuses_a_truncated_frame() {
        assert_eq!(decode(&[0, 0, 0]), Err(FrameError::Truncated));
        let mut frame = encode(b"hello").unwrap();
        frame.truncate(HEADER_BYTES + 2);
        assert_eq!(decode(&frame), Err(FrameError::Truncated));
    }
}
