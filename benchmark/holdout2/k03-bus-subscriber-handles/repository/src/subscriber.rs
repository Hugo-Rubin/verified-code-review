//! A subscriber mailbox.

use std::sync::Mutex;

/// A mailbox owned by whoever called `Bus::subscribe`.
pub struct Subscriber {
    name: String,
    inbox: Mutex<Vec<String>>,
}

impl Subscriber {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            inbox: Mutex::new(Vec::new()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn deliver(&self, message: &str) {
        self.inbox
            .lock()
            .expect("inbox poisoned")
            .push(message.to_string());
    }

    pub fn messages(&self) -> Vec<String> {
        self.inbox.lock().expect("inbox poisoned").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_messages_are_kept_in_order() {
        let sub = Subscriber::new("alerts");
        sub.deliver("one");
        sub.deliver("two");
        assert_eq!(sub.name(), "alerts");
        assert_eq!(sub.messages(), vec!["one".to_string(), "two".to_string()]);
    }
}
