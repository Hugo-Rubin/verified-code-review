//! The job a leased slot is used for: summing a comma-separated list.

use crate::lease::LeasePool;

#[derive(Debug, PartialEq, Eq)]
pub enum JobError {
    Empty,
    NotANumber(String),
}

#[derive(Debug, Default)]
pub struct Worker {
    pool: LeasePool,
}

impl Worker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Slots currently taken by in-flight jobs.
    pub fn active(&self) -> usize {
        self.pool.active()
    }

    pub fn completions(&self) -> Vec<(usize, i64)> {
        self.pool.completions()
    }

    /// Parse `input` as a comma-separated list of integers and sum them.
    pub fn run(&self, input: &str) -> Result<i64, JobError> {
        let lease = self.pool.lease();

        let body = non_empty(input)?;
        let mut total = 0i64;
        for field in body.split(',') {
            total += parse_field(field)?;
        }

        self.pool.record_completion(lease, total);
        Ok(total)
    }
}

fn non_empty(input: &str) -> Result<&str, JobError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(JobError::Empty);
    }
    Ok(trimmed)
}

fn parse_field(field: &str) -> Result<i64, JobError> {
    field
        .trim()
        .parse::<i64>()
        .map_err(|_| JobError::NotANumber(field.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_a_list() {
        let w = Worker::new();
        assert_eq!(w.run("1, 2, 3"), Ok(6));
        assert_eq!(w.completions(), vec![(0, 6)]);
    }

    #[test]
    fn rejects_blank_input() {
        let w = Worker::new();
        assert_eq!(w.run("   "), Err(JobError::Empty));
    }

    #[test]
    fn rejects_a_non_numeric_field() {
        let w = Worker::new();
        assert_eq!(
            w.run("1,two,3"),
            Err(JobError::NotANumber("two".to_string()))
        );
    }
}
