//! The settings every component in the process shares.

use std::cell::Cell;

/// Operator settings.
///
/// A `Settings` value is a live view rather than a snapshot: the control
/// channel installs new values into it while the process is running (see
/// [`crate::reload`]), and every component holding a reference is expected to
/// observe the new value on its next read. The interior mutability is what
/// makes that possible without unique access to something the whole process
/// borrows.
#[derive(Debug)]
pub struct Settings {
    concurrent_jobs: Cell<u32>,
}

impl Settings {
    pub fn new(concurrent_jobs: u32) -> Self {
        Self {
            concurrent_jobs: Cell::new(concurrent_jobs),
        }
    }

    /// How many jobs may be in flight at once, as of right now.
    pub fn concurrent_jobs(&self) -> u32 {
        self.concurrent_jobs.get()
    }

    /// Install a new value. Every holder of a `&Settings` sees it from here on.
    pub fn set_concurrent_jobs(&self, jobs: u32) {
        self.concurrent_jobs.set(jobs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_taken_earlier_sees_the_new_value() {
        let s = Settings::new(2);
        let view: &Settings = &s;
        s.set_concurrent_jobs(9);
        assert_eq!(view.concurrent_jobs(), 9);
    }

    #[test]
    fn the_initial_value_is_what_was_configured() {
        assert_eq!(Settings::new(3).concurrent_jobs(), 3);
    }
}
