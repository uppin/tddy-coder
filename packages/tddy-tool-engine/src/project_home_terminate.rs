//! `TerminateSession`: what the web UI's Terminate does — `SessionService.SignalSession` with
//! SIGTERM to the session's process on its host — for an orchestrator agent (`#start-parity` 7/9).
//!
//! Terminate stops the process and nothing else. The session's record, its worktree and its branch
//! stay, so a terminated session can be resumed; deleting them is the UI's separate Delete.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde_json::json;

use crate::catalog::ToolDef;
use crate::project_home_tools::{target_host_property, tool_def, TARGET_DAEMON_INSTANCE_ID};
use crate::ToolOutcome;

pub const TERMINATE_SESSION: &str = "TerminateSession";

/// The tool's definition.
pub fn terminate_session_tool_def() -> Option<ToolDef> {
    Some(tool_def(
        TERMINATE_SESSION,
        "Terminate a session of this project, as the web UI's Terminate does: its process gets SIGTERM. The session's record, worktree and branch are kept, so a terminated session can be resumed. Terminating a session whose process has already ended is an error. Only sessions of this project can be terminated.",
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                TARGET_DAEMON_INSTANCE_ID: target_host_property()
            },
            "required": ["session_id"]
        }),
    ))
}

/// Send SIGTERM to the target session's process. The target is already resolved and checked to be
/// in the caller's project.
pub(crate) async fn terminate_session(
    port: &dyn crate::project_home_tools::ProjectHomePort,
    home: &tddy_worktree_service::session_target::HomeCaller,
    args: serde_json::Value,
) -> ToolOutcome {
    let args: TerminateArgs = match serde_json::from_value(args) {
        Ok(args) => args,
        Err(e) => {
            return ToolOutcome::err(format!("{TERMINATE_SESSION}: invalid arguments: {e}"));
        }
    };
    let target = match crate::project_home_tools::resolve_target(port, home, &args.session_id).await
    {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    match port.terminate_session(&target).await {
        Ok(()) => ToolOutcome::ok(
            json!({ "session_id": target.session_id, "signal": "SIGTERM" }).to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{TERMINATE_SESSION}: {e:#}")),
    }
}

#[derive(serde::Deserialize)]
struct TerminateArgs {
    session_id: String,
}
