//! Family P — [`tddy_session_lifecycle::WorkQueueHandler`], answered by [`WorkQueueRpcHandler`].

use super::branch_legs::{base_sync_unavailable, base_sync_view, worktree_leg};
use super::guards::{token_for_repoint, validate_repoint_target};
use super::pr_status::owner_repo_from_repo_root;
use super::project_stacks::{require_a_stack, work_queue_target, WorkQueueTarget};
use super::{wire_same_anyhow, WorkQueueRpcHandler};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_projects::project_storage;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::types::BranchSession;
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, AddWorkItemResponse, GetPrStatusRequest, GetPrStatusResponse,
    GetWorkQueueRequest, GetWorkQueueResponse, LinkWorkItemRequest, LinkWorkItemResponse,
    ListProjectWorkQueuesRequest, ListProjectWorkQueuesResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderWorkItemRequest,
    ReorderWorkItemResponse, RepointWorkItemRequest, RepointWorkItemResponse,
    ResolveWorkItemBaseRequest, ResolveWorkItemBaseResponse,
};
use tddy_service::proto::work_queue::{
    AssignWorkItemPrRequest, ClearWorkItemBadgeRequest, LinkWorkItemSessionRequest,
    SetWorkItemBadgeRequest, SetWorkItemLabelsRequest, UnassignWorkItemPrRequest,
    UnlinkWorkItemSessionRequest, WorkItemLinksResponse,
};
use tddy_session_lifecycle::connection_service::{
    local_gh_github_token, project_github_token, resolve_os_user, spawn_blocking_with_timeout,
    wire_same,
};
use tddy_session_lifecycle::session_list_enrichment;
use tddy_session_lifecycle::session_reader::DaemonSessionListing;
use tddy_session_lifecycle::user_sessions_path::{projects_path_for_user, sessions_base_for_user};
use tddy_session_lifecycle::WorkQueueHandler;
use tddy_worktree_service::branch_owner::find_session_owning_branch;

