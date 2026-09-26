use tddy_service::proto::session_agents_svc::AgentCloneState;

use tddy_rpc::Status;

pub fn refuse_unready_clone(
    session_id: &str,
    record: &tddy_core::SessionAgentRecord,
    clone: Option<crate::session_agent_clone::AgentClone>,
) -> Result<(), Status> {
    let (state, error) = match clone {
        Some(clone) => (clone.state, clone.error),
        None => (AgentCloneState::Unspecified, String::new()),
    };
    match state {
        AgentCloneState::Ready | AgentCloneState::Local => Ok(()),
        AgentCloneState::Provisioning => Err(Status::failed_precondition(format!(
            "agent '{}' cannot be prompted yet: its clone on daemon '{}' is still \
                 provisioning",
            record.agent_id, record.daemon_instance_id
        ))),
        AgentCloneState::Error => Err(Status::failed_precondition(format!(
            "agent '{}' cannot be prompted: its clone on daemon '{}' is in the error state \
                 ({error})",
            record.agent_id, record.daemon_instance_id
        ))),
        AgentCloneState::Unspecified => Err(Status::failed_precondition(format!(
            "agent '{}' cannot be prompted: this daemon has no clone on daemon '{}' for \
                 session '{session_id}' — the state is unknown, which is not the same as ready",
            record.agent_id, record.daemon_instance_id
        ))),
    }
}
