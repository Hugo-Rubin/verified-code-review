//! Prompt text, versioned.
//!
//! Every prompt has a version string that is written into the trajectory, so a
//! result can always be traced back to the exact instructions that produced
//! it. Change the text, bump the version.
//!
//! The baseline prompt is written to be *good*. Handicapping it would make the
//! comparison meaningless, so it gets the same model, the same task, the same
//! JSON contract, the same view of the diff and changed files, and an explicit
//! instruction to avoid speculation — everything a competent prompt author
//! would do. The only thing it does not get is repository tools.
//!
//! The two arms do differ in one instruction, deliberately: the baseline is
//! told that an empty answer is correct when the code is sound, while the
//! advanced reviewer is told to err toward proposing. That is not a resource
//! advantage, it is the design under test. The baseline's output IS its
//! report, so confidence is the right bar. The advanced reviewer's output is a
//! worklist for an investigation stage that can settle uncertainty against the
//! repository, so the right bar there is "worth checking". Both are scored on
//! the same thing: what a human is finally shown.

use crate::finding::IssueType;

pub const BASELINE_REVIEW_V: &str = "baseline-review/v2";
pub const ADVANCED_REVIEW_V: &str = "advanced-review/v6";
pub const INVESTIGATE_V: &str = "advanced-investigate/v2";
pub const FALSIFY_V: &str = "advanced-falsify/v2";
pub const VERIFY_V: &str = "fresh-verify/v5";

