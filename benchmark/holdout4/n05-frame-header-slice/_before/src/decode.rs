//! Reading the fields of a frame.

use crate::frame::{Frame, HEADER_LEN};

/// The frame's kind tag.
pub fn kind(frame: &Frame) -> Option<u16> {
    let bytes = frame.bytes();
    if bytes.len() < HEADER_LEN {
        return None;
    }
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// The payload length the sender declared in the header.
pub fn declared_len(frame: &Frame) -> Option<u16> {
    let bytes = frame.bytes();
    if bytes.len() < HEADER_LEN {
        return None;
    }
    Some(u16::from_be_bytes([bytes[2], bytes[3]]))
}

/// The bytes after the header.
pub fn payload(frame: &Frame) -> &[u8] {
    let bytes = frame.bytes();
    if bytes.len() < HEADER_LEN {
        return &[];
    }
    &bytes[HEADER_LEN..]
}

/// Whether the frame carries as much payload as its header declares.
pub fn is_complete(frame: &Frame) -> bool {
    match declared_len(frame) {
        Some(declared) => payload(frame).len() >= declared as usize,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(bytes: &[u8]) -> Frame {
        Frame::parse(bytes).expect("test frames are well formed")
    }

    #[test]
    fn the_header_fields_are_read_big_endian() {
        let f = frame(&[0x01, 0x02, 0x00, 0x03, 7, 8, 9]);
        assert_eq!(kind(&f), Some(0x0102));
        assert_eq!(declared_len(&f), Some(3));
    }

    #[test]
    fn the_payload_is_everything_after_the_header() {
        let f = frame(&[0, 1, 0, 2, 42, 43]);
        assert_eq!(payload(&f), &[42, 43]);
    }

    #[test]
    fn a_header_only_frame_has_an_empty_payload() {
        let f = frame(&[0, 1, 0, 0]);
        assert!(payload(&f).is_empty());
        assert!(is_complete(&f));
    }

    #[test]
    fn a_frame_missing_payload_bytes_is_incomplete() {
        let f = frame(&[0, 1, 0, 4, 1, 2]);
        assert!(!is_complete(&f));
    }
}
