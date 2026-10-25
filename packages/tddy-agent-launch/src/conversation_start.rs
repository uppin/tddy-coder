//! A start an orchestrator asks for, begun on this daemon's streamed start path and recorded in the
//! orchestrator's start ledger — shared by the project-home exec tools and the conversation-spawn
//! handler, so an agent that spawns a conversation gets the very start `StartSession` gives
//! (`#start-parity` 9/9).

use async_trait::async_trait;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ListSessionsRequest, ListSessionsResponse, StartSessionRequest,
};
use tddy_service::proto::tasks::CancelTaskRequest;
use tddy_tool_engine::start_ledger::{
    begin_recorded_start_with, StartHandle, StartLedger, StartState, StartedSessionDetails,
};

use crate::launch_ports::LaunchSessions;

const SESSION_SERVICE: &str = "session.SessionService";

/// The `CancelTask` RPC a start's cancel is forwarded as, to the host that runs it.
const CANCEL_TASK_RPC: &str = "CancelTask";

fn status_error(status: tddy_rpc::Status) -> anyhow::Error {
    anyhow::anyhow!(status.message().to_string())
}

fn state_name(state: StartState) -> &'static str {
    match state {
        StartState::Running => "running",
        StartState::Succeeded => "succeeded",
        StartState::Failed => "failed",
        StartState::Cancelled => "been cancelled",
    }
}

impl LaunchSessions {
    /// Begin `request` on the streamed start path — which forwards it to the host it names — and
    /// record it in `ledger`, the start ledger of the orchestrator the request is linked to.
    /// Answers with the handle once the start has named its task.
    ///
    /// A start the caller could be told is refused (a CLI session without a model, an orchestrated
    /// `tool` session, a branch another session owns) is refused here, in place of a handle.
    pub async fn begin_recorded_start(
        &self,
        request: StartSessionRequest,
        os_user: &str,
        ledger: StartLedger,
    ) -> anyhow::Result<StartHandle> {
        let session_token = request.session_token.clone();
        let conflict = self
            .refuse_a_start_before_it_begins(&request, os_user)
            .await
            .map_err(status_error)?;
        if let Some(conflict) = conflict {
            let owner = conflict
                .owner
                .map(|owner| format!(" by session {}", owner.session_id))
                .unwrap_or_default();
            anyhow::bail!(
                "the session was not started: branch {} is owned{owner}",
                conflict.branch
            );
        }
        let events = self
            .stream_start_session_at_session_coordinate(Request::direct(request))
            .await
            .map_err(status_error)?
            .into_inner();
        let details: std::sync::Arc<dyn StartedSessionDetails> =
            std::sync::Arc::new(StartedSessionListings {
                launch: self.clone(),
                session_token,
            });
        begin_recorded_start_with(ledger, events, Some(details))
            .await
            .map_err(anyhow::Error::msg)
    }

    /// Stop the start `start_id` of `ledger` on the host its task runs on: this host's task
    /// registry, or the peer's `CancelTask`. The record reads cancelled once the start's stream has
    /// ended.
    pub async fn cancel_recorded_start(
        &self,
        ledger: &StartLedger,
        start_id: &str,
        session_token: &str,
    ) -> anyhow::Result<()> {
        let record = ledger.read(start_id)?;
        if record.state != StartState::Running {
            anyhow::bail!(
                "start {start_id} is not running: it has already {}",
                state_name(record.state)
            );
        }
        if record.daemon_instance_id == local_instance_id_for_config(&self.config) {
            let found = self.cancel_local_task(&record.task_id).await;
            anyhow::ensure!(
                found,
                "start {start_id}'s task is no longer running on {}",
                record.daemon_instance_id
            );
            return Ok(());
        }
        let room = self
            .peer_routing
            .common_room_slot(CANCEL_TASK_RPC)
            .map_err(status_error)?;
        let answer = tddy_daemon_kernel::peer_calls::forward_cancel_task(
            room,
            &record.daemon_instance_id,
            &CancelTaskRequest {
                session_token: session_token.to_string(),
                task_id: record.task_id.clone(),
                daemon_instance_id: record.daemon_instance_id.clone(),
            },
        )
        .await
        .map_err(status_error)?;
        anyhow::ensure!(
            answer.ok,
            "start {start_id} could not be cancelled on {}: {}",
            record.daemon_instance_id,
            answer.message
        );
        Ok(())
    }
}

/// A started session's branch and worktree, read off the listing of the host that runs it.
struct StartedSessionListings {
    launch: LaunchSessions,
    session_token: String,
}

#[async_trait]
impl StartedSessionDetails for StartedSessionListings {
    async fn branch_and_worktree(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
    ) -> Option<(String, String)> {
        let request = ListSessionsRequest {
            session_token: self.session_token.clone(),
        };
        let listed = match self
            .launch
            .peer_routing
            .rpc_served_by_peer::<_, ListSessionsResponse>(
                SESSION_SERVICE,
                "ListSessions",
                daemon_instance_id,
                &request,
            )
            .await
        {
            Ok(Some(listed)) => listed,
            // Not a peer's: this host's own listing.
            Ok(None) => match self
                .launch
                .list_sessions_at_session_coordinate(Request::direct(request))
                .await
            {
                Ok(listed) => listed.into_inner(),
                Err(status) => {
                    log::warn!("start of session {session_id}: this host's sessions could not be listed: {status}");
                    return None;
                }
            },
            Err(status) => {
                log::warn!(
                    "start of session {session_id}: host {daemon_instance_id} could not list its sessions: {status}"
                );
                return None;
            }
        };
        listed
            .sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .map(|s| (s.branch, s.repo_path))
    }
}
