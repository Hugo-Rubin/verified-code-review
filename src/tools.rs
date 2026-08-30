//! Repository investigation tools.
//!
//! These are the only way an agent touches the repository, and they are the
//! only source of [`Evidence`]. The model may ask for a tool call and may
//! interpret what comes back, but it cannot author an evidence item — every
//! `Evidence` value in the system is constructed here, from bytes actually
//! read off disk.
//!
//! That is what makes "I verified this" insufficient. A claim is supported by
//! excerpts a reader can go and check, or it is not supported.
//!
//! Search is literal-substring, not regex. It is enough to find callers and
//! definitions, it cannot blow up on a pathological pattern, and it keeps the
//! tool's behaviour obvious to anyone auditing a trajectory.

use crate::config::RunConfig;
use crate::finding::{Evidence, EvidenceKind};
use crate::repo::RepoRoot;
use serde::{Deserialize, Serialize};

/// A tool invocation requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// What executing a tool produced.
pub struct ToolResult {
    /// False when the tool refused: unknown tool, bad arguments, rejected
    /// path, or no matches.
    pub ok: bool,
    /// Text handed back to the model.
    pub text: String,
    /// Repository-grounded evidence, if any.
    pub evidence: Vec<Evidence>,
}

impl ToolResult {
    fn refused(text: impl Into<String>) -> Self {
        Self {
            ok: false,
            text: text.into(),
            evidence: Vec::new(),
        }
    }
}

/// Tool names the model may request.
pub const TOOL_NAMES: [&str; 3] = ["search", "read", "list_files"];

/// Execute one tool call inside the repository sandbox.
pub fn execute(
    repo: &RepoRoot,
    call: &ToolCall,
    tool_call_id: &str,
    cfg: &RunConfig,
) -> ToolResult {
    match call.tool.as_str() {
        "search" => search(repo, call, tool_call_id, cfg),
        "read" => read(repo, call, tool_call_id, cfg),
        "list_files" => list_files(repo),
        other => ToolResult::refused(format!(
            "unknown tool {other:?}; available tools are: {}",
            TOOL_NAMES.join(", ")
        )),
    }
}

fn arg_str<'a>(call: &'a ToolCall, key: &str) -> Option<&'a str> {
    call.arguments.get(key).and_then(|v| v.as_str())
}

fn arg_u32(call: &ToolCall, key: &str) -> Option<u32> {
    call.arguments
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
}

/// Literal substring search across the repository.
fn search(repo: &RepoRoot, call: &ToolCall, tool_call_id: &str, cfg: &RunConfig) -> ToolResult {
    let Some(pattern) = arg_str(call, "pattern") else {
        return ToolResult::refused("search requires a `pattern` string argument");
    };
    if pattern.is_empty() {
        return ToolResult::refused("search `pattern` must not be empty");
    }

    // An optional path filter. Only the informative part of a glob is used:
    // matching on a suffix such as `.rs`, or on a directory prefix.
    let filter = arg_str(call, "glob").map(glob_to_filter);

    let files = match repo.list_files() {
        Ok(f) => f,
        Err(e) => return ToolResult::refused(format!("could not list repository: {e}")),
    };

    let mut lines = Vec::new();
    let mut truncated = false;

    'outer: for file in files {
        if let Some(f) = &filter {
            if !f.matches(&file) {
                continue;
            }
        }
        let Ok(content) = repo.read_to_string(&file) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                if lines.len() as u32 >= cfg.max_search_results {
                    truncated = true;
                    break 'outer;
                }
                lines.push((file.clone(), i as u32 + 1, line.trim_end().to_string()));
            }
        }
    }

    if lines.is_empty() {
        return ToolResult {
            ok: false,
            text: format!("no matches for {pattern:?}"),
            evidence: Vec::new(),
        };
    }

    let mut text = format!("{} match(es) for {:?}:\n", lines.len(), pattern);
    for (file, line_no, line) in &lines {
        text.push_str(&format!("{file}:{line_no}: {line}\n"));
    }
    if truncated {
        text.push_str(&format!(
            "... truncated at {} matches\n",
            cfg.max_search_results
        ));
    }

    let evidence = lines
        .iter()
        .map(|(file, line_no, line)| Evidence {
            kind: EvidenceKind::Search,
            file: Some(file.clone()),
            start_line: Some(*line_no),
            end_line: Some(*line_no),
            symbol: Some(pattern.to_string()),
            excerpt: line.clone(),
            tool_call_id: tool_call_id.to_string(),
        })
        .collect();

    ToolResult {
        ok: true,
        text,
        evidence,
    }
}

