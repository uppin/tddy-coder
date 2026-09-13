//! Where a clone's checkout is on this host.
//!
//! The clone domain itself — the facilitating daemon's [`SessionAgentCloneStore`], the owning
//! daemon's [`HostedAgentClones`] and the mirror it runs against the session's room — moved to
//! [`tddy_session_agents::session_agent_clone`] with `#unbundle` node 7, and is re-exported below
//! so a call site names one module rather than two.
//!
//! What stayed is [`clone_worktree_path`], and it stayed because it resolves the checkout through
//! [`crate::workspace_session`], which reads the `workspace` session's own `.session.yaml` — that
//! is session lifecycle, which this node's changeset keeps in the daemon deliberately.
//!
//! **It is currently unreferenced.** Nothing in the workspace calls it, and nothing did before
//! `#unbundle` node 7 either — the similarly named
//! [`crate::connection_service::DaemonSessionHost::agent_clone_worktree_path`] is a different
//! function and is what the acceptance suites drive. So this module is not what kept the clone
//! domain out of `tddy-session-agents`; the re-export above is the whole of its current use. Left
//! in place rather than removed, because pre-existing dead code is not a move's to delete.

use std::path::{Path, PathBuf};

use tddy_rpc::Status;

pub use tddy_session_agents::session_agent_clone::*;

/// Where a clone's checkout is, given the `workspace` session that holds it.
pub fn clone_worktree_path(
    sessions_base: &Path,
    codebase_session_id: &str,
) -> Result<PathBuf, Status> {
    crate::workspace_session::resolve_worktree_root_for_session(sessions_base, codebase_session_id)
}
