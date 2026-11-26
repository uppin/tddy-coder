//! Bodies of the `WorkQueueHandler` methods that `ports.rs` delegates here, as inherent methods
//! of [`WorkQueueRpcHandler`].
//!
//! TODO: these are a verbatim hand move out of the trait impl in `ports.rs` — the restructure
//! engine cannot cut a trait impl (no extract-into-function after a `forward_if_addressed` guard).

use super::branch_legs::worktree_leg;
use super::project_stacks::work_queue_target;
use super::{wire_same_anyhow, WorkQueueRpcHandler};
use std::path::PathBuf;
use tddy_projects::project_storage;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::types::BranchSession;
use tddy_service::proto::work_queue::{
    GetPrStatusRequest, GetPrStatusResponse, QueryBranchRequest, QueryBranchResponse,
    ResolveWorkItemBaseRequest, ResolveWorkItemBaseResponse,
};
use tddy_session_lifecycle::connection_service::{resolve_os_user, wire_same};
use tddy_session_lifecycle::session_reader::DaemonSessionListing;
use tddy_session_lifecycle::user_sessions_path::{projects_path_for_user, sessions_base_for_user};
use tddy_worktree_service::branch_owner::find_session_owning_branch;

impl WorkQueueRpcHandler {
    pub(super) async fn query_branch_rpc(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        use tddy_service::proto::work_queue::BranchResolution;

        let req = request.into_inner();
        let os_user_owned = self.authenticated_os_user(&req.session_token)?;
        let target = work_queue_target(req.stack.as_ref(), &req.session_id)?;
        if let Some(answered) = self
            .forward_if_addressed::<_, QueryBranchResponse>("QueryBranch", &target, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }
        let os_user = &os_user_owned;
        if req.branch.trim().is_empty() {
            return Err(Status::invalid_argument("branch is required"));
        }
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        // `None` when nothing in the session directory names a checkout: every repo-derived leg below
        // then reads as absent or unavailable, never as a confident answer about a repo we don't know.
        let repo_root = self.repo_root_for_target(&sessions_base, os_user, &target)?;
        let branch = req.branch.trim().to_string();

        let session = self.branch_session_leg(&sessions_base, &branch).await?;
        let worktree = self.branch_worktree_leg(repo_root.clone(), &branch).await;
        let remote = self.branch_remote_leg(repo_root.clone(), &branch).await;

        // Base sync — how the branch stands against the base the caller named. Like every other leg
        // it never fails the call: an unnamed base, an unknown checkout, a probe that could not run
        // and a probe that timed out all arrive as `unavailable` with a reason.
        let base_sync = match self
            .base_sync_leg(repo_root.as_deref(), &branch, req.base_branch.trim())
            .await
        {
            Some(leg) => Some(wire_same(&leg)?),
            None => None,
        };

        // PR — same path as get_pr_status. A lookup that cannot be performed degrades this leg
        // alone; the session, worktree and remote legs above stay usable.
        let site = self.pr_site_for_target(os_user, &req.session_token, &sessions_base, &target);
        let pr = self
            .pr_status_for_caller(site, repo_root.as_deref(), &branch)
            .await;

        Ok(Response::new(QueryBranchResponse {
            resolution: Some(BranchResolution {
                branch,
                session: Some(session),
                worktree: Some(worktree),
                pr: Some(wire_same(&pr)?),
                remote: Some(remote),
                base_sync,
            }),
        }))
    }

    /// Session — the session that owns the branch, by the one rule `branch_owner` holds for every
    /// surface that asks (prefer active, then most-recently-updated).
    async fn branch_session_leg(
        &self,
        sessions_base: &std::path::Path,
        branch: &str,
    ) -> Result<BranchSession, Status> {
        let branch_for_scan = branch.to_string();
        let sessions_base_for_scan = sessions_base.to_path_buf();
        let live = self.terminals.live_sessions().await;
        let owner = self
            .query_branch_lanes
            .session
            .run(
                format!("{}|{branch}", sessions_base.display()),
                self.config.spawn_worker_request_timeout(),
                "QueryBranch: scan sessions by branch",
                move || {
                    find_session_owning_branch(
                        &DaemonSessionListing::new(live),
                        &sessions_base_for_scan,
                        &branch_for_scan,
                    )
                },
            )
            .await?;
        Ok(match owner {
            Some(s) => BranchSession {
                exists: true,
                session_id: s.session_id,
                is_active: s.is_active,
                status: s.status,
            },
            None => BranchSession::default(),
        })
    }

    /// Worktree — the on-disk worktree checked out for the branch (non-erroring), and whether it
    /// holds outstanding work. Dirtiness is deliberately *not* cached with the base comparison: it
    /// is not a function of the two commits, and an operator's edit must show up on the next tick.
    ///
    /// Both halves are git subprocesses — a `worktree list` walk and a `status --porcelain` — so
    /// they run in the lane like every other leg. A timeout or a full lane degrades this leg to
    /// "no worktree" rather than failing the call, which is the same contract the other legs keep.
    async fn branch_worktree_leg(
        &self,
        repo_root: Option<PathBuf>,
        branch: &str,
    ) -> tddy_service::proto::work_queue::BranchWorktree {
        let key = format!("{repo_root:?}|{branch}");
        let branch_for_worktree = branch.to_string();
        self.query_branch_lanes
            .worktree
            .run(
                key,
                self.config.spawn_worker_request_timeout(),
                "QueryBranch: read the branch's worktree",
                move || wire_same_anyhow(&worktree_leg(repo_root.as_deref(), &branch_for_worktree)),
            )
            .await
            .unwrap_or_else(|status| {
                log::warn!(
                    "QueryBranch: the worktree leg did not complete: {}",
                    status.message()
                );
                tddy_service::proto::work_queue::BranchWorktree::default()
            })
    }

