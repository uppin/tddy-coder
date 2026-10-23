//! The read-only rule of a project-home session, enforced where exec tools run (`#project-home` 4/7).
//!
//! A session whose recorded role is `project-home` may read and search its home worktree and nothing
//! else. The rule is applied here, on the daemon, rather than left to the agent's spawn-time
//! allowlist, which is harness-specific and which a harness that cannot enforce it ignores.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session.md.

use std::path::Path;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_rpc::Status;
use tddy_tool_engine::read_only::is_read_only_tool;

/// Refuse `tool_name` with `PermissionDenied` when `session_id` is a project-home session and the
/// tool is not a read-only one. Any other session, and one whose metadata cannot be read, is left to
/// the ordinary path (which refuses an unknown session itself).
pub(super) fn reject_non_read_only_tool_for_project_home(
    sessions_base: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<(), Status> {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let Ok(metadata) = tddy_core::read_session_metadata(&session_dir) else {
        return Ok(());
    };
    if metadata.session_role.as_deref() == Some(PROJECT_HOME_ROLE) && !is_read_only_tool(tool_name)
    {
        return Err(Status::permission_denied(format!(
            "tool {tool_name} is refused: a project-home session is read-only"
        )));
    }
    Ok(())
}
