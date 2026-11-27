//! `PreviewCloseSession` and `CloseSession` — the sessions drawer's × and trash bin: terminate every
//! agent tab of a session, then remove every worktree it owns, as one task
//! (docs/ft/daemon/session-liveness.md § Closing a session).
//!
//! Both are answered by the daemon holding the session, as `DeleteSession` is: a session this
//! daemon has no directory for is refused `FAILED_PRECONDITION`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::SessionMetadata;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    CloseSessionEvent, CloseSessionPreview, CloseSessionRequest, PreviewCloseSessionRequest,
};
use tddy_task::{ChannelKind, TaskChannel};
use tokio::sync::mpsc::UnboundedReceiver;

use super::DaemonSessionHost;

mod close_plan;
mod close_tabs;
mod close_task;
mod closes_in_flight;
// `pub` for the port seam the `fs_port` suite drives: everything else in the module stays
// private, so the newly reachable surface is `conversation_directory_names_on` alone.
pub mod conversation_worktrees;
mod worktree_reads;

pub(crate) use closes_in_flight::SessionCloses;

/// The id of a close task's one output channel, `progress`: a task with a single channel numbers it
/// `0`, as the LSP task does.
const PROGRESS_CHANNEL_ID: &str = "0";

/// The session a close or its preview acts on, and who asked.
pub(super) struct CloseContext {
    pub os_user: String,
    /// Who the caller's token resolved to: the account a target is reached with.
    pub subject: Option<String>,
    pub session_token: String,
    pub session_id: String,
    pub session_dir: PathBuf,
    pub metadata: SessionMetadata,
}

impl DaemonSessionHost {
    /// The conversations open on this host's sessions — which a close ends — for
    /// [`crate::test_util`] to open one.
    pub(crate) fn open_agent_conversations(
        &self,
    ) -> &Arc<tddy_session_agents::OpenAgentConversations> {
        &self.agent_conversations
    }

    /// What closing `request.session_id` would lose, per worktree it owns, and the tabs it would end.
    pub(crate) async fn preview_close_session_at_host(
        &self,
        request: Request<PreviewCloseSessionRequest>,
    ) -> Result<Response<CloseSessionPreview>, Status> {
        let req = request.into_inner();
        let ctx = self.close_context(&req.session_token, &req.session_id)?;
        Ok(Response::new(self.plan_close(&ctx).await.preview()))
    }

    /// Run the close as a `close_session` task, streaming its steps. A second close of a session
    /// whose close is still running is refused `ALREADY_EXISTS`, naming the running task.
    pub(crate) async fn close_session_at_host(
        &self,
        request: Request<CloseSessionRequest>,
    ) -> Result<Response<crate::CloseSessionEventStream>, Status> {
        let req = request.into_inner();
        let ctx = self.close_context(&req.session_token, &req.session_id)?;
        let reservation = self.session_closes.reserve(&ctx.session_id)?;
        let (events, received) = tokio::sync::mpsc::unbounded_channel();
        let progress =
            TaskChannel::output_only(PROGRESS_CHANNEL_ID, "progress", ChannelKind::Combined);
        let session_id = ctx.session_id.clone();
        self.task_registry
            .spawn(
                close_task::CloseTask {
                    host: self.clone(),
                    ctx,
                    confirmed: req.confirmed,
                    confirmed_fingerprint: req.confirmed_fingerprint,
                    events,
                    progress: Arc::clone(&progress),
                    daemon_instance_id:
                        tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(
                            &self.config,
                        ),
                    reservation,
                },
                close_task::CLOSE_TASK_KIND,
                session_id,
                vec![progress],
            )
            .await;
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::UnboundedReceiverStream::new(received),
        )))
    }

    /// The caller and the session a close request names, refused when the token names nobody this
    /// daemon serves, when the id is not one path segment, or when this daemon does not hold the
    /// session.
    fn close_context(&self, session_token: &str, session_id: &str) -> Result<CloseContext, Status> {
        let os_user = self.resolve_os_user(session_token)?;
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        let metadata = held_session(&session_dir)?;
        Ok(CloseContext {
            os_user,
            subject: (self.user_resolver)(session_token),
            session_token: session_token.to_string(),
            session_id: session_id.to_string(),
            session_dir,
            metadata,
        })
    }

    /// Force the session's own worktree on this host out of its project's clone (step 4 of a close:
    /// the checks before it proved nothing unconfirmed is lost). One already gone counts as removed;
    /// one `planned_present` false that is there again is refused, since what it holds was never
    /// shown.
    async fn remove_local_worktree(
        &self,
        ctx: &CloseContext,
        worktree: &Path,
        planned_present: bool,
    ) -> Result<String, String> {
        let projects_dir = tddy_daemon_kernel::user_paths::projects_path_for_user(
            &ctx.os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| "the projects of the session's user cannot be resolved".to_string())?;
        let project_id = ctx.metadata.project_id.clone();
        let project = tddy_projects::project_storage::find_project(&projects_dir, &project_id)
            .map_err(|e| format!("the projects cannot be read: {e:#}"))?
            .ok_or_else(|| {
                format!(
                    "the session's project {project_id} is not registered on this daemon, so the \
                     clone its worktree belongs to is unknown"
                )
            })?;
        let (repo, worktree) = (
            PathBuf::from(project.main_repo_path),
            worktree.to_path_buf(),
        );
        let there = tddy_worktree_service::worktrees::path_is_present(&worktree)
            .map_err(|error| error.to_string())?;
        if there && !planned_present {
            return Err(close_task::reappeared());
        }
        let already_gone = !there;
        tokio::task::spawn_blocking(move || {
            tddy_worktree_service::worktrees::force_remove_worktree_under_repo(&repo, &worktree)
        })
        .await
        .map_err(|e| format!("the removal did not finish: {e}"))?
        .map_err(|error| error.to_string())?;
        Ok(if already_gone {
            "already gone".to_string()
        } else {
            "removed".to_string()
        })
    }

    /// Forward `CloseSession` to `daemon_instance_id`, refusing — distinctly from "that daemon has
    /// no such session" — when this daemon is not in the common room right now.
    async fn forward_close(
        &self,
        daemon_instance_id: &str,
        request: &CloseSessionRequest,
    ) -> Result<UnboundedReceiver<Result<CloseSessionEvent, Status>>, Status> {
        let slot = self
            .peer_routing
            .common_room_slot("CloseSession")
            .map_err(|refusal| Status::unavailable(refusal.message().to_string()))?;
        if !slot.is_connected().await {
            return Err(Status::unavailable(format!(
                "cannot reach the common room to close the checkout on daemon \
                 {daemon_instance_id}; retry once the daemons can see each other"
            )));
        }
        tddy_daemon_kernel::peer_calls::forward_close_session(slot, daemon_instance_id, request)
            .await
    }
}

/// The record of the session in `session_dir`, refused as `DeleteSession` refuses a session this
/// daemon does not hold. A record that cannot be read is `INTERNAL`, never that refusal: a
/// parent closing its codebase half reads "not held here" as a checkout already gone.
fn held_session(session_dir: &Path) -> Result<SessionMetadata, Status> {
    if !session_dir.is_dir() {
        return Err(Status::failed_precondition(
            tddy_session_agents::peer_session_answer::SESSION_NOT_HELD_HERE,
        ));
    }
    tddy_core::read_session_metadata(session_dir).map_err(|error| {
        Status::internal(format!(
            "the record of the session in {} cannot be read: {error}",
            session_dir.display()
        ))
    })
}
