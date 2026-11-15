//! `RenameSession`: what the web's rename does — record a name for a session of this project in
//! its `.session.yaml`, or clear it — for an orchestrator agent. The name is the session's label
//! from then on: its drawer row, its Telegram alerts, and what `ListProjectSessions` and
//! `ReadSessionActivity` report as `display_name`.
//!
//! A target tool, like the rest of its family: a call naming another host is forwarded to that host
//! whole. A running session is renamed like a stopped one — a name changes nothing the session does.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde_json::Value;
use tddy_worktree_service::session_target::HomeCaller;

use super::ProjectHomeLifecyclePort;
use crate::catalog::ToolDef;
use crate::ToolOutcome;

pub const RENAME_SESSION: &str = "RenameSession";

/// The tool's definition.
pub fn rename_session_tool_def() -> ToolDef {
    // TODO(session-display-name): green phase.
    todo!("rename_session_tool_def")
}

/// Rename the session `args` name, in `home`'s project.
pub(super) async fn rename_session(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    // TODO(session-display-name): green phase.
    let _ = (port, home, args);
    todo!("RenameSession")
}
