//! `TerminateSession`: what the web UI's Terminate does — `SessionService.SignalSession` with
//! SIGTERM to the session's process on its host — for an orchestrator agent (`#start-parity` 7/9).
//!
//! Terminate stops the process and nothing else. The session's record, its worktree and its branch
//! stay, so a terminated session can be resumed; deleting them is the UI's separate Delete.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use crate::catalog::ToolDef;

pub const TERMINATE_SESSION: &str = "TerminateSession";

/// The tool's definition.
pub fn terminate_session_tool_def() -> Option<ToolDef> {
    // TODO(terminate): implement
    None
}
