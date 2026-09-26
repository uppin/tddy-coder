use tddy_rpc::Status;

use std::path::PathBuf;

pub fn agent_clone_worktree_path(
    session_id: &str,
    agent_id: &str,
    clone: crate::session_agent_clone::AgentClone,
) -> Result<PathBuf, Status> {
    clone.worktree_path.ok_or_else(|| {
        Status::failed_precondition(format!(
            "the daemon owning '{agent_id}' has not reported where session {session_id}'s \
                 clone landed (its state is {:?})",
            clone.state
        ))
    })
}
