//! Bodies of the `WorkQueueHandler` methods that `ports.rs` delegates here, as inherent methods
//! of [`WorkQueueRpcHandler`].
//!
//! TODO: these are a verbatim hand move out of the trait impl in `ports.rs` — the restructure
//! engine cannot cut a trait impl (no extract-into-function after a `forward_if_addressed` guard).

use super::branch_legs::{base_sync_unavailable, base_sync_view, worktree_leg};
use super::project_stacks::{require_a_stack, work_queue_target};
use super::{wire_same_anyhow, WorkQueueRpcHandler};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::types::BranchSession;
use tddy_service::proto::work_queue::{PullBaseIntoBranchRequest, PullBaseIntoBranchResponse};
use tddy_session_lifecycle::connection_service::{spawn_blocking_with_timeout, wire_same};
use tddy_session_lifecycle::session_reader::DaemonSessionListing;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_worktree_service::branch_owner::find_session_owning_branch;

impl WorkQueueRpcHandler {
    pub(super) async fn pull_base_into_branch_rpc(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        use tddy_service::proto::work_queue::{BranchRemote, BranchResolution, BranchWorktree};

        let req = request.into_inner();
        let os_user_owned = self.authenticated_os_user(&req.session_token)?;
        let target = work_queue_target(req.stack.as_ref(), &req.session_id)?;
        if let Some(answered) = self
            .forward_if_addressed::<_, PullBaseIntoBranchResponse>(
                "PullBaseIntoBranch",
                &target,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let os_user = &os_user_owned;
        if req.item_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        if req.base_branch.trim().is_empty() {
            return Err(Status::invalid_argument("base_branch is required"));
        }
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = self.session_dir_for_target(&sessions_base, &target)?;
        require_a_stack(&session_dir)?;

        let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
            Status::failed_precondition(
                "no checkout is recorded for this session, so its repository could not be resolved",
            )
        })?;

        let strategy = tddy_work_queue::item_ops::BaseSyncStrategy::from_wire(&req.strategy);
        let node_id = req.item_id.trim().to_string();
        let base_branch = req.base_branch.trim().to_string();
        let dirty_worktree_action = req.dirty_worktree_action.clone();
        let commit_message = req.commit_message.clone();
        let session_dir_for_op = session_dir.clone();
        let repo_root_for_op = repo_root.clone();
        let report = tokio::task::spawn_blocking(move || {
            tddy_work_queue::item_ops::pull_base_into_node_branch(
                &session_dir_for_op,
                &repo_root_for_op,
                &node_id,
                &base_branch,
                strategy,
                &dirty_worktree_action,
                &commit_message,
            )
        })
        .await
        .map_err(|e| Status::internal(format!("pull_base_into_branch join error: {e}")))?
        .map_err(Status::failed_precondition)?;

        // The branch the pull just moved, re-read from disk.
        let branch = tddy_core::changeset::read_session_stack(&session_dir)
            .ok()
            .flatten()
            .and_then(|stack| stack.node(req.item_id.trim())?.branch.clone())
            .unwrap_or_default();

        // A sessions-directory walk, two worktree probes and a `git merge-tree` — all blocking, so
        // they go to the blocking pool together rather than tying up a runtime thread for the length
        // of four git subprocesses.
        let resolution_branch = branch.clone();
        let resolution_repo_root = repo_root.clone();
        let resolution_sessions_base = sessions_base.clone();
        let resolution_base_branch = req.base_branch.trim().to_string();
        let live = self.terminals.live_sessions().await;
        let (session, worktree, remote, base_sync) = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "PullBaseIntoBranch: re-read the branch",
            move || {
                let session = match find_session_owning_branch(
                    &DaemonSessionListing::new(live),
                    &resolution_sessions_base,
                    &resolution_branch,
                )
                .ok()
                .flatten()
                {
                    Some(s) => BranchSession {
                        exists: true,
                        session_id: s.session_id,
                        is_active: s.is_active,
                        status: s.status,
                    },
                    None => BranchSession::default(),
                };
                let worktree = wire_same_anyhow(&worktree_leg(
                    Some(&resolution_repo_root),
                    &resolution_branch,
                ))?;
                let remote = match tddy_core::worktree::remote_branch_ref_sha(
                    &resolution_repo_root,
                    &resolution_branch,
                ) {
                    Some(sha) => BranchRemote { exists: true, sha },
                    None => BranchRemote::default(),
                };
                // Uncached on purpose: the cache is keyed on the refs and the commits they point at,
                // and both commits may have just moved. Going through it would answer about the pair
                // the poll saw a moment ago.
                let base_sync = Some(
                    match tddy_core::base_sync::branch_base_sync(
                        &resolution_repo_root,
                        &resolution_branch,
                        &resolution_base_branch,
                    ) {
                        Ok(sync) => wire_same_anyhow(&base_sync_view(sync))?,
                        Err(reason) => wire_same_anyhow(&base_sync_unavailable(
                            &resolution_base_branch,
                            &reason,
                        ))?,
                    },
                );
                Ok((session, worktree, remote, base_sync))
            },
        )
        .await
        // The pull itself already landed. Failing the whole call because re-reading the branch took
        // too long would leave the operator with no idea that it did, so the description degrades
        // and the report below still says what happened.
        .unwrap_or_else(|status| {
            use tddy_service::proto::work_queue::BranchBaseSync;
            let reason = format!(
                "the branch could not be re-read after the pull: {}",
                status.message()
            );
            log::warn!("PullBaseIntoBranch: {reason}");
            let base_sync = wire_same(&base_sync_unavailable(req.base_branch.trim(), &reason))
                .unwrap_or_else(|e| {
                    log::warn!(
                        "PullBaseIntoBranch: could not bridge degraded base_sync: {}",
                        e.message()
                    );
                    BranchBaseSync::default()
                });
            (
                BranchSession::default(),
                BranchWorktree::default(),
                BranchRemote::default(),
                Some(base_sync),
            )
        });
        let site = self.pr_site_for_target(os_user, &req.session_token, &sessions_base, &target);
        let pr = self
            .pr_status_for_caller(site, Some(&repo_root), &branch)
            .await;

        Ok(Response::new(PullBaseIntoBranchResponse {
            resolution: Some(BranchResolution {
                branch,
                session: Some(session),
                worktree: Some(worktree),
                pr: Some(wire_same(&pr)?),
                remote: Some(remote),
                base_sync,
            }),
            strategy: report.strategy.to_string(),
            changed: report.changed,
            pushed: report.pushed,
            push_error: report.push_error.unwrap_or_default(),
        }))
    }
}
