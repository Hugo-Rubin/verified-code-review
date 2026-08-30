//! Retry planning, driven entirely by the outcome class.

use crate::classify::classify;

/// Milliseconds to wait before attempt `attempt` (0-based), or `None` when
/// the response should not be retried at all.
pub fn plan_retry(code: u16, attempt: u32, max_attempts: u32) -> Option<u64> {
    if attempt + 1 >= max_attempts {
        return None;
    }
    if !classify(code).retryable() {
        return None;
    }
    Some(100u64 << attempt.min(6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_success_is_never_retried() {
        assert_eq!(plan_retry(200, 0, 5), None);
    }

    #[test]
    fn the_last_attempt_never_schedules_another() {
        assert_eq!(plan_retry(404, 4, 5), None);
    }
}
