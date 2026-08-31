//! A token bucket.

/// Admits requests up to a burst, refilling at a fixed rate.
///
/// A `burst` of zero is the "throttling off" sentinel. A limiter built with it
/// admits every request and never looks at the rate. Operators set it when
/// they want a tenant exempted without deleting the tenant's configuration.
pub struct Limiter {
    rate_per_sec: u32,
    burst: u32,
    tokens: u32,
}

impl Limiter {
    pub fn new(rate_per_sec: u32, burst: u32) -> Self {
        Self {
            rate_per_sec,
            burst,
            tokens: burst,
        }
    }

    /// True when this limiter admits everything.
    pub fn is_unlimited(&self) -> bool {
        self.burst == 0
    }

    /// Admit one request, spending a token unless throttling is off.
    pub fn allow(&mut self) -> bool {
        if self.is_unlimited() {
            return true;
        }
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }

    /// Account for one elapsed second.
    pub fn tick(&mut self) {
        if self.is_unlimited() {
            return;
        }
        self.tokens = self.tokens.saturating_add(self.rate_per_sec).min(self.burst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_up_to_the_burst_then_refuses() {
        let mut l = Limiter::new(1, 2);
        assert!(l.allow());
        assert!(l.allow());
        assert!(!l.allow());
    }

    #[test]
    fn a_tick_refills_up_to_the_burst() {
        let mut l = Limiter::new(10, 2);
        assert!(l.allow());
        assert!(l.allow());
        l.tick();
        assert!(l.allow());
        assert!(l.allow());
        assert!(!l.allow());
    }

    #[test]
    fn a_zero_burst_admits_everything() {
        let mut l = Limiter::new(1, 0);
        assert!(l.is_unlimited());
        for _ in 0..1000 {
            assert!(l.allow());
        }
    }
}
