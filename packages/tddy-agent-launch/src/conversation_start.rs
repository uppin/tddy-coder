//! A start an orchestrator asks for, begun on this daemon's streamed start path and recorded in the
//! orchestrator's start ledger — shared by the project-home exec tools and the conversation-spawn
//! handler, so an agent that spawns a conversation gets the very start `StartSession` gives
//! (`#start-parity` 9/9).

use async_trait::async_trait;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ListSessionsRequest, ListSessionsResponse, SessionEntry, StartSessionRequest,
};
use tddy_service::proto::tasks::CancelTaskRequest;
use tddy_tool_engine::start_ledger::{
    begin_recorded_start_with, StartHandle, StartLedger, StartState, StartedSessionDetails,
    StartedSessionListing,
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

/// A started session's branch, worktree and name, read off the listing of the host that runs it.
struct StartedSessionListings {
    launch: LaunchSessions,
    session_token: String,
}

#[async_trait]
impl ListedSessions for StartedSessionListings {
    async fn listed_session(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
    ) -> Option<SessionEntry> {
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
    }
}

#[async_trait]
impl StartedSessionDetails for StartedSessionListings {
    async fn listing(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
    ) -> Option<StartedSessionListing> {
        reported_listing(self, daemon_instance_id, session_id).await
    }
}

/// A host's listing of one session, as the start ledger reads it.
#[async_trait]
trait ListedSessions: Send + Sync {
    /// `session_id` as `daemon_instance_id` lists it, or `None` when it does not (or cannot).
    async fn listed_session(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
    ) -> Option<SessionEntry>;
}

/// What a start reports for `session_id` on `daemon_instance_id`: its branch, worktree and name as
/// that host lists it — or, for a split session's agent half, which holds no checkout, the branch and
/// worktree its codebase half is listed with on the codebase host. The name is the agent half's own.
async fn reported_listing(
    listings: &dyn ListedSessions,
    daemon_instance_id: &str,
    session_id: &str,
) -> Option<StartedSessionListing> {
    let listed = listings
        .listed_session(daemon_instance_id, session_id)
        .await?;
    let display_name = listed.display_name;
    let (branch, worktree) =
        if listed.codebase_daemon_instance_id.is_empty() || listed.codebase_session_id.is_empty() {
            (listed.branch, listed.repo_path)
        } else {
            let codebase = listings
                .listed_session(
                    &listed.codebase_daemon_instance_id,
                    &listed.codebase_session_id,
                )
                .await?;
            (codebase.branch, codebase.repo_path)
        };
    Some(StartedSessionListing {
        branch,
        worktree,
        display_name,
    })
}

#[cfg(test)]
mod reported_listing_tests {
    use super::*;

    async fn branch_and_worktree(
        hosts: &dyn ListedSessions,
        host: &str,
        session_id: &str,
    ) -> Option<(String, String)> {
        reported_listing(hosts, host, session_id)
            .await
            .map(|l| (l.branch, l.worktree))
    }
    use std::collections::HashMap;

    /// Hosts that list the sessions they were given, keyed by (host, session id).
    struct Hosts(HashMap<(String, String), SessionEntry>);

    #[async_trait]
    impl ListedSessions for Hosts {
        async fn listed_session(&self, host: &str, session_id: &str) -> Option<SessionEntry> {
            self.0
                .get(&(host.to_string(), session_id.to_string()))
                .cloned()
        }
    }

    fn listed(host: &str, entry: SessionEntry) -> ((String, String), SessionEntry) {
        ((host.to_string(), entry.session_id.clone()), entry)
    }

    #[tokio::test]
    async fn a_split_agent_is_reported_with_its_codebase_halfs_branch_and_worktree() {
        // Given a split session: its agent half on the mac holds no checkout, its codebase half on
        // udoo does
        let hosts = Hosts(HashMap::from([
            listed(
                "mac",
                SessionEntry {
                    session_id: "agent-half".to_string(),
                    codebase_daemon_instance_id: "udoo".to_string(),
                    codebase_session_id: "codebase-half".to_string(),
                    ..Default::default()
                },
            ),
            listed(
                "udoo",
                SessionEntry {
                    session_id: "codebase-half".to_string(),
                    branch: "test-split-udoo-4".to_string(),
                    repo_path: "/home/mantas/Code/tddy-coder/.worktrees/test-split-udoo-4"
                        .to_string(),
                    ..Default::default()
                },
            ),
        ]));

        // When the start reports the agent half's branch and worktree
        let reported = branch_and_worktree(&hosts, "mac", "agent-half").await;

        // Then they are the codebase half's
        assert_eq!(
            reported,
            Some((
                "test-split-udoo-4".to_string(),
                "/home/mantas/Code/tddy-coder/.worktrees/test-split-udoo-4".to_string()
            ))
        );
    }

    #[tokio::test]
    async fn a_session_with_no_codebase_half_is_reported_as_listed() {
        // Given a co-located session
        let hosts = Hosts(HashMap::from([listed(
            "mac",
            SessionEntry {
                session_id: "s1".to_string(),
                branch: "feature/s1".to_string(),
                repo_path: "/worktrees/s1".to_string(),
                ..Default::default()
            },
        )]));

        // When its start reports its branch and worktree
        let reported = branch_and_worktree(&hosts, "mac", "s1").await;

        // Then they are its own
        assert_eq!(
            reported,
            Some(("feature/s1".to_string(), "/worktrees/s1".to_string()))
        );
    }

    #[tokio::test]
    async fn a_split_agent_whose_codebase_host_does_not_list_its_half_reports_nothing() {
        // Given a split agent half whose codebase host no longer lists the codebase half
        let hosts = Hosts(HashMap::from([listed(
            "mac",
            SessionEntry {
                session_id: "agent-half".to_string(),
                codebase_daemon_instance_id: "udoo".to_string(),
                codebase_session_id: "codebase-half".to_string(),
                ..Default::default()
            },
        )]));

        // When the start reports its branch and worktree
        let reported = branch_and_worktree(&hosts, "mac", "agent-half").await;

        // Then it reports none rather than the agent half's empty ones
        assert_eq!(reported, None);
    }
}
