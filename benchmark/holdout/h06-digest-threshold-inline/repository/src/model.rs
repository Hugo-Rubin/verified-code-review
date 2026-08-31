//! The alert record and the rules that classify it.

/// One alert as delivered by the ingester. `severity` runs from 0 (chatter)
/// to 10 (site down).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub id: u32,
    pub source: String,
    pub severity: u8,
}

impl Alert {
    pub fn new(id: u32, source: &str, severity: u8) -> Self {
        Alert {
            id,
            source: source.to_string(),
            severity,
        }
    }
}

/// The lowest severity that still wakes the on-call engineer.
///
/// Agreed with the operations team: severity 7 is "customer-visible
/// degradation", and that is the point at which somebody gets paged.
pub const PAGE_THRESHOLD: u8 = 7;

/// Whether `alert` is severe enough to page the on-call engineer.
pub fn is_page_worthy(alert: &Alert) -> bool {
    alert.severity >= PAGE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_threshold_itself_pages() {
        assert!(is_page_worthy(&Alert::new(1, "api", PAGE_THRESHOLD)));
    }

    #[test]
    fn one_below_the_threshold_does_not_page() {
        assert!(!is_page_worthy(&Alert::new(2, "api", PAGE_THRESHOLD - 1)));
    }

    #[test]
    fn a_site_down_alert_pages() {
        assert!(is_page_worthy(&Alert::new(3, "edge", 10)));
    }
}
