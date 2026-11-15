//! `RenameSession`: what the web's rename does — record a name for a session of this project in
//! its `.session.yaml`, or clear it — for an orchestrator agent. The name is the session's label
//! from then on: its drawer row, its Telegram alerts, and what `ListProjectSessions` and
//! `ReadSessionActivity` report as `display_name`.
//!
//! A target tool, like the rest of its family: a call naming another host is forwarded to that host
//! whole. A running session is renamed like a stopped one — a name changes nothing the session does.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::Deserialize;
use serde_json::{json, Value};
use tddy_core::session_display_name::normalize_display_name;
use tddy_worktree_service::session_target::HomeCaller;

use super::ProjectHomeLifecyclePort;
use crate::catalog::ToolDef;
use crate::project_home_tools::{
    resolve_target, target_host_property, tool_def, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

pub const RENAME_SESSION: &str = "RenameSession";

/// The tool's definition.
pub fn rename_session_tool_def() -> ToolDef {
    tool_def(
        RENAME_SESSION,
        "Name a session of this project, as the web UI's rename does, or clear its name with an empty display_name. The name is recorded in the session itself and is what the operator sees it called from then on: its row in the web UI and its Telegram alerts. ListProjectSessions and ReadSessionActivity report it as display_name. One line, at most 80 characters; surrounding whitespace is trimmed. A running session keeps running. Answers session_id and display_name (null when cleared).",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session of this project, as ListProjectSessions reports it."
                },
                "display_name": {
                    "type": "string",
                    "description": "The session's new name; empty clears it, so it is called by its worktree or goal again."
                },
                TARGET_DAEMON_INSTANCE_ID: target_host_property()
            },
            "required": ["session_id", "display_name"]
        }),
    )
}

#[derive(Deserialize)]
struct RenameArgs {
    session_id: String,
    display_name: String,
}

/// Rename the session `args` name, in `home`'s project. The name is held to the record's rules
/// before the target is resolved or the port asked, so a refused name changes nothing.
pub(super) async fn rename_session(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: RenameArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{RENAME_SESSION}: invalid arguments: {e}")),
    };
    let display_name = match normalize_display_name(&args.display_name) {
        Ok(name) => name,
        Err(e) => return ToolOutcome::err(format!("{RENAME_SESSION}: {e}")),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    if let Err(e) = port.rename_session(&target, display_name.as_deref()).await {
        return ToolOutcome::err(format!("{RENAME_SESSION}: {e}"));
    }
    ToolOutcome::ok(
        json!({ "session_id": target.session_id, "display_name": display_name }).to_string(),
    )
}
