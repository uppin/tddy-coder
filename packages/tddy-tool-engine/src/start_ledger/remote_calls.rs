//! The start's log of what it ran on its target (`#coder-start` 8/10): the record's
//! `remote_calls`, folded from the start's `remote_call` events.

use serde::{Deserialize, Serialize};
use tddy_service::proto::session::start_remote_call::Outcome as StartCallOutcome;
use tddy_service::proto::session::StartRemoteCall;

/// The most remote calls one start's record keeps; past it the oldest are dropped.
pub const MAX_REMOTE_CALLS: usize = 50;

/// How a remote call of a start ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCallOutcome {
    /// The command ran and exited; `exit_code` says with what.
    Exited,
    /// The command was still running at its deadline and was stopped.
    TimedOut,
    /// The transport could not reach the target; nothing ran.
    Unreachable,
}

/// One command a start ran on its target, as `GetSessionStart` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCallRecord {
    /// The target, as its transport names it.
    pub host: String,
    /// The caller's command, truncated; never a credential.
    pub command: String,
    pub duration_ms: u64,
    pub outcome: RemoteCallOutcome,
    /// Only for [`RemoteCallOutcome::Exited`].
    pub exit_code: Option<i32>,
    /// The tail of the command's stderr for a call that failed; empty otherwise.
    pub stderr_tail: String,
}

/// A reported call as the record keeps it. The reporter already scrubbed it; this scrubs again, so a
/// start relayed from a daemon that did not still leaves no credential on record.
pub(super) fn remote_call_record(call: StartRemoteCall) -> RemoteCallRecord {
    let outcome = match StartCallOutcome::try_from(call.outcome) {
        Ok(StartCallOutcome::TimedOut) => RemoteCallOutcome::TimedOut,
        Ok(StartCallOutcome::Unreachable) => RemoteCallOutcome::Unreachable,
        _ => RemoteCallOutcome::Exited,
    };
    RemoteCallRecord {
        host: call.host,
        command: tddy_remote_shell::command_head(&call.command),
        duration_ms: call.duration_ms,
        outcome,
        exit_code: (outcome == RemoteCallOutcome::Exited).then_some(call.exit_code),
        stderr_tail: tddy_remote_shell::scrub_credentials(&call.stderr_tail),
    }
}
