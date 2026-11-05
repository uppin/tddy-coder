//! The session host as a [`SessionStarter`]: a satellite's operator start reaches the host's one
//! generic start (`LaunchSessions::start_session_for_os_user`) as a `StartSessionRequest` that asks
//! to be rejected on a branch conflict, so the satellite can ask its operator how to continue
//! (`#agent-tools 18/19`).
//!
//! The daemon binds one of these into its `LateBoundSessionStarter` once the host is built; a
//! satellite built before the host holds the late-bound port, never the host.

use std::sync::Arc;

use tddy_daemon_extensions::{
    OperatorBranchChoice, OperatorBranchConflict, OperatorSessionStart, OperatorStartOutcome,
    SessionStarter,
};
use tddy_rpc::Status;
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_files::attachment_progress::AttachmentProgressSink;

use super::DaemonSessionHost;

/// The host's generic start, offered to a satellite.
pub struct HostSessionStarter {
    host: Arc<DaemonSessionHost>,
}

impl HostSessionStarter {
    pub fn new(host: Arc<DaemonSessionHost>) -> Self {
        Self { host }
    }
}

#[async_trait::async_trait]
impl SessionStarter for HostSessionStarter {
    async fn start_for_operator(
        &self,
        start: OperatorSessionStart,
    ) -> Result<OperatorStartOutcome, Status> {
        let request = operator_start_request(&start);
        let response = self
            .host
            .launch_sessions()
            .start_session_for_os_user(
                &start.os_user,
                request,
                &AttachmentProgressSink::discarding(),
            )
            .await?;
        Ok(operator_start_outcome(response.into_inner()))
    }
}

/// The `StartSessionRequest` `start` stands for: its project, session type, model, first prompt and
/// branch intent, `on_branch_conflict = "reject"`, and no session token — an operator start is keyed
/// by its OS user.
pub fn operator_start_request(start: &OperatorSessionStart) -> StartSessionRequest {
    let request = StartSessionRequest {
        project_id: start.project_id.clone(),
        session_type: start.session_type.clone(),
        model: start.model.clone(),
        initial_prompt: start.initial_prompt.clone(),
        on_branch_conflict: "reject".to_string(),
        ..Default::default()
    };
    match &start.branch {
        OperatorBranchChoice::NewBranchFromBase {
            new_branch_name,
            integration_base_ref,
        } => StartSessionRequest {
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: new_branch_name.clone(),
            selected_integration_base_ref: integration_base_ref.clone(),
            ..request
        },
        OperatorBranchChoice::WorkOnBranch { branch } => StartSessionRequest {
            branch_worktree_intent: "work_on_selected_branch".to_string(),
            selected_branch_to_work_on: branch.clone(),
            ..request
        },
    }
}

/// What the generic start's answer means to the operator: the started session, or the branch
/// conflict it reported instead of creating one.
pub fn operator_start_outcome(response: StartSessionResponse) -> OperatorStartOutcome {
    match response.branch_conflict {
        Some(conflict) => OperatorStartOutcome::BranchConflict(OperatorBranchConflict {
            branch: conflict.branch,
            owner_session_id: conflict
                .owner
                .map(|owner| owner.session_id)
                .unwrap_or_default(),
            suggested_branch_name: conflict.suggested_branch_name,
        }),
        None => OperatorStartOutcome::Started {
            session_id: response.session_id,
        },
    }
}
