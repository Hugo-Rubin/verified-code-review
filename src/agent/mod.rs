//! Review agents and the parsing they share.

pub mod advanced;
pub mod baseline;

use crate::bench::Case;
use crate::finding::{CandidateFinding, IssueType, Location, Severity};
use crate::repo::RepoRoot;
use serde::Deserialize;

/// The raw shape a model is asked to emit.
#[derive(Debug, Deserialize)]
struct RawFinding {
    issue_type: String,
    #[serde(default)]
    severity: Option<String>,
    file: String,
    start_line: u32,
    #[serde(default)]
    end_line: Option<u32>,
    claim: String,
    #[serde(default)]
    reasoning: String,
}

/// Outcome of parsing a review response.
pub struct ParsedReview {
    pub candidates: Vec<CandidateFinding>,
    /// Findings that had to be dropped, with the reason. Recorded in the
    /// trajectory rather than silently discarded — a model that keeps emitting
    /// invented issue types is a real result, not noise to hide.
    pub warnings: Vec<String>,
}

fn parse_issue_type(s: &str) -> Option<IssueType> {
    IssueType::ALL.into_iter().find(|t| t.as_str() == s.trim())
}

fn parse_severity(s: Option<&str>) -> Severity {
    match s.map(|s| s.trim()) {
        Some("Low") => Severity::Low,
        Some("High") => Severity::High,
        // Medium is the neutral default; severity does not affect scoring.
        _ => Severity::Medium,
    }
}

/// Parse a `{"findings": [...]}` response into candidates.
///
/// `id_prefix` disambiguates ids across stages within a case.
pub fn parse_review(value: &serde_json::Value, id_prefix: &str) -> ParsedReview {
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();

    let Some(items) = value.get("findings").and_then(|f| f.as_array()) else {
        warnings.push("response had no `findings` array".to_string());
        return ParsedReview {
            candidates,
            warnings,
        };
    };

    for (i, item) in items.iter().enumerate() {
        let raw: RawFinding = match serde_json::from_value(item.clone()) {
            Ok(r) => r,
            Err(e) => {
                warnings.push(format!("finding[{i}] dropped: {e}"));
                continue;
            }
        };

        let Some(issue_type) = parse_issue_type(&raw.issue_type) else {
            warnings.push(format!(
                "finding[{i}] dropped: issue_type {:?} is not in the controlled taxonomy",
                raw.issue_type
            ));
            continue;
        };

        if raw.start_line == 0 {
            warnings.push(format!(
                "finding[{i}] dropped: start_line 0 (lines are 1-based)"
            ));
            continue;
        }

        if raw.claim.trim().is_empty() {
            warnings.push(format!("finding[{i}] dropped: empty claim"));
            continue;
        }

        let end_line = raw.end_line.unwrap_or(raw.start_line).max(raw.start_line);

        candidates.push(CandidateFinding {
            id: format!("{id_prefix}-{}", i + 1),
            issue_type,
            severity: parse_severity(raw.severity.as_deref()),
            location: Location::new(&raw.file, raw.start_line, end_line),
            claim: raw.claim.trim().to_string(),
            reasoning: raw.reasoning.trim().to_string(),
        });
    }

    ParsedReview {
        candidates,
        warnings,
    }
}

/// Repository-relative paths touched by a unified diff.
pub fn changed_files(diff: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in diff.lines() {
        // Prefer the "+++ b/path" side: it names the file's post-change path.
        if let Some(rest) = line.strip_prefix("+++ ") {
            let path = rest.trim();
            if path == "/dev/null" {
                continue;
            }
            let path = path
                .strip_prefix("b/")
                .or_else(|| path.strip_prefix("a/"))
                .unwrap_or(path);
            // Strip a trailing timestamp column, which some diff tools add.
            let path = path.split('\t').next().unwrap_or(path).trim();
            let normalized = crate::finding::normalize_path(path);
            if !normalized.is_empty() && !files.contains(&normalized) {
                files.push(normalized);
            }
        }
    }
    files
}

/// Render the current contents of the changed files, with line numbers.
///
/// Bounded per file so a large file cannot crowd out the rest of the prompt.
/// Both agents use this, so neither gets a context advantage from it.
pub fn build_file_context(repo: &RepoRoot, files: &[String], max_lines_per_file: u32) -> String {
    let mut out = String::new();

    for file in files {
        out.push_str(&format!("\n### {file}\n\n"));
        match repo.read_to_string(file) {
            Ok(content) => {
                out.push_str("```rust\n");
                out.push_str(&number_lines(&content, 1, max_lines_per_file));
                let total = content.lines().count() as u32;
                if total > max_lines_per_file {
                    out.push_str(&format!(
                        "... [{} further lines not shown]\n",
                        total - max_lines_per_file
                    ));
                }
                out.push_str("```\n");
            }
            Err(e) => {
                out.push_str(&format!("(could not read: {e})\n"));
            }
        }
    }

    if out.is_empty() {
        out.push_str("(no changed files could be read)\n");
    }
    out
}

