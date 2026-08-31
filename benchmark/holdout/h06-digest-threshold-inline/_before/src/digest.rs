//! Building the on-call digest.

use crate::model::{is_page_worthy, Alert};

/// What the on-call engineer is shown for one batch of alerts.
#[derive(Debug, PartialEq, Eq)]
pub struct Digest {
    /// Ids of the alerts that should page somebody, in arrival order.
    pub paging: Vec<u32>,
    /// How many alerts the batch contained in total.
    pub total_seen: usize,
    /// Distinct sources represented in the batch, sorted.
    pub sources: Vec<String>,
}

/// Summarise one batch of alerts.
pub fn build(alerts: &[Alert]) -> Digest {
    let paging: Vec<u32> = alerts
        .iter()
        .filter(|alert| is_page_worthy(alert))
        .map(|alert| alert.id)
        .collect();

    let mut sources: Vec<String> = alerts.iter().map(|a| a.source.clone()).collect();
    sources.sort();
    sources.dedup();

    Digest {
        paging,
        total_seen: alerts.len(),
        sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_down_alert_pages() {
        let batch = [
            Alert::new(1, "edge", 10),
            Alert::new(2, "api", 2),
            Alert::new(3, "api", 1),
        ];
        assert_eq!(build(&batch).paging, vec![1]);
    }

    #[test]
    fn a_quiet_batch_pages_nobody() {
        let batch = [Alert::new(1, "api", 3), Alert::new(2, "api", 5)];
        assert!(build(&batch).paging.is_empty());
    }

    #[test]
    fn every_alert_is_counted_and_sources_are_deduplicated() {
        let batch = [
            Alert::new(1, "edge", 9),
            Alert::new(2, "api", 2),
            Alert::new(3, "api", 1),
        ];
        let d = build(&batch);
        assert_eq!(d.total_seen, 3);
        assert_eq!(d.sources, vec!["api".to_string(), "edge".to_string()]);
    }
}
