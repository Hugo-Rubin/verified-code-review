//! Admission control: decides whether one more job may start.

use crate::settings::Settings;

/// Gate in front of the job runner.
///
/// One limiter is built when the runner starts and lives for as long as the
/// process does; `try_start` runs on every job start.
pub struct Limiter<'a> {
    settings: &'a Settings,
    running: u32,
}

impl<'a> Limiter<'a> {
    pub fn new(settings: &'a Settings) -> Self {
        Self {
            settings,
            running: 0,
        }
    }

    /// Admit one job if there is room for it.
    pub fn try_start(&mut self) -> bool {
        if self.running >= self.settings.concurrent_jobs() {
            return false;
        }
        self.running += 1;
        true
    }

    /// Report that a job finished and gave its slot back.
    pub fn finish(&mut self) {
        self.running = self.running.saturating_sub(1);
    }

    /// Jobs currently in flight.
    pub fn running(&self) -> u32 {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_up_to_the_limit_and_no_further() {
        let s = Settings::new(2);
        let mut l = Limiter::new(&s);
        assert!(l.try_start());
        assert!(l.try_start());
        assert!(!l.try_start());
        assert_eq!(l.running(), 2);
    }

    #[test]
    fn a_finished_job_makes_room_for_another() {
        let s = Settings::new(1);
        let mut l = Limiter::new(&s);
        assert!(l.try_start());
        assert!(!l.try_start());
        l.finish();
        assert!(l.try_start());
        assert_eq!(l.running(), 1);
    }

    #[test]
    fn finishing_with_nothing_in_flight_stays_at_zero() {
        let s = Settings::new(4);
        let mut l = Limiter::new(&s);
        l.finish();
        assert_eq!(l.running(), 0);
    }

    #[test]
    fn a_limit_of_zero_admits_nothing() {
        let s = Settings::new(0);
        let mut l = Limiter::new(&s);
        assert!(!l.try_start());
    }
}
