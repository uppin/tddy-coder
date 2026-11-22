//! The session inspector's Worktree-sync and PR-tab handlers (`#workqueue` 4/14): resolve the session,
//! then hand its worktree to [`crate::worktree_sync`] or its branch to [`crate::pull_request_view`].

use std::path::{Path, PathBuf};

use tddy_rpc::{Response, Status};
use tddy_service::proto::worktree::{
    SessionPullRequestView, SessionWorktreeRequest, WorktreeSyncState,
};

use super::WorktreeServiceImpl;
use crate::pull_request_view::watch_pull_request;
use crate::service_helpers::{blocking_within, sync_state_to_proto};
use crate::stream::MpscResultStream;
use crate::worktree_sync::{SyncState, WorktreeSyncError};

impl WorktreeServiceImpl {
    /// Run a sync-state read or action in session `req.session_id`'s own worktree, off the async
    /// runtime and under the request deadline. A target session's worktree lives on another host:
    /// it answers with the reason it is unavailable rather than an error.
    pub(super) async fn session_sync(
        &self,
        op_label: &'static str,
        req: SessionWorktreeRequest,
        action: fn(&Path) -> Result<SyncState, WorktreeSyncError>,
    ) -> Result<Response<WorktreeSyncState>, Status> {
        let os_user = self.authorize(&req.session_token)?;
        let session_dir = self.session_dir_for(&os_user, req.session_id.trim())?;
        if !session_dir.is_dir() {
            return Err(Status::not_found("session not found"));
        }
        if crate::target_worktrees::target_session_in(&session_dir).is_some() {
            // TODO: no target-side sync-state reader yet —
            // docs/dev/todo/2026-11-22-target-session-inspector-has-no-sync-state-or-pr.md
            return Ok(Response::new(WorktreeSyncState {
                unavailable_reason: "a target session's worktree sync state is not available"
                    .to_string(),
                ..Default::default()
            }));
        }
        let worktree = tddy_core::read_session_metadata(&session_dir)
            .map_err(|e| Status::internal(format!("session metadata: {e}")))?
            .repo_path
            .map(PathBuf::from)
            .ok_or_else(|| Status::failed_precondition("session has no worktree"))?;
        if !worktree.is_dir() {
            return Err(Status::failed_precondition(
                "the session's worktree is not on disk",
            ));
        }
        let state = blocking_within(self.request_timeout(), op_label, move || {
            action(&worktree).map_err(|e| anyhow::anyhow!(e.0))
        })
        .await?;
        Ok(Response::new(sync_state_to_proto(state)))
    }

    /// `StreamSessionPullRequest`: the PR view of session `req.session_id`'s branch, live from the
    /// host's GitHub inbox until the client goes away.
    pub(super) async fn session_pull_request_stream(
        &self,
        req: SessionWorktreeRequest,
    ) -> Result<Response<MpscResultStream<SessionPullRequestView>>, Status> {
        let os_user = self.authorize(&req.session_token)?;
        let session_id = req.session_id.trim();
        let session_dir = self.session_dir_for(&os_user, session_id)?;
        if !session_dir.is_dir() {
            return Err(Status::not_found("session not found"));
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        if crate::target_worktrees::target_session_in(&session_dir).is_some() {
            // TODO: a target session's branch lives on another host; its PR is not read from here —
            // docs/dev/todo/2026-11-22-target-session-inspector-has-no-sync-state-or-pr.md
            let view = SessionPullRequestView {
                unavailable_reason: "a target session's pull request is not available".to_string(),
                ..Default::default()
            };
            let _ = tx.send(Ok(view));
            return Ok(Response::new(MpscResultStream::from(rx)));
        }
        let pull_requests = self
            .pull_requests
            .get()
            .ok_or_else(|| Status::failed_precondition("this daemon reads no pull requests"))?;
        let metadata = tddy_core::read_session_metadata(&session_dir)
            .map_err(|e| Status::internal(format!("session metadata: {e}")))?;
        let branch = tddy_core::read_changeset(&session_dir)
            .map_err(|e| Status::internal(format!("session changeset: {e}")))?
            .branch
            .ok_or_else(|| Status::failed_precondition("the session records no branch"))?;
        let site = pull_requests.site_for_session(
            &os_user,
            session_id,
            &metadata.project_id,
            &req.session_token,
        )?;

        let mut watch = watch_pull_request(site.inbox, site.repo, branch);
        tokio::spawn(async move {
            loop {
                // Ends with the client, not only at its next view: a dropped watch stops the
                // inbox subscription behind it.
                let view = tokio::select! {
                    () = tx.closed() => return,
                    view = watch.next() => view,
                };
                let Some(view) = view else { return };
                if tx.send(Ok(view)).is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(MpscResultStream::from(rx)))
    }
}
