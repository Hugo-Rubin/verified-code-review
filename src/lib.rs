//! Verified Code Reviewer.
//!
//! A Rust code reviewer that investigates its own candidate findings against
//! the repository and then falsifies them in a fresh reasoning context before
//! deciding whether to report them.
//!
//! The crate is organised around one rule: the LLM reasons, and Rust decides.
//! Evidence is produced by [`repo`] tools, scoring is done by [`eval`] with no
//! model in the loop, and a finding's [`finding::FindingStatus`] is assigned by
//! the orchestrator rather than claimed by the model.

pub mod agent;
pub mod bench;
pub mod config;
pub mod eval;
pub mod finding;
pub mod llm;
pub mod prompts;
pub mod repo;
pub mod runner;
pub mod tools;
pub mod trajectory;