/// Bounded read of a line range.
fn read(repo: &RepoRoot, call: &ToolCall, tool_call_id: &str, cfg: &RunConfig) -> ToolResult {
    let Some(file) = arg_str(call, "file") else {
        return ToolResult::refused("read requires a `file` string argument");
    };

    let content = match repo.read_to_string(file) {
        Ok(c) => c,
        Err(e) => return ToolResult::refused(format!("could not read {file}: {e}")),
    };

    let total = content.lines().count() as u32;
    if total == 0 {
        return ToolResult::refused(format!("{file} is empty"));
    }

    let start = arg_u32(call, "start_line").unwrap_or(1).max(1);
    let requested_end = arg_u32(call, "end_line").unwrap_or(total);
    if requested_end < start {
        return ToolResult::refused(format!(
            "read: end_line ({requested_end}) is before start_line ({start})"
        ));
    }

    if start > total {
        return ToolResult::refused(format!(
            "read: start_line {start} is past the end of {file} ({total} lines)"
        ));
    }

    let capped_end = requested_end.min(total).min(start + cfg.max_read_lines - 1);

    let body: String = content
        .lines()
        .enumerate()
        .map(|(i, l)| (i as u32 + 1, l))
        .filter(|(n, _)| *n >= start && *n <= capped_end)
        .map(|(n, l)| format!("{n:>5} | {l}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut text = format!("{file} lines {start}-{capped_end} of {total}:\n{body}\n");
    if capped_end < requested_end {
        text.push_str(&format!(
            "... stopped at line {capped_end} (read limit {} lines)\n",
            cfg.max_read_lines
        ));
    }

    let evidence = vec![Evidence {
        kind: EvidenceKind::FileRegion,
        file: Some(crate::finding::normalize_path(file)),
        start_line: Some(start),
        end_line: Some(capped_end),
        symbol: None,
        excerpt: body,
        tool_call_id: tool_call_id.to_string(),
    }];

    ToolResult {
        ok: true,
        text,
        evidence,
    }
}

fn list_files(repo: &RepoRoot) -> ToolResult {
    match repo.list_files() {
        Err(e) => ToolResult::refused(format!("could not list repository: {e}")),
        Ok(files) => {
            let text = format!("{} file(s):\n{}\n", files.len(), files.join("\n"));
            ToolResult {
                ok: true,
                text,
                evidence: Vec::new(),
            }
        }
    }
}

/// A deliberately small subset of glob semantics.
#[derive(Debug, PartialEq, Eq)]
pub struct PathFilter {
    prefix: Option<String>,
    suffix: Option<String>,
}

impl PathFilter {
    pub fn matches(&self, path: &str) -> bool {
        if let Some(p) = &self.prefix {
            if !path.starts_with(p) {
                return false;
            }
        }
        if let Some(s) = &self.suffix {
            if !path.ends_with(s) {
                return false;
            }
        }
        true
    }
}

/// Reduce a glob to a prefix and/or suffix test.
///
/// Full glob matching is not worth a dependency here. `src/**/*.rs` becomes
/// "starts with src/ and ends with .rs", which is what such a pattern is
/// actually used for, and anything unrecognised degrades to matching
/// everything rather than to matching nothing.
pub fn glob_to_filter(glob: &str) -> PathFilter {
    let g = glob.trim().replace('\\', "/");

    let prefix = g
        .split('*')
        .next()
        .map(|p| p.trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty())
        .map(|p| format!("{p}/"));

    let suffix = g
        .rsplit('*')
        .next()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && !s.contains('/'));

    PathFilter { prefix, suffix }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> (tempfile::TempDir, RepoRoot, RunConfig) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/store.rs"),
            "fn touch() {}\nfn other() {}\nfn touch_again() {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/handler.rs"),
            "fn a() {\n  store.touch();\n}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("README.md"), "touch this\n").unwrap();
        let repo = RepoRoot::new(dir.path()).unwrap();
        (dir, repo, RunConfig::mock())
    }

    fn call(tool: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            tool: tool.to_string(),
            arguments: args,
        }
    }

    // --- search ---

    #[test]
    fn search_finds_matches_with_file_and_line() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call("search", json!({"pattern": "store.touch"})),
            "t1",
            &cfg,
        );
        assert!(r.ok);
        assert!(r.text.contains("src/handler.rs:2"));
        assert_eq!(r.evidence.len(), 1);
        assert_eq!(r.evidence[0].file.as_deref(), Some("src/handler.rs"));
        assert_eq!(r.evidence[0].start_line, Some(2));
        assert_eq!(r.evidence[0].tool_call_id, "t1");
    }

    #[test]
    fn search_evidence_excerpt_is_the_real_line() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call("search", json!({"pattern": "store.touch"})),
            "t1",
            &cfg,
        );
        assert_eq!(r.evidence[0].excerpt, "  store.touch();");
    }

    #[test]
    fn search_with_no_matches_is_not_ok_and_yields_no_evidence() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call("search", json!({"pattern": "zzz"})),
            "t1",
            &cfg,
        );
        assert!(!r.ok);
        assert!(r.evidence.is_empty());
        assert!(r.text.contains("no matches"));
    }

    #[test]
    fn search_respects_a_glob_filter() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call("search", json!({"pattern": "touch", "glob": "src/**/*.rs"})),
            "t1",
            &cfg,
        );
        assert!(r.ok);
        assert!(
            !r.text.contains("README.md"),
            "glob should exclude the markdown file"
        );
        assert!(r.text.contains("src/store.rs"));
    }

    #[test]
    fn search_requires_a_pattern() {
        let (_d, repo, cfg) = fixture();
        assert!(!execute(&repo, &call("search", json!({})), "t1", &cfg).ok);
        assert!(!execute(&repo, &call("search", json!({"pattern": ""})), "t1", &cfg).ok);
    }

    #[test]
    fn search_honours_the_result_cap() {
        let (_d, repo, mut cfg) = fixture();
        cfg.max_search_results = 1;
        let r = execute(
            &repo,
            &call("search", json!({"pattern": "touch"})),
            "t1",
            &cfg,
        );
        assert_eq!(r.evidence.len(), 1);
        assert!(r.text.contains("truncated"));
    }

    // --- read ---

    #[test]
    fn read_returns_numbered_lines_and_evidence() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call(
                "read",
                json!({"file": "src/store.rs", "start_line": 1, "end_line": 2}),
            ),
            "t2",
            &cfg,
        );
        assert!(r.ok);
        assert!(r.text.contains("    1 | fn touch() {}"));
        assert_eq!(r.evidence[0].kind, EvidenceKind::FileRegion);
        assert_eq!(r.evidence[0].start_line, Some(1));
        assert_eq!(r.evidence[0].end_line, Some(2));
    }

    #[test]
    fn read_clamps_end_line_to_the_file_length() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call(
                "read",
                json!({"file": "src/store.rs", "start_line": 1, "end_line": 9999}),
            ),
            "t2",
            &cfg,
        );
        assert!(r.ok);
        assert_eq!(r.evidence[0].end_line, Some(3));
    }

    #[test]
    fn read_honours_the_line_budget() {
        let (_d, repo, mut cfg) = fixture();
        cfg.max_read_lines = 2;
        let r = execute(
            &repo,
            &call(
                "read",
                json!({"file": "src/store.rs", "start_line": 1, "end_line": 3}),
            ),
            "t2",
            &cfg,
        );
        assert_eq!(r.evidence[0].end_line, Some(2));
        assert!(r.text.contains("read limit"));
    }

    #[test]
    fn read_refuses_a_path_outside_the_sandbox() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call("read", json!({"file": "../../../etc/passwd"})),
            "t2",
            &cfg,
        );
        assert!(!r.ok);
        assert!(r.evidence.is_empty());
    }

    #[test]
    fn read_refuses_ground_truth() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call("read", json!({"file": "ground_truth.json"})),
            "t2",
            &cfg,
        );
        assert!(!r.ok);
    }

    #[test]
    fn read_rejects_an_inverted_range() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call(
                "read",
                json!({"file": "src/store.rs", "start_line": 3, "end_line": 1}),
            ),
            "t2",
            &cfg,
        );
        assert!(!r.ok);
    }

    #[test]
    fn read_rejects_a_start_past_the_end_of_the_file() {
        let (_d, repo, cfg) = fixture();
        let r = execute(
            &repo,
            &call("read", json!({"file": "src/store.rs", "start_line": 500})),
            "t2",
            &cfg,
        );
        assert!(!r.ok);
    }

    // --- list_files ---

    #[test]
    fn list_files_returns_every_path() {
        let (_d, repo, cfg) = fixture();
        let r = execute(&repo, &call("list_files", json!({})), "t3", &cfg);
        assert!(r.ok);
        assert!(r.text.contains("src/store.rs"));
        assert!(r.text.contains("src/handler.rs"));
        // Listing a directory is not evidence about a claim.
        assert!(r.evidence.is_empty());
    }

    // --- dispatch ---

    #[test]
    fn unknown_tool_is_refused_with_the_available_names() {
        let (_d, repo, cfg) = fixture();
        let r = execute(&repo, &call("exec_shell", json!({})), "t4", &cfg);
        assert!(!r.ok);
        assert!(r.text.contains("search"));
    }

    // --- glob reduction ---

    #[test]
    fn glob_reduces_to_prefix_and_suffix() {
        let f = glob_to_filter("src/**/*.rs");
        assert!(f.matches("src/a/b.rs"));
        assert!(!f.matches("tests/a.rs"));
        assert!(!f.matches("src/a/b.md"));
    }

    #[test]
    fn suffix_only_glob_matches_anywhere() {
        let f = glob_to_filter("*.rs");
        assert!(f.matches("src/a.rs"));
        assert!(f.matches("b.rs"));
        assert!(!f.matches("a.md"));
    }

    #[test]
    fn unrecognised_glob_matches_everything_rather_than_nothing() {
        // Failing open keeps a malformed filter from silently hiding the very
        // evidence that would disprove a claim.
        let f = glob_to_filter("**");
        assert!(f.matches("anything/at/all.rs"));
    }
}