#[async_trait]
impl WorkQueueHandler for WorkQueueRpcHandler {
    async fn add_work_item(
        &self,
        request: Request<AddWorkItemRequest>,
    ) -> Result<Response<AddWorkItemResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "AddWorkItem: session_id={} title={:?}",
            req.session_id.trim(),
            req.title
        );
        let (github_user, os_user_owned) = self.authenticated_user(&req.session_token)?;
        let target = work_queue_target(req.stack.as_ref(), &req.session_id)?;
        if let Some(answered) = self
            .forward_if_addressed::<_, AddWorkItemResponse>("AddWorkItem", &target, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }
        let _ = &github_user;
        if req.title.trim().is_empty() {
            return Err(Status::invalid_argument("title is required"));
        }
        let os_user = &os_user_owned;
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        if let WorkQueueTarget::Address(address) = &target {
            let added = self.add_work_item_to_store(&sessions_base, address, &req)?;
            log::info!(
                "AddWorkItem: success stack={} node_id={} title={:?}",
                address.queue_id,
                added.item_id,
                req.title
            );
            return Ok(Response::new(added));
        }
        let WorkQueueTarget::Session(session_id) = &target else {
            unreachable!("work_queue_target yields an address or a session")
        };
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        require_a_stack(&session_dir)?;

        let branch_suggestion =
            (!req.branch_suggestion.trim().is_empty()).then(|| req.branch_suggestion.clone());
        let child_recipe = (!req.child_recipe.trim().is_empty()).then(|| req.child_recipe.clone());

        // The appended node, so the response can *name* what this call created. The caller must not
        // infer it by diffing the returned plan: the orchestrator agent appends nodes to the same
        // stack, so a plan can come back holding several nodes the caller has never seen.
        let added = tddy_work_queue::item_ops::add_work_item(
            &session_dir,
            tddy_work_queue::item_ops::AddWorkItemInput {
                title: req.title.clone(),
                description: req.description.clone(),
                branch_suggestion,
                parents: req.parents.clone(),
                child_recipe,
            },
        )
        .map_err(Status::invalid_argument)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `work_queue_json` is byte-for-byte the same wire shape
        // `PrStackScreen`'s `parseWorkQueue` already knows how to read.
        let work_queue_json = session_list_enrichment::work_queue_json_of(&session_dir);

        log::info!(
            "AddWorkItem: success session_id={} node_id={} title={:?}",
            req.session_id.trim(),
            added.node_id,
            req.title
        );
        Ok(Response::new(AddWorkItemResponse {
            work_queue_json,
            item_id: added.node_id,
        }))
    }

    async fn get_pr_status(
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

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        use tddy_service::proto::work_queue::{BranchRemote, BranchResolution, BranchWorktree};

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

        // Session — the session that owns the branch, by the one rule `branch_owner` holds for every
        // surface that asks (prefer active, then most-recently-updated).
        let branch_for_scan = branch.clone();
        let sessions_base_for_scan = sessions_base.clone();
        let live = self.terminals.live_sessions().await;
        let session = spawn_blocking_with_timeout(
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
        let session = match session {
            Some(s) => BranchSession {
                exists: true,
                session_id: s.session_id,
                is_active: s.is_active,
                status: s.status,
            },
            None => BranchSession::default(),
        };

        // Worktree — the on-disk worktree checked out for the branch (non-erroring), and whether it
        // holds outstanding work. Dirtiness is deliberately *not* cached with the base comparison
        // below: it is not a function of the two commits, and an operator's edit must show up on the
        // next tick.
        //
        // Both halves are git subprocesses — a `worktree list` walk and a `status --porcelain` — so
        // they run on the blocking pool like every other leg. A timeout degrades this leg to "no
        // worktree" rather than failing the call, which is the same contract the other four keep.
        let worktree_repo_root = repo_root.clone();
        let branch_for_worktree = branch.clone();
        let worktree = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: read the branch's worktree",
            move || {
                wire_same_anyhow(&worktree_leg(
                    worktree_repo_root.as_deref(),
                    &branch_for_worktree,
                ))
            },
        )
        .await
        .unwrap_or_else(|status| {
            log::warn!(
                "QueryBranch: the worktree leg did not complete: {}",
                status.message()
            );
            BranchWorktree::default()
        });

        // Remote — `origin/<branch>` in the orchestrator's repo, which is what a descendant's
        // worktree is created from. Only as fresh as the last fetch, so it can delay a spawn but
        // never permit one that would fail inside `git fetch`.
        //
        // Shells out to `git rev-parse`, so it goes to the blocking pool like every other leg here:
        // this handler is polled once per rendered row every few seconds, and a subprocess run
        // inline would occupy a runtime worker thread for its whole duration.
        let remote_repo_root = repo_root.clone();
        let remote_branch = branch.clone();
        let remote = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: remote ref",
            move || {
                Ok(
                    match remote_repo_root.as_deref().and_then(|root| {
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
            // Degrades this leg alone — an absent remote ref only blocks a *descendant's* spawn, and
            // reporting the RPC as failed would take the session, worktree and PR legs down with it.
            log::warn!(
                "QueryBranch: resolving the remote ref for '{branch}' did not complete: {}",
                status.message()
            );
            BranchRemote::default()
        });

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

    /// What a child spawn's branch bases off, answered by the daemon that owns the stack parent.
    ///
    /// The whole point of the call is *where* it is served. A `work_queue_parent` is a bare session id
    /// and the base is read out of that session's own `changeset.yaml`, so a daemon resolving one
    /// it does not hold reads an absent file — not "this branch was not planned", but "no such
    /// session", which refuses a spawn over an orchestrator that exists one host over. So the
    /// question is routed to the named daemon first, and answered from local disk only when this
    /// daemon is the one that owns it.
    ///
    /// Routed **before** the caller is authenticated, like the roster RPCs: a token is verified by
    /// the daemon that serves the call, and a peer's user mapping is not this one's to judge.
    async fn resolve_work_item_base(
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

    /// Record a child session and the branch it created on a planned node of a pr-stack
    /// orchestrator, answered by the daemon that owns that orchestrator.
    ///
    /// The write half of [`Self::resolve_work_item_base`], and routed for the same reason: a planned
    /// node lives in its orchestrator's own `changeset.yaml`, so only the daemon holding that
    /// session can write it. A child spawned on another host writing its *own* sessions tree finds
    /// no such session and writes nothing at all — the node keeps neither a branch nor a child, and
    /// every descendant then stays unspawnable because
    /// [`tddy_core::changeset::WorkQueue::base_ref_for_spawn`] gates on a parent owning a branch (D35).
    ///
    /// Routed **before** the caller is authenticated, like every other peer-routed RPC: a token is
    /// verified by the daemon that serves the call, and a peer's user mapping is not this one's to
    /// judge.
    ///
    /// Nothing here is guessed. An unnamed node is refused rather than derived from the branch
    /// (D34), and a node the plan does not hold is `not_found` with nothing written — silently
    /// succeeding against an unchanged plan is precisely the bug this RPC exists to remove.
    async fn link_work_item(
        &self,
        request: Request<LinkWorkItemRequest>,
    ) -> Result<Response<LinkWorkItemResponse>, Status> {
        self.rpc_activity.record();
        let req = request.into_inner();

        if let Some(answered) = self
            .peer_routing
            .rpc_served_by_peer(
                tddy_work_queue::WORK_QUEUE_SERVICE,
                "LinkWorkItem",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }

        let node_id = req.item_id.trim();
        if node_id.is_empty() {
            return Err(Status::invalid_argument(
                "node_id is required: a planned node is never derived from the branch a spawn creates, which the operator can rename before confirming",
            ));
        }
        let child_session_id = req.child_session_id.trim();
        if child_session_id.is_empty() {
            return Err(Status::invalid_argument(
                "child_session_id is required: a link names the session that materialized the node",
            ));
        }
        // Refused for the same reason `node_id` is. A blank branch leaves `node.branch` untouched —
        // `link_work_item_to_session` takes `None` and writes nothing there — while the RPC
        // answers `Ok`, so the node still refuses every descendant and nothing said so. That is the
        // "reports success, changes nothing" shape D35 exists to remove, and it is reachable
        // whenever `effective_spawn_branch` yields an empty name.
        let branch = req.branch.trim();
        if branch.is_empty() {
            return Err(Status::invalid_argument(
                "branch is required: the branch is what makes the node's descendants spawnable, so a link that records none changes nothing",
            ));
        }

        let os_user = resolve_os_user(&self.config, &self.user_resolver, &req.session_token)?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.orchestrator_session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.orchestrator_session_id);
        require_a_stack(&session_dir)?;

        // Checked before the write, because `link_work_item_to_session` leaves an unknown
        // node alone and reports success — which reads to the caller exactly like a link that
        // landed, and is the failure mode this whole path exists to make visible.
        let stack = tddy_core::changeset::read_session_stack(&session_dir)
            .map_err(|e| Status::internal(e.to_string()))?;
        if stack
            .as_ref()
            .is_none_or(|stack| stack.node(node_id).is_none())
        {
            return Err(Status::not_found(format!(
                "planned node '{node_id}' is not in the stack of orchestrator {}",
                req.orchestrator_session_id.trim()
            )));
        }

        tddy_core::changeset::link_work_item_to_session(
            &session_dir,
            node_id,
            child_session_id,
            Some(branch.to_string()),
        )
        .map_err(|e| {
            Status::internal(format!(
                "failed to link stack node '{node_id}' to child session {child_session_id}: {e}"
            ))
        })?;
        log::info!(
            "LinkWorkItem: orchestrator {} node '{node_id}' now records child session {child_session_id} on branch '{branch}'",
            req.orchestrator_session_id.trim()
        );

        // Re-read and serialize through the same path `AddWorkItem` / `RepointWorkItem` /
        // `ReorderWorkItem` answer with, so a caller that renders the plan reuses `parseWorkQueue`
        // and does not re-read what this call already knows.
        Ok(Response::new(LinkWorkItemResponse {
            work_queue_json: session_list_enrichment::work_queue_json_of(&session_dir),
        }))
    }

    async fn repoint_work_item(
        &self,
        request: Request<RepointWorkItemRequest>,
    ) -> Result<Response<RepointWorkItemResponse>, Status> {
        let req = request.into_inner();
        let (github_user, os_user_owned) = self.authenticated_user(&req.session_token)?;
        let target = work_queue_target(req.stack.as_ref(), &req.session_id)?;
        if let Some(answered) = self
            .forward_if_addressed::<_, RepointWorkItemResponse>("RepointWorkItem", &target, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }
        let _ = &github_user;
        let os_user = &os_user_owned;
        if req.item_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = self.session_dir_for_target(&sessions_base, &target)?;
        require_a_stack(&session_dir)?;

        // A repoint rebases a local branch and re-targets a PR base, so it needs the real checkout:
        // an unknown one is a precondition failure, not a repoint attempted in the session directory.
        let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
            Status::failed_precondition(
                "no checkout is recorded for this session, so its repository could not be resolved",
            )
        })?;
        let owner_repo = owner_repo_from_repo_root(&repo_root).unwrap_or_default();
        let default_branch =
            tddy_core::resolve_default_integration_base_ref(&repo_root).map_err(|e| {
                Status::failed_precondition(format!("could not resolve default branch: {e}"))
            })?;

        let node_id = req.item_id.trim().to_string();

        // On the wire, an unnamed target means "the project's default branch" — the daemon's own
        // resolved ref, substituted here.
        //
        // A client cannot always name it: `ProjectEntry.main_branch_ref` is empty for a project that
        // stores no default, and the web then renders "Repoint to default branch". Forwarding that
        // empty string would instead select the recipe's drop-merged-parents rule, which in the very
        // case this feature exists for — a predecessor whose PR merged but whose plan still records
        // `open` — drops nothing at all and returns success against an unchanged plan. The operator
        // would see no error and no change.
        //
        // The recipe's `None` mode is therefore in-process only; it is never reachable from here.
        let requested_target = if req.target_base_branch.trim().is_empty() {
            default_branch.as_str()
        } else {
            req.target_base_branch.as_str()
        };

        // The named target must be a branch this node can be based onto: the repoint retains only
        // the parents that own it, so an unvalidated target is a silent plan rewrite.
        let stack = tddy_core::changeset::read_session_stack(&session_dir)
            .map_err(|e| Status::internal(e.to_string()))?
            .unwrap_or_default();
        let parent_branches: Vec<String> = stack
            .node(&node_id)
            .map(|node| {
                node.parents
                    .iter()
                    .filter_map(|parent_id| stack.node(parent_id)?.branch.clone())
                    .collect()
            })
            .unwrap_or_default();
        let target_base_branch = validate_repoint_target(
            requested_target,
            &default_branch,
            &parent_branches
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>(),
        )
        .map_err(Status::invalid_argument)?;

        // Only a node that owns a branch has a PR to re-target, so only it needs GitHub. Its token is
        // the account the orchestrator's project is assigned, from the caller's own vault — resolved
        // here, before the plan is rewritten, so a refusal changes nothing. The login-keyed
        // `retained_github_token` is not used: it is the credential this stack retires.
        let node_owns_branch = stack
            .node(&node_id)
            .is_some_and(|node| node.branch.is_some());
        let token = token_for_repoint(node_owns_branch, || {
            let project_id = tddy_core::read_session_metadata(&session_dir)
                .map_err(|e| format!("the orchestrator's session metadata could not be read: {e}"))?
                .project_id;
            let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| "could not resolve the projects path".to_string())?;
            let project = project_storage::find_project(&projects_dir, project_id.trim())
                .map_err(|e| format!("the orchestrator's project could not be read: {e}"))?
                .ok_or_else(|| {
                    format!("the orchestrator's project '{project_id}' is not registered")
                })?;
            // The session's own source (its sidecar), else this host's row of its project.
            let local_gh = match tddy_core::read_session_github_source(&session_dir)
                .map_err(|e| format!("the orchestrator's GitHub source could not be read: {e}"))?
            {
                Some(source) => source == tddy_core::SessionGithubSource::LocalGh,
                None => project.local_gh,
            };
            if local_gh {
                return local_gh_github_token(&self.github_cli, os_user, &project.git_url);
            }
            project_github_token(
                self.credential_vaults.clone(),
                Arc::clone(&self.user_resolver),
                &req.session_token,
                &project.accounts,
            )
        })?;

        let session_dir_for_op = session_dir.clone();
        tokio::task::spawn_blocking(move || {
            // A plan-only repoint reaches no GitHub call, so it holds no credential.
            let gh = match token {
                Some(token) => tddy_github::pr_api::RealGithubPrApi::with_token(owner_repo, token),
                None => tddy_github::pr_api::RealGithubPrApi::without_credential(owner_repo),
            };
            tddy_work_queue::item_ops::repoint_work_item(
                &session_dir_for_op,
                &repo_root,
                &node_id,
                &default_branch,
                target_base_branch.as_deref(),
                &gh,
            )
        })
        .await
        .map_err(|e| Status::internal(format!("repoint_work_item join error: {e}")))?
        .map_err(Status::failed_precondition)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `work_queue_json` is the exact wire shape `PrStackScreen`
        // already knows how to parse.
        let work_queue_json = session_list_enrichment::work_queue_json_of(&session_dir);
        Ok(Response::new(RepointWorkItemResponse { work_queue_json }))
    }

    /// Move one planned node up or down the operator's reading order.
    ///
    /// Touches nothing but `WorkItem.display_order`: the dependency graph is a different fact, and
    /// keeping the two independent is the whole reason the field exists. Moving past either end is a
    /// successful no-op — the control at the end of the list is inert, not wrong.
    async fn reorder_work_item(
        &self,
        request: Request<ReorderWorkItemRequest>,
    ) -> Result<Response<ReorderWorkItemResponse>, Status> {
        self.reorder_work_item_rpc(request).await
    }

    async fn stream_work_queue_status(
        &self,
        request: Request<tddy_service::proto::work_queue::StreamWorkQueueStatusRequest>,
    ) -> Result<Response<tddy_work_queue::rpc::WorkQueueStatusStream>, Status> {
        self.stream_work_queue_status_rpc(request).await
    }

    async fn assign_work_item_pr(
        &self,
        request: Request<AssignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.assign_work_item_pr_rpc(request).await
    }

    async fn unassign_work_item_pr(
        &self,
        request: Request<UnassignWorkItemPrRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.unassign_work_item_pr_rpc(request).await
    }

    async fn link_work_item_session(
        &self,
        request: Request<LinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.link_work_item_session_rpc(request).await
    }

    async fn unlink_work_item_session(
        &self,
        request: Request<UnlinkWorkItemSessionRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.unlink_work_item_session_rpc(request).await
    }

    async fn set_work_item_labels(
        &self,
        request: Request<SetWorkItemLabelsRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.set_work_item_labels_rpc(request).await
    }

    async fn set_work_item_badge(
        &self,
        request: Request<SetWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.set_work_item_badge_rpc(request).await
    }

    async fn clear_work_item_badge(
        &self,
        request: Request<ClearWorkItemBadgeRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.clear_work_item_badge_rpc(request).await
    }

    async fn set_work_queue_dependencies(
        &self,
        request: Request<tddy_service::proto::work_queue::SetWorkQueueDependenciesRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        self.set_work_queue_dependencies_rpc(request).await
    }

    async fn get_work_queue_graph(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueGraphRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueGraphResponse>, Status> {
        self.get_work_queue_graph_rpc(request).await
    }

    async fn get_work_queue_waves(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueWavesRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueWavesResponse>, Status> {
        self.get_work_queue_waves_rpc(request).await

    }

    async fn get_work_item_inbox(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemInboxResponse>, Status> {
        self.get_work_item_inbox_rpc(request).await
    }

    async fn get_work_item_outbox(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkItemOutboxResponse>, Status> {
        self.get_work_item_outbox_rpc(request).await
    }

    async fn get_work_queue_commit_counts(
        &self,
        request: Request<tddy_service::proto::work_queue::GetWorkQueueGraphRequest>,
    ) -> Result<Response<tddy_service::proto::work_queue::WorkQueueCommitCountsResponse>, Status>
    {
        self.get_work_queue_commit_counts_rpc(request).await
    }

    /// Take a base branch's commits into a planned node's branch, inside that node's own worktree,
    /// and push the result.
    ///
    /// A mutation, so an unknown checkout is a precondition failure rather than a degraded leg — the
    /// pull has nowhere to happen. The branch is then re-resolved **uncached**: the refs it compares
    /// have just moved, and the point of returning a resolution at all is that the row repaints
    /// without waiting for the next poll tick.
    async fn pull_base_into_branch(
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

    /// See `project_stacks.rs`, which holds the project-stack RPCs (`#orchestrators` 9/11).
    async fn list_project_work_queues(
        &self,
        request: Request<ListProjectWorkQueuesRequest>,
    ) -> Result<Response<ListProjectWorkQueuesResponse>, Status> {
        self.list_project_work_queues_rpc(request).await
    }

    async fn get_work_queue(
        &self,
        request: Request<GetWorkQueueRequest>,
    ) -> Result<Response<GetWorkQueueResponse>, Status> {
        self.get_work_queue_rpc(request).await
    }
}
