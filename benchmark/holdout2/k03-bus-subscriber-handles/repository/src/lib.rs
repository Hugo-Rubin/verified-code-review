//! In-process publish/subscribe fan-out.

pub mod bus;
pub mod subscriber;

pub use bus::Bus;
pub use subscriber::Subscriber;
