use tddy_rpc::Status;

use std::path::PathBuf;

pub fn agent_clone_for(
    session_id: &str,
    agent_id: &str,
    session_dir: PathBuf,
    state: crate::AgentRosterState<'_>,
) -> Result<crate::session_agent_clone::AgentClone, Status> {
    let record = state
        .session_agent_rosters
        .entry(session_id, &session_dir, agent_id)?
        .ok_or_else(|| {
            Status::not_found(format!(
                "agent '{agent_id}' is not attached to session '{session_id}'"
            ))
        })?;
    state
        .session_agent_clones
        .get(session_id, &record.daemon_instance_id)
        .ok_or_else(|| {
            Status::failed_precondition(format!(
                "agent '{agent_id}' is served locally by daemon \
                     '{}', which works the session's own worktree and has no clone",
                record.daemon_instance_id
            ))
        })
}
