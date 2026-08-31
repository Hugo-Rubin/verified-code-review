//! The metrics sampler. Scraped once per interval.

use crate::counters::Counters;

#[derive(Debug, PartialEq, Eq)]
pub struct Gauges {
    pub accepted: u64,
    pub rejected: u64,
}

/// Sample the pipeline counters for the metrics endpoint.
///
/// One call per scrape interval; the values reported are the records seen
/// since the previous scrape.
pub fn sample(counters: &Counters) -> Gauges {
    Gauges {
        accepted: counters.accepted(),
        rejected: counters.rejected(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Pipeline;

    #[test]
    fn sampling_reports_what_the_pipeline_recorded() {
        let mut p = Pipeline::new();
        p.ingest("a");
        p.ingest("b");
        p.ingest("   ");
        assert_eq!(
            sample(p.counters()),
            Gauges {
                accepted: 2,
                rejected: 1
            }
        );
    }

    #[test]
    fn a_second_scrape_covers_only_new_records() {
        let mut p = Pipeline::new();
        p.ingest("a");
        let _ = sample(p.counters());
        assert_eq!(
            sample(p.counters()),
            Gauges {
                accepted: 0,
                rejected: 0
            }
        );
    }
}
