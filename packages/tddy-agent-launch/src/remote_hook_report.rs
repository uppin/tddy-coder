//! Which of a target session's hook lines report a session status.

use crate::remote_hook_follower::HookLine;

/// Whether `line` maps to a status report.
///
/// A `PreToolUse` line without an answer nonce is the catch-all hook, which only reports the tool
/// call as agent activity: a question tool is recorded by the waiting hook, and any other call has
/// no status of its own (`#target-parity` 4/7). It is the same rule `tddy-tools session-hook`
/// applies to a `PreToolUse` that does not wait.
pub(crate) fn reports_a_status(line: &HookLine) -> bool {
    line.event != "PreToolUse" || line.answer_nonce.is_some()
}