/// The controlled taxonomy, rendered for a prompt.
pub fn issue_type_list() -> String {
    IssueType::ALL
        .iter()
        .map(|t| format!("  - {}", t.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The shared JSON contract. `closing` differs between the two arms because
/// they are answering different questions: the baseline is asked what it would
/// report, the advanced reviewer is asked what is worth checking.
fn finding_schema_with(closing: &str) -> String {
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

{}"#,
        issue_type_list(),
        closing
    )
}

/// Baseline closing: an empty answer is a correct answer.
fn baseline_schema() -> String {
    finding_schema_with(
        r#"If the change contains no real defect, return {"findings": []}. An empty
result is a correct answer when the code is sound."#,
    )
}

/// Advanced closing: these are candidates, and under-proposing is the
/// expensive mistake.
///
/// The baseline is asked what it would put in front of a human, so silence is
/// the right answer when it is not confident. The advanced reviewer is asked
/// what deserves checking, and every candidate is investigated against the
/// repository and independently adjudicated before anything reaches a human —
/// so a wrong candidate is cheap and a missed one is not. Telling this stage
/// that an empty answer is correct suppresses exactly the uncertain candidates
/// the pipeline exists to resolve.
fn advanced_schema() -> String {
    finding_schema_with(
        r#"Return {"findings": []} only if there is genuinely nothing worth checking.

Under-proposing is the expensive mistake at this stage. Include a candidate
even when you suspect it is fine, and in particular whenever the code's
correctness depends on something you cannot see in the changed files — a
caller, a constructor, an invariant asserted in a comment, a value's origin.
Those are the candidates the investigation stage exists to settle, and it can
only settle the ones you raise."#,
    )
}

/// Baseline: one direct review pass over the diff and the changed files.
pub fn baseline_system(language: &str) -> String {
    format!(
        r#"You are an experienced {language} reviewer examining a proposed change before it merges.

Report defects that are actually present in this change. For each one, give the
file and the line range in the file's current state, classify it, and state the
problem in one sentence.

Be precise and be selective:
- Report an issue only when you are confident the code is genuinely wrong.
- A pattern that merely looks risky is not a defect.
- Do not report style preferences, naming, formatting, or missing comments.
- Do not invent a defect to have something to say.

{schema}"#,
        schema = baseline_schema()
    )
}

/// Advanced reviewer: same task, but findings are explicitly provisional
/// because an investigation stage follows.
pub fn advanced_system(language: &str) -> String {
    format!(
        r#"You are an experienced {language} reviewer examining a proposed change before it merges.

You are producing CANDIDATE findings, not final ones. Nothing you write here
reaches a human directly. Each candidate is investigated against the actual
repository and then independently adjudicated, and only what survives is
reported. A candidate that turns out to be wrong costs almost nothing. A real
defect you never raised is never checked at all.

So err toward proposing. Raise anything a careful reviewer would want confirmed
before approving this change, including things you expect will turn out to be
fine.

- Anchor each candidate to a specific file and line range.
- State each claim as something that can actually happen, not as a
  conditional. Write "callers can reach this with an empty list, and it will
  panic", not "this will panic if the list is empty" — the second is true of
  the code whether or not the situation ever arises, so it cannot be
  disproved and is not a finding.
- State each claim as a single falsifiable sentence — something repository
  evidence could confirm or refute.
- Every candidate must name a consequence: what goes wrong, and for whom.
  Incorrect output, a panic, data loss, a violated contract, a security
  consequence, or a real cost at the scale this code runs at. If you cannot
  finish the sentence "and so the user gets...", it is not a candidate.
  Missing trait derives, a signature that is more fallible than it needs to
  be, and unused declarations are observations about the code, not defects —
  leave them out. The investigation stage can settle whether a claim is true;
  it cannot make a true triviality worth a reviewer's time.
- Pay particular attention to anything whose correctness depends on facts that
  are not visible in the changed files. If the code is only correct given some
  assumption about the rest of the repository, that assumption is a candidate.
- Apply that concretely. Where this change calls something whose definition you
  cannot see here, ask what it must return or do for this code to be right, and
  raise that as a candidate stating the assumption. A guard is only as good as
  the thing it calls; a name suggests what a function does but does not
  establish it. This is the single most common way a change that looks correct
  in isolation turns out not to be, and it is precisely what the investigation
  stage can settle in one lookup.
- Do not report style preferences, naming, formatting, or missing comments.
- Avoid vague concerns such as "this looks fragile"; say what would go wrong.

{schema}"#,
        schema = advanced_schema()
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

Ask about the thing the claim actually depends on, not about the mechanism.
Whether a function panics on a bad input is usually not in doubt and can be
read off the body; whether anything ever hands it a bad input is the question.
So when a claim rests on how the code is used, ask about the call sites by
name.

Good: "Does every caller of parse_row check is_empty() before indexing?"
Bad:  "Does parse_row panic when the row is empty?"  (asks the mechanism)
Bad:  "Is this code correct?"                        (unanswerable)

Never make a comment the answer. If a doc comment already asserts the
precondition, the question is whether the callers honour it.

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

  search      {{"pattern": "<literal substring>", "glob": "<optional path filter, e.g. src/**/*.rs>"}}
              Case-sensitive literal substring match, not a regular
              expression. Returns matching lines with file and line number.

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
    gap: Option<&str>,
    memory: Option<&str>,
) -> String {
    // When an independent check has already looked at the evidence and said
    // what it could not settle, that is the most useful steer available — far
    // better than letting the investigation pick a direction again from
    // scratch.
    let gap_section = match gap {
        None => String::new(),
        Some(g) => format!(
            r#"
## An independent check has already reviewed the evidence below

It could not settle the claim either way, and said this was missing:

> {g}

Gather specifically what would close that gap. If you cannot, say so and stop
rather than collecting more of what has already proved insufficient.
"#
        ),
    };

    // Facts already gathered while investigating other candidates in this same
    // review. Lookups only — no conclusions, so nothing here can anchor this
    // investigation toward an earlier verdict.
    let memory_section = match memory {
        None => String::new(),
        Some(m) => format!(
            r#"
## Already looked up during this review

These are results of earlier tool calls on this repository, recorded so you do
not have to repeat them. They are lookups, not conclusions.

{m}
"#
        ),
    };

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
{gap_section}{memory_section}
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
    r#"You are deciding whether a body of evidence establishes that a codebase has a real defect.

You did not write the claim and have no stake in it. Someone asserted it; your
task is to weigh the evidence.

Decide one of:

  "Supports"      - the evidence establishes a real defect, as claimed.
  "Contradicts"   - the evidence establishes that this is not a defect.
  "Insufficient"  - the evidence does not settle it either way.

Rules:
- Judge only from the evidence provided. Do not assume facts about code you
  were not shown.
- Absence of evidence is not evidence. If the excerpts do not cover what the
  claim depends on, the answer is "Insufficient", not "Supports".
- "Insufficient" is a perfectly good answer and is expected to be common.
- Quote the specific excerpts that decided it.

Weigh comments by whether the repository could check them:

- A comment asserting something the repository itself can settle — which
  callers exist, what they pass, whether a guard runs first, whether a
  constructor rejects a value — is a CLAIM, not evidence. It is exactly the
  kind of thing that is wrong when the code is wrong. "Callers check first"
  written above a function tells you what the author believed, not what the
  callers do. Go and look at the call sites; if the evidence asserts something
  about callers but contains no callers, that is "Insufficient", never
  "Contradicts". A documented precondition does not make violating it
  acceptable — the question is whether anything violates it.

- A comment stating a fact from outside the repository — a database column
  type, a wire-protocol constant, the size of production inputs, what a
  downstream consumer requires — is different. Nothing in the repository can
  confirm or refute it, and the people who wrote it knew things you do not.
  Treat it as the best available evidence and reason from it. Do not dismiss a
  finding merely because such a fact came from a comment rather than from
  code; that standard would make every claim about the real world
  unverifiable.

A true statement is not automatically a defect:

- You are judging the finding, not the sentence. Many claims are accurate
  descriptions of the code that identify nothing wrong with it: a type that
  does not derive a trait nobody needs, a function returning `Option` that
  never returns `None`, a name someone dislikes. Confirming the description is
  accurate does not make any of these a defect.
- Ask what goes wrong, and for whom. If the evidence does not show incorrect
  behaviour, a crash, data loss, a violated contract, a security consequence,
  or a real cost at the scale the code actually runs at, then this is not a
  defect and the answer is "Contradicts" — however accurate the claim is.
- Reporting a true triviality wastes human attention just as surely as
  reporting something false, so hold both to the same bar.

Reachability is part of the claim, not a separate question:

- Most claims depend on the code reaching some state — a collection being
  empty, a key being absent, a number being out of range, a caller passing a
  particular value. Confirming that the code WOULD misbehave in that state
  settles only half of it.
- Code that would misbehave in a state it can never actually be in is not a
  defect. If the evidence shows the state is prevented — by a constructor, a
  guard, a type, or an invariant every mutation preserves — then the evidence
  CONTRADICTS the claim, however faithfully the mechanism was described.
- So do not answer "Supports" merely because the described mechanism is real.
  Answer "Supports" only when the evidence also shows the triggering state is
  reachable. If the evidence settles the mechanism but leaves reachability
  open, that is "Insufficient".

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
        for s in [baseline_schema(), advanced_schema()] {
            for t in IssueType::ALL {
                assert!(s.contains(t.as_str()), "schema omits {t}");
            }
        }
    }

    #[test]
    fn baseline_and_advanced_share_the_output_contract() {
        // Fair comparison requires an identical JSON shape and an identical
        // taxonomy, so both arms are parsed and scored the same way. Only the
        // closing guidance differs, and that difference is the design under
        // test rather than a resource advantage.
        let shape = [
            "\"issue_type\"",
            "\"severity\"",
            "\"file\"",
            "\"start_line\"",
            "\"end_line\"",
            "\"claim\"",
            "\"reasoning\"",
        ];
        for prompt in [baseline_system("Rust"), advanced_system("Rust")] {
            for field in shape {
                assert!(prompt.contains(field), "missing {field} from the contract");
            }
            for t in IssueType::ALL {
                assert!(prompt.contains(t.as_str()), "missing issue type {t}");
            }
        }
    }

    #[test]
    fn only_the_baseline_is_told_that_silence_is_correct() {
        assert!(baseline_system("Rust").contains(
            "An empty
result is a correct answer"
        ));
        assert!(!advanced_system("Rust").contains("is a correct answer"));
        assert!(advanced_system("Rust").contains("Under-proposing is the expensive mistake"));
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
    fn review_prompts_contain_no_benchmark_specific_tells() {
        // v1 of the baseline prompt named `unwrap()` on a non-None value and
        // an in-bounds index as examples of correct code. Those are almost
        // verbatim descriptions of benchmark cases c03 and c02, so the prompt
        // was coaching the reviewer past the exact situations under test.
        // Guidance must stay general.
        let tells = [
            "unwrap",
            "index",
            "out of bounds",
            "deadlock",
            "shard",
            "session",
            "counter",
            "heartbeat",
            "pool",
        ];
        for prompt in [baseline_system("Rust"), advanced_system("Rust")] {
            let lower = prompt.to_lowercase();
            for tell in tells {
                assert!(
                    !lower.contains(tell),
                    "review prompt leaks a case-specific tell: {tell:?}"
                );
            }
        }
    }

    #[test]
    fn the_verifier_judges_defects_not_sentences() {
        // A verifier that only checks whether a claim is accurate confirms
        // true trivialities, which cost a reviewer just as much attention as
        // false ones. Observed on a real run: "SizeReport does not derive
        // Clone" was verified because it is, in fact, true.
        let s = verify_system();
        assert!(s.contains("A true statement is not automatically a defect"));
        assert!(s.contains("what goes wrong, and for whom"));
    }

    #[test]
    fn candidates_cover_calls_whose_definition_is_not_visible() {
        // Measured cause of the only unstable case: the reviewer proposed
        // *nothing* on c12 in 2 of 3 trials, so there was nothing to
        // investigate. The guard there calls a method defined in an untouched
        // file. The rule is general - name the assumption you are making about
        // code you cannot see - and mentions no benchmark noun.
        let s = advanced_system("Rust");
        assert!(s.contains("whose definition you"));
        assert!(s.contains("A guard is only as good as"));
    }

    #[test]
    fn candidates_must_name_a_consequence() {
        let s = advanced_system("Rust");
        assert!(s.contains("must name a consequence"));
    }

    #[test]
    fn the_verifier_distinguishes_checkable_comments_from_external_facts() {
        // Two real runs pinned both halves of this rule. First, the verifier
        // rejected a genuine reachable panic because the function's own doc
        // comment asserted that callers check first — the comment was wrong.
        // Then, told to distrust comments, it rejected two genuine defects
        // because the facts they rested on (a VARCHAR(64) column, production
        // batch sizes) were also stated in comments. The repository can settle
        // the first kind and cannot settle the second.
        let s = verify_system();
        assert!(s.contains("Weigh comments by whether the repository could check them"));
        assert!(s.contains("Go and look at the call sites"));
        assert!(s.contains("A comment stating a fact from outside the repository"));
        assert!(s.contains("best available evidence"));
    }

    #[test]
    fn the_falsification_question_targets_usage_not_mechanism() {
        let s = falsify_system();
        assert!(s.contains("ask about the call sites by") || s.contains("call sites by name"));
        assert!(s.contains("Never make a comment the answer"));
    }

    #[test]
    fn the_reviewer_addresses_itself_in_the_case_language() {
        assert!(baseline_system("Python").contains("experienced Python reviewer"));
        assert!(advanced_system("Python").contains("experienced Python reviewer"));
        assert!(baseline_system("Rust").contains("experienced Rust reviewer"));
    }

    #[test]
    fn the_verifier_is_language_neutral() {
        // The verifier reasons about claims and evidence, not syntax, so it
        // needs no language and must not assume one.
        let s = verify_system();
        assert!(!s.contains("Rust"));
        assert!(!s.contains("Python"));
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