/// Render `content` with 1-based line numbers, starting at `from` and emitting
/// at most `limit` lines.
pub fn number_lines(content: &str, from: u32, limit: u32) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, l)| (i as u32 + 1, l))
        .filter(|(n, _)| *n >= from)
        .take(limit as usize)
        .map(|(n, l)| format!("{n:>5} | {l}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Convenience: the file context for a case, as both agents see it.
pub fn case_file_context(case: &Case, max_lines_per_file: u32) -> String {
    let files = changed_files(&case.diff);
    build_file_context(&case.repo, &files, max_lines_per_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn parses_a_well_formed_finding() {
        let v = json(
            r#"{"findings":[{"issue_type":"ErrorHandling","severity":"High",
                 "file":"src/a.rs","start_line":10,"end_line":12,
                 "claim":"can panic","reasoning":"because"}]}"#,
        );
        let p = parse_review(&v, "b");
        assert_eq!(p.candidates.len(), 1);
        let c = &p.candidates[0];
        assert_eq!(c.id, "b-1");
        assert_eq!(c.issue_type, IssueType::ErrorHandling);
        assert_eq!(c.severity, Severity::High);
        assert_eq!(c.location.to_string(), "src/a.rs:10-12");
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn empty_findings_is_a_valid_answer() {
        let p = parse_review(&json(r#"{"findings":[]}"#), "b");
        assert!(p.candidates.is_empty());
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn missing_findings_key_warns_rather_than_panicking() {
        let p = parse_review(&json(r#"{"result":"none"}"#), "b");
        assert!(p.candidates.is_empty());
        assert_eq!(p.warnings.len(), 1);
    }

    #[test]
    fn invented_issue_type_is_dropped_and_reported() {
        let v = json(
            r#"{"findings":[{"issue_type":"SecurityVulnerability","file":"src/a.rs",
                 "start_line":1,"claim":"x"}]}"#,
        );
        let p = parse_review(&v, "b");
        assert!(p.candidates.is_empty());
        assert!(p.warnings[0].contains("controlled taxonomy"));
    }

    #[test]
    fn one_bad_finding_does_not_discard_the_good_ones() {
        let v = json(
            r#"{"findings":[
                {"issue_type":"Nonsense","file":"a.rs","start_line":1,"claim":"x"},
                {"issue_type":"Correctness","file":"a.rs","start_line":2,"claim":"y"}]}"#,
        );
        let p = parse_review(&v, "b");
        assert_eq!(p.candidates.len(), 1);
        assert_eq!(p.candidates[0].id, "b-2", "ids track the original index");
        assert_eq!(p.warnings.len(), 1);
    }

    #[test]
    fn missing_end_line_defaults_to_start_line() {
        let v = json(
            r#"{"findings":[{"issue_type":"Testing","file":"a.rs","start_line":7,"claim":"x"}]}"#,
        );
        let c = &parse_review(&v, "b").candidates[0];
        assert_eq!(c.location.to_string(), "a.rs:7");
    }

    #[test]
    fn end_line_before_start_line_is_clamped() {
        let v = json(
            r#"{"findings":[{"issue_type":"Testing","file":"a.rs","start_line":7,
                 "end_line":3,"claim":"x"}]}"#,
        );
        let c = &parse_review(&v, "b").candidates[0];
        assert_eq!(c.location.start_line, 7);
        assert_eq!(c.location.end_line, 7);
    }

    #[test]
    fn zero_line_and_empty_claim_are_dropped() {
        let v = json(
            r#"{"findings":[
                {"issue_type":"Testing","file":"a.rs","start_line":0,"claim":"x"},
                {"issue_type":"Testing","file":"a.rs","start_line":1,"claim":"   "}]}"#,
        );
        let p = parse_review(&v, "b");
        assert!(p.candidates.is_empty());
        assert_eq!(p.warnings.len(), 2);
    }

    #[test]
    fn missing_severity_defaults_to_medium() {
        let v = json(
            r#"{"findings":[{"issue_type":"Testing","file":"a.rs","start_line":1,"claim":"x"}]}"#,
        );
        assert_eq!(
            parse_review(&v, "b").candidates[0].severity,
            Severity::Medium
        );
    }

    // --- diff parsing ---

    #[test]
    fn extracts_changed_files_from_a_unified_diff() {
        let diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-x\n+y\n";
        assert_eq!(changed_files(diff), vec!["src/a.rs"]);
    }

    #[test]
    fn handles_multiple_files_without_duplicates() {
        let diff = "--- a/x.rs\n+++ b/x.rs\n--- a/y.rs\n+++ b/y.rs\n--- a/x.rs\n+++ b/x.rs\n";
        assert_eq!(changed_files(diff), vec!["x.rs", "y.rs"]);
    }

    #[test]
    fn ignores_dev_null_for_deleted_files() {
        let diff = "--- a/gone.rs\n+++ /dev/null\n";
        assert!(changed_files(diff).is_empty());
    }

    #[test]
    fn strips_trailing_timestamp_column() {
        let diff = "+++ b/src/a.rs\t2026-08-30 10:00:00.000000000 +0000\n";
        assert_eq!(changed_files(diff), vec!["src/a.rs"]);
    }

    #[test]
    fn empty_diff_yields_no_files() {
        assert!(changed_files("").is_empty());
    }

    // --- line numbering ---

    #[test]
    fn numbers_lines_from_one() {
        let s = number_lines("a\nb\nc\n", 1, 10);
        assert!(s.starts_with("    1 | a"));
        assert!(s.contains("    3 | c"));
    }

    #[test]
    fn number_lines_respects_start_and_limit() {
        let s = number_lines("a\nb\nc\nd\n", 2, 2);
        assert!(s.contains("    2 | b"));
        assert!(s.contains("    3 | c"));
        assert!(!s.contains("| a"));
        assert!(!s.contains("| d"));
    }
}
