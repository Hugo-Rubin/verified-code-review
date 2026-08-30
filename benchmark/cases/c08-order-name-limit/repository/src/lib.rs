//! Order intake validation.

pub mod order;

pub use order::{validate, Order, ValidationError, MAX_NAME_LEN, MAX_QUANTITY};
