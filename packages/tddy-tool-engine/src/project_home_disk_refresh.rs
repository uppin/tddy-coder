//! `RefreshWorktreeDiskUsage`: the Worktrees screen's Refresh, for an orchestrator agent
//! (`#start-parity` 8/9).
//!
//! It queues the same background size walk the screen's Refresh queues and returns at once. The
//! size calculator keeps each worktree's state (`none`, `calculating`, `cached`) and persists the
//! sizes, so progress is read where the screen reads it: `ListWorktrees`' `sizing {done, total}`
//! (the screen's "{done} of {total}") and each row's `size_status`. No ledger of its own.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::catalog::ToolDef;
use crate::project_home_tools::{target_host_property, tool_def, TARGET_DAEMON_INSTANCE_ID};
use crate::ToolOutcome;

pub const REFRESH_WORKTREE_DISK_USAGE: &str = "RefreshWorktreeDiskUsage";

/// What a refresh answers at once: how many worktrees it queued a size walk for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshQueued {
    pub queued: usize,
}

/// The tool's definition.
pub fn refresh_worktree_disk_usage_tool_def() -> Option<ToolDef> {
    Some(tool_def(
        REFRESH_WORKTREE_DISK_USAGE,
        "Refresh the disk size of a worktree of this project, as the Worktrees screen's Refresh does, and return at once with how many were queued. With a path, only that worktree is recalculated; without one, every worktree of the project is. A path not in the project's worktree list is refused. Read the progress with ListWorktrees: its sizing {done, total} is the screen's \"{done} of {total}\", and each row's size_status reads calculating until its size is cached.",
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The worktree's path, as ListWorktrees lists it; empty or absent for every worktree of the project."
                },
                TARGET_DAEMON_INSTANCE_ID: target_host_property()
            }
        }),
    ))
}

/// The arguments of `RefreshWorktreeDiskUsage`.
#[derive(Deserialize)]
struct RefreshArgs {
    #[serde(default)]
    path: Option<String>,
}

/// Queue the refresh the arguments ask for through the port, and answer how many were queued.
pub(crate) async fn refresh_worktree_disk_usage(
    port: &dyn crate::project_home_tools::ProjectHomePort,
    home: &tddy_worktree_service::session_target::HomeCaller,
    args: serde_json::Value,
) -> ToolOutcome {
    let args: RefreshArgs = match serde_json::from_value(args) {
        Ok(args) => args,
        Err(e) => {
            return ToolOutcome::err(format!(
                "{REFRESH_WORKTREE_DISK_USAGE}: invalid arguments: {e}"
            ));
        }
    };
    let path = args
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    match port
        .refresh_worktree_disk_usage(&home.project_id, path)
        .await
    {
        Ok(queued) => match serde_json::to_string(&queued) {
            Ok(json) => ToolOutcome::ok(json),
            Err(e) => ToolOutcome::err(format!("{REFRESH_WORKTREE_DISK_USAGE}: {e}")),
        },
        Err(e) => ToolOutcome::err(format!("{REFRESH_WORKTREE_DISK_USAGE}: {e:#}")),
    }
}
