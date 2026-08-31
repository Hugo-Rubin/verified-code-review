//! Fan-out of published messages to subscriber mailboxes.

use crate::subscriber::Subscriber;
use std::sync::{Arc, Mutex, Weak};

/// Delivers every published message to each live subscriber.
///
/// The bus does not own its subscribers: a subscription lasts exactly as long
/// as the caller keeps the `Arc<Subscriber>` returned by `subscribe`.
#[derive(Default)]
pub struct Bus {
    subs: Mutex<Vec<Weak<Subscriber>>>,
}

impl Bus {
    /// Register a subscriber and hand its mailbox back to the caller.
    pub fn subscribe(&self, name: &str) -> Arc<Subscriber> {
        let sub = Arc::new(Subscriber::new(name));
        self.subs
            .lock()
            .expect("bus poisoned")
            .push(Arc::downgrade(&sub));
        sub
    }

    /// Deliver `message` to every live subscriber, returning how many received
    /// it.
    pub fn publish(&self, message: &str) -> usize {
        let mut subs = self.subs.lock().expect("bus poisoned");
        subs.retain(|weak| weak.strong_count() > 0);
        let mut delivered = 0;
        for weak in subs.iter() {
            if let Some(sub) = weak.upgrade() {
                sub.deliver(message);
                delivered += 1;
            }
        }
        delivered
    }

    /// How many subscriptions are currently live.
    pub fn subscriber_count(&self) -> usize {
        let mut subs = self.subs.lock().expect("bus poisoned");
        subs.retain(|weak| weak.strong_count() > 0);
        subs.len()
    }

    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_reaches_every_subscriber() {
        let bus = Bus::new();
        let a = bus.subscribe("a");
        let b = bus.subscribe("b");
        assert_eq!(bus.publish("tick"), 2);
        assert_eq!(a.messages(), vec!["tick".to_string()]);
        assert_eq!(b.messages(), vec!["tick".to_string()]);
    }

    #[test]
    fn subscriber_count_tracks_registrations() {
        let bus = Bus::new();
        let _a = bus.subscribe("a");
        assert_eq!(bus.subscriber_count(), 1);
        let _b = bus.subscribe("b");
        assert_eq!(bus.subscriber_count(), 2);
    }

    #[test]
    fn publishing_to_an_empty_bus_delivers_nothing() {
        let bus = Bus::new();
        assert_eq!(bus.publish("tick"), 0);
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn a_subscriber_keeps_its_name() {
        let bus = Bus::new();
        let sub = bus.subscribe("alerts");
        assert_eq!(sub.name(), "alerts");
    }
}
