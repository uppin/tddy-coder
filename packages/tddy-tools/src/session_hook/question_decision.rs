//! What `session-hook --await-answer` prints when an orchestrator answered the `AskUserQuestion` a
//! `PreToolUse` hook is holding (`#home-inbox` 8/9).
//!
//! The encoding moved to `tddy_session_activity::hook_decision` (`#target-parity` 6/7): a target
//! session's hook-log follower prints the same decision, and only daemon-side crates reach a
//! target's shell. The mechanism's notes live there.

pub use tddy_session_activity::hook_decision::encode_question_decision;
