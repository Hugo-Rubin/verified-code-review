//! The logger front end: applies the retention limit, then stores the entry.

use crate::buffer::{Entry, RingBuffer};
use crate::Config;

#[derive(Debug, PartialEq, Eq)]
pub enum LogError {
    /// The log is already holding every entry it is allowed to hold.
    Full,
}

pub struct Logger {
    buffer: RingBuffer,
    config: Config,
    rejected: usize,
}

impl Logger {
    pub fn new(config: Config) -> Self {
        Self {
            buffer: RingBuffer::with_capacity(config.max_entries),
            config,
            rejected: 0,
        }
    }

    /// Record one entry.
    ///
    /// The log reports [`LogError::Full`] to the caller rather than discarding
    /// anything once it is holding its full complement of entries.
    pub fn append(&mut self, level: u8, message: &str) -> Result<(), LogError> {
        if self.buffer.len() >= self.config.max_entries {
            self.rejected += 1;
            return Err(LogError::Full);
        }
        self.buffer.push(Entry {
            level,
            message: message.to_string(),
        });
        Ok(())
    }

    /// How many appends were refused because the log was full.
    pub fn rejected(&self) -> usize {
        self.rejected
    }

    /// How many entries are currently held.
    pub fn stored(&self) -> usize {
        self.buffer.len()
    }

    pub fn messages(&self) -> Vec<String> {
        self.buffer.iter().map(|e| e.message.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logger(max: usize) -> Logger {
        Logger::new(Config::new(max))
    }

    #[test]
    fn stores_appended_entries() {
        let mut l = logger(4);
        assert!(l.append(1, "a").is_ok());
        assert!(l.append(1, "b").is_ok());
        assert_eq!(l.stored(), 2);
        assert_eq!(l.messages(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn reports_full_at_the_configured_limit() {
        let mut l = logger(2);
        assert!(l.append(1, "a").is_ok());
        assert!(l.append(1, "b").is_ok());
        assert_eq!(l.append(1, "c"), Err(LogError::Full));
        assert_eq!(l.rejected(), 1);
        assert_eq!(l.stored(), 2);
    }

    #[test]
    fn a_refused_append_stores_nothing() {
        let mut l = logger(1);
        assert!(l.append(1, "a").is_ok());
        let _ = l.append(1, "b");
        assert_eq!(l.messages(), vec!["a".to_string()]);
    }
}