    /// Remote — `origin/<branch>` in the orchestrator's repo, which is what a descendant's worktree
    /// is created from. Only as fresh as the last fetch, so it can delay a spawn but never permit
    /// one that would fail inside `git fetch`.
    ///
    /// Shells out to `git rev-parse`, so it runs in the lane: this handler is polled once per
    /// rendered row every few seconds.
    async fn branch_remote_leg(
        &self,
        repo_root: Option<PathBuf>,
        branch: &str,
    ) -> tddy_service::proto::work_queue::BranchRemote {
        use tddy_service::proto::work_queue::BranchRemote;
        let key = format!("{repo_root:?}|{branch}");
        let remote_branch = branch.to_string();
        self.query_branch_lanes
            .remote
            .run(
                key,
                self.config.spawn_worker_request_timeout(),
                "QueryBranch: remote ref",
                move || {
                    Ok(
                        match repo_root.as_deref().and_then(|root| {
                            tddy_core::worktree::remote_branch_ref_sha(root, &remote_branch)
                        }) {
                            Some(sha) => BranchRemote { exists: true, sha },
                            None => BranchRemote::default(),
                        },
                    )
                },
            )
            .await
            .unwrap_or_else(|status| {
                // Degrades this leg alone — an absent remote ref only blocks a *descendant's* spawn,
                // and reporting the RPC as failed would take the other legs down with it.
                log::warn!(
                    "QueryBranch: resolving the remote ref for '{branch}' did not complete: {}",
                    status.message()
                );
                BranchRemote::default()
            })
    }

    pub(super) async fn get_pr_status_rpc(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        let req = request.into_inner();
        let os_user_owned = self.authenticated_os_user(&req.session_token)?;
        let target = work_queue_target(req.stack.as_ref(), &req.session_id)?;
        if let Some(answered) = self
            .forward_if_addressed::<_, GetPrStatusResponse>("GetPrStatus", &target, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }
        let os_user = &os_user_owned;
        if req.branch.trim().is_empty() {
            return Err(Status::invalid_argument("branch is required"));
        }
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        // An orchestrator never gets a worktree, so its `changeset.yaml` records no checkout and only
        // `.session.yaml` names the one it plans over. `pr_status_for_caller` resolves the GitHub
        // namespace from that checkout's remote and reports a lookup it cannot perform — including
        // one over an unknown checkout — as unavailable rather than failing this call.
        let repo_root = self.repo_root_for_target(&sessions_base, os_user, &target)?;

        let site = self.pr_site_for_target(os_user, &req.session_token, &sessions_base, &target);
        let status = self
            .pr_status_for_caller(site, repo_root.as_deref(), req.branch.trim())
            .await;
        Ok(Response::new(GetPrStatusResponse {
            status: Some(wire_same(&status)?),
        }))
    }

    pub(super) async fn resolve_work_item_base_rpc(
        &self,
        request: Request<ResolveWorkItemBaseRequest>,
    ) -> Result<Response<ResolveWorkItemBaseResponse>, Status> {
        self.rpc_activity.record();
        let req = request.into_inner();

        if let Some(answered) = self
            .peer_routing
            .rpc_served_by_peer(
                tddy_work_queue::WORK_QUEUE_SERVICE,
                "ResolveWorkItemBase",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }

        let work_queue_parent = req.work_queue_parent.trim();
        if work_queue_parent.is_empty() {
            return Err(Status::invalid_argument(
                "work_queue_parent is required: ResolveWorkItemBase resolves the base of a named parent session",
            ));
        }
        let os_user = resolve_os_user(&self.config, &self.user_resolver, &req.session_token)?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let projects_dir = projects_path_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        // This daemon's own checkout of the same logical project. The answer is a remote-tracking
        // ref, so the asking daemon's checkout and this one need only share an origin — never a path.
        let project = project_storage::find_project(&projects_dir, req.project_id.trim())
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        if !repo_root.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let base_ref = tddy_core::resolve_chain_base_ref(
            &sessions_base,
            Some(work_queue_parent),
            &repo_root,
            req.work_item_id.trim(),
            req.new_branch_name.trim(),
        )
        .map_err(Status::failed_precondition)?;
        log::info!(
            "ResolveWorkItemBase: parent {work_queue_parent} in project {} bases {} on {:?}",
            req.project_id.trim(),
            req.new_branch_name.trim(),
            base_ref
        );
        Ok(Response::new(ResolveWorkItemBaseResponse {
            base_ref: base_ref.unwrap_or_default(),
        }))
    }
}
