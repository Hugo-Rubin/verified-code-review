//! Prompt text, versioned.
//!
//! Every prompt has a version string that is written into the trajectory, so a
//! result can always be traced back to the exact instructions that produced
//! it. Change the text, bump the version.
//!
//! The baseline prompt is written to be *good*. Handicapping it would make the
//! comparison meaningless, so it gets the same model, the same output schema,
//! and an explicit instruction to avoid speculation — everything a competent
//! prompt author would do. The only thing it does not get is repository tools.

use crate::finding::IssueType;

pub const BASELINE_REVIEW_V: &str = "baseline-review/v1";
pub const ADVANCED_REVIEW_V: &str = "advanced-review/v1";
pub const INVESTIGATE_V: &str = "advanced-investigate/v1";
pub const FALSIFY_V: &str = "advanced-falsify/v1";
pub const VERIFY_V: &str = "fresh-verify/v1";

/// The controlled taxonomy, rendered for a prompt.
pub fn issue_type_list() -> String {
    IssueType::ALL
        .iter()
        .map(|t| format!("  - {}", t.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn finding_schema() -> String {
    format!(
        r#"Return JSON of exactly this shape and nothing else:

{{
  "findings": [
    {{
      "issue_type": "<one of the values below, spelled exactly>",
      "severity": "Low" | "Medium" | "High",
      "file": "<repository-relative path, forward slashes>",
      "start_line": <integer, 1-based, in the file's CURRENT state after the change>,
      "end_line": <integer, >= start_line>,
      "claim": "<one sentence stating precisely what is wrong>",
      "reasoning": "<why you believe it, 1-3 sentences>"
    }}
  ]
}}

Valid issue_type values:
{}

If the change contains no real defect, return {{"findings": []}}. An empty
result is a correct answer when the code is sound."#,
        issue_type_list()
    )
}

/// Baseline: one direct review pass over the diff and the changed files.
pub fn baseline_system() -> String {
    format!(
        r#"You are an experienced Rust reviewer examining a proposed change before it merges.

Report defects that are actually present in this change. For each one, give the
file and the line range in the file's current state, classify it, and state the
problem in one sentence.

Be precise and be selective:
- Report an issue only when you are confident the code is genuinely wrong.
- A pattern that merely looks risky is not a defect. `unwrap()` on a value that
  cannot be `None`, an index that cannot be out of bounds, and a lock that
  cannot deadlock are all correct code.
- Do not report style preferences, naming, formatting, or missing comments.
- Do not invent a defect to have something to say.

{}"#,
        finding_schema()
    )
}

/// Advanced reviewer: same task, but findings are explicitly provisional
/// because an investigation stage follows.
pub fn advanced_system() -> String {
    format!(
        r#"You are an experienced Rust reviewer examining a proposed change before it merges.

You are producing CANDIDATE findings, not final ones. Each candidate will be
investigated against the repository and then independently checked, so a
candidate that turns out to be wrong costs little. What costs a lot is missing
a real defect entirely.

Propose every plausible defect in this change. For each, state the claim as a
single falsifiable sentence — something that repository evidence could confirm
or refute.

- Anchor each candidate to a specific file and line range.
- Do not report style preferences, naming, formatting, or missing comments.
- Prefer claims about behaviour ("this can panic when x is empty") over vague
  concerns ("this looks fragile").

{}"#,
        finding_schema()
    )
}

/// User message carrying the change under review.
pub fn review_user(description: &str, diff: &str, file_context: &str) -> String {
    format!(
        r#"## Change under review

{description}

## Diff

```diff
{diff}
```

## Current contents of the changed files

{file_context}"#
    )
}

/// Ask the reviewer what evidence would disprove its own candidate.
///
/// Kept as its own call so the question is fixed on the record *before* any
/// verification happens. A question written after the verdict would just
/// rationalise it.
pub fn falsify_system() -> String {
    r#"You are helping test a code-review finding.

Given a claim about a defect, write the single most decisive question whose
answer would show the claim is WRONG. The question must be answerable by
inspecting the repository: callers, implementations, tests, configuration, or
type definitions.

Good: "Does every caller of parse_row check is_empty() before indexing?"
Bad:  "Is this code correct?"

Return JSON of exactly this shape:

{"falsification_question": "<one question>"}"#
        .to_string()
}

pub fn falsify_user(claim: &str, location: &str, reasoning: &str) -> String {
    format!(
        r#"Claim: {claim}
Location: {location}
Reviewer's reasoning: {reasoning}

What evidence would prove this claim wrong?"#
    )
}

/// Investigation: choose the next repository tool call, or stop.
pub fn investigate_system(max_calls: u32) -> String {
    format!(
        r#"You are investigating a code-review claim by inspecting the repository.

Your job is to gather the evidence that answers the falsification question —
evidence that could show the claim is wrong, not only evidence that supports it.
Actively look for the disproof.

You may issue up to {max_calls} tool calls in total. Each turn, either request
one tool call or declare the investigation complete.

Available tools:

  search      {{"pattern": "<literal substring or regex>", "glob": "<optional path filter, e.g. src/**/*.rs>"}}
              Returns matching lines with file and line number.

  read        {{"file": "<repository-relative path>", "start_line": <int>, "end_line": <int>}}
              Returns the requested line range with line numbers.

  list_files  {{}}
              Returns every file path in the repository.

Return JSON of exactly this shape:

{{
  "done": false,
  "tool": "search" | "read" | "list_files",
  "arguments": {{ ... }},
  "rationale": "<what you expect this to tell you about the falsification question>"
}}

or, when you have gathered what you need:

{{ "done": true, "tool": null, "arguments": null, "rationale": "<what the evidence shows>" }}

Stop as soon as the falsification question is answered either way. Do not spend
calls confirming something you have already established."#
    )
}

pub fn investigate_user(
    claim: &str,
    location: &str,
    falsification_question: &str,
    diff: &str,
    history: &str,
) -> String {
    format!(
        r#"## Claim under investigation

{claim}

Location: {location}

## Falsification question

{falsification_question}

## Diff that produced the claim

```diff
{diff}
```

## Investigation so far

{history}

What is your next step?"#
    )
}

/// Fresh-context verifier.
///
/// This prompt is deliberately written as if the reader has never seen the
/// review. It receives the claim and the collected evidence, and nothing else
/// — in particular, not the reviewer's reasoning, and no indication that a
/// previous stage already believed the claim. The whole point is to remove the
/// anchor.
pub fn verify_system() -> String {
    r#"You are adjudicating whether a body of evidence supports a specific claim about a Rust codebase.

You did not write the claim and have no stake in it. Someone asserted it; your
task is to weigh the evidence.

Decide one of:

  "Supports"      - the evidence establishes that the claim is true.
  "Contradicts"   - the evidence establishes that the claim is false.
  "Insufficient"  - the evidence does not settle it either way.

Rules:
- Judge only from the evidence provided. Do not assume facts about code you
  were not shown.
- Absence of evidence is not evidence. If the excerpts do not cover what the
  claim depends on, the answer is "Insufficient", not "Supports".
- "Insufficient" is a perfectly good answer and is expected to be common.
- Quote the specific excerpts that decided it.

Return JSON of exactly this shape:

{
  "outcome": "Supports" | "Contradicts" | "Insufficient",
  "rationale": "<2-4 sentences explaining what the evidence shows>",
  "decisive_evidence": ["<short quote or file:line reference>", ...]
}"#
        .to_string()
}

pub fn verify_user(claim: &str, location: &str, question: &str, evidence: &str) -> String {
    format!(
        r#"## Claim

{claim}

Stated location: {location}

## Question the evidence was gathered to answer

{question}

## Evidence gathered from the repository

{evidence}

Does the evidence support the claim, contradict it, or leave it unsettled?"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_lists_every_issue_type() {
        let s = finding_schema();
        for t in IssueType::ALL {
            assert!(s.contains(t.as_str()), "schema omits {t}");
        }
    }

    #[test]
    fn baseline_and_advanced_share_the_output_schema() {
        // Fair comparison requires identical output contracts; only the
        // surrounding guidance differs.
        let schema = finding_schema();
        assert!(baseline_system().contains(&schema));
        assert!(advanced_system().contains(&schema));
    }

    #[test]
    fn verifier_prompt_never_mentions_the_reviewer() {
        // Any hint that a prior stage believed the claim reintroduces the
        // anchor the fresh context exists to remove.
        let s = verify_system().to_lowercase();
        for leak in [
            "reviewer",
            "candidate",
            "previous",
            "earlier",
            "your finding",
        ] {
            assert!(!s.contains(leak), "verifier prompt leaks context: {leak}");
        }
    }

    #[test]
    fn verifier_user_message_carries_no_reviewer_reasoning() {
        let u = verify_user("c", "src/a.rs:1", "q", "e");
        assert!(!u.to_lowercase().contains("reasoning"));
    }

    #[test]
    fn investigate_prompt_states_the_call_budget() {
        assert!(investigate_system(5).contains("up to 5 tool calls"));
    }

    #[test]
    fn prompt_versions_are_distinct() {
        let vs = [
            BASELINE_REVIEW_V,
            ADVANCED_REVIEW_V,
            INVESTIGATE_V,
            FALSIFY_V,
            VERIFY_V,
        ];
        let unique: std::collections::HashSet<_> = vs.iter().collect();
        assert_eq!(unique.len(), vs.len());
    }
}
