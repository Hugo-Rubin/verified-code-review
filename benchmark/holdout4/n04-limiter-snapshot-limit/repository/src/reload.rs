//! The control channel: applies operator updates to the live settings.

use crate::settings::Settings;

/// One instruction from the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    ConcurrentJobs(u32),
}

/// Apply an update to the process settings.
///
/// This runs mid-flight, from the same loop that starts jobs: the runner polls
/// the control channel between job starts, so admission control is already
/// built and running when an update arrives. Raising the value is how an
/// operator opens the throttle; lowering it is how they shed load without a
/// restart.
pub fn apply(settings: &Settings, update: Update) {
    match update {
        Update::ConcurrentJobs(n) => settings.set_concurrent_jobs(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_raises_the_limit() {
        let s = Settings::new(2);
        apply(&s, Update::ConcurrentJobs(8));
        assert_eq!(s.concurrent_jobs(), 8);
    }

    #[test]
    fn an_update_lowers_the_limit() {
        let s = Settings::new(8);
        apply(&s, Update::ConcurrentJobs(1));
        assert_eq!(s.concurrent_jobs(), 1);
    }
}
