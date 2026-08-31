//! Length-prefixed framing for a small binary protocol.

pub mod frame;

pub use frame::{
    decode, encode, FrameError, HEADER_BYTES, MAX_FRAME_BYTES, MAX_PAYLOAD_BYTES,
};
