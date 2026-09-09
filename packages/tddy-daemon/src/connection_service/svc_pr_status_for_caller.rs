use std::path::PathBuf;
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;

use crate::{
    connection_service::{service_util, stack_parent},
    project_storage,
    user_sessions_path::projects_path_for_user,
};

use tddy_service::proto::connection::BranchSession;

use tddy_service::proto::connection::BranchConflict;

use tddy_service::proto::connection::StartSessionRequest;

use super::prepare_managed_workflow_inner;

use super::ManagedLaunch;

use std::sync::Arc;

use tddy_service::proto::connection::LinkStackNodeRequest;

use tddy_service::proto::connection::ResolveStackBaseRequest;

use tddy_rpc::Request;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_rpc::Status;

use super::base_sync_view;

use super::base_sync_through_cache;

use super::base_sync_unavailable;

use super::pr_state_label;

use super::owner_repo_from_repo_root;

use super::pr_status_unavailable;

use std::path::Path;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// PR status for one branch, resolved with the calling operator's own GitHub credential.
    ///
    /// `repo_root` is `None` when no file in the session directory records a checkout (see
    /// [`tddy_core::repo_root_for_session`]) — an unknown repository, not a repository without PRs.
    ///
    /// Never fails: a lookup that cannot be performed degrades this one field to *unavailable* (D8),
    /// and stub/demo authentication resolves to an empty result (D12) — so the enclosing RPC keeps
    /// returning its other legs instead of collapsing into an error the web discards wholesale.
    pub(crate) async fn pr_status_for_caller(
        &self,
        github_login: &str,
        repo_root: Option<&std::path::Path>,
        branch: &str,
    ) -> tddy_service::proto::connection::PrStatusView {
        use crate::github_pr_credentials::{pr_lookup_for_caller, PrLookup};
        use tddy_service::proto::connection::PrStatusView;
        use tddy_workflow_recipes::orchestrate_pr_stack::github::PrLookupOutcome;

        // Both ways of failing to name a GitHub repository leave the lookup un-performable, so they
        // are *unavailable* with a reason — reporting `exists = false` would claim the branch has no
        // PR when in fact nothing was ever asked (D8).
        let Some(repo_root) = repo_root else {
            return pr_status_unavailable(
                branch,
                "no checkout is recorded for this session, so its GitHub repository is unknown"
                    .to_string(),
            );
        };
        let Some(owner_repo) = owner_repo_from_repo_root(repo_root) else {
            return pr_status_unavailable(
                branch,
                format!(
                    "no GitHub repository could be resolved from the origin remote of {}",
                    repo_root.display()
                ),
            );
        };
        let stub_mode = self
            .config
            .github
            .as_ref()
            .and_then(|g| g.stub)
            .unwrap_or(false);
        let stored = self
            .github_token_store
            .as_ref()
            .and_then(|store| store.get(github_login));
        let token = match pr_lookup_for_caller(stub_mode, stored.as_deref()) {
            PrLookup::Empty => return PrStatusView::default(),
            PrLookup::Unavailable(reason) => return pr_status_unavailable(branch, reason),
            PrLookup::Perform(token) => token,
        };

        let head_branch = branch.to_string();
        let outcome = tokio::task::spawn_blocking(move || {
            use tddy_workflow_recipes::orchestrate_pr_stack::github::GithubPrApi;
            tddy_workflow_recipes::orchestrate_pr_stack::github::RealGithubPrApi::with_token(
                owner_repo, token,
            )
            .get_pr_by_head(&head_branch)
        })
        .await;

        match outcome {
            Ok(PrLookupOutcome::Found(pr)) => PrStatusView {
                exists: true,
                number: pr.number,
                url: pr.url,
                state: pr_state_label(pr.state).to_string(),
                unavailable: false,
                unavailable_reason: String::new(),
            },
            Ok(PrLookupOutcome::NotFound) => PrStatusView::default(),
            Ok(PrLookupOutcome::Unavailable(reason)) => pr_status_unavailable(branch, reason),
            Err(join_error) => pr_status_unavailable(
                branch,
                format!("the PR lookup did not complete: {join_error}"),
            ),
        }
    }

    /// How the branch stands against the base the caller named, for `QueryBranch`'s fifth leg.
    ///
    /// Never fails, exactly like the session, worktree, remote and PR legs beside it: an unnamed
    /// base, an unknown checkout, a probe that could not run and a probe that ran out of time all
    /// arrive as `unavailable` carrying a reason. A comparison that could not be made is byte-
    /// identical to a healthy one on every other field, so the discriminator is the only thing that
    /// keeps "could not tell" from rendering as "clean" (PRD D27).
    ///
    /// An unnamed base is reported unavailable rather than substituted with the project default
    /// (D29): this is a display, and the number beside a row must describe the same base the row's
    /// own base line shows.
    pub(crate) async fn base_sync_leg(
        &self,
        repo_root: Option<&std::path::Path>,
        branch: &str,
        base_branch: &str,
    ) -> Option<tddy_service::proto::connection::BranchBaseSync> {
        if base_branch.is_empty() {
            return Some(base_sync_unavailable(
                "",
                "no base branch was named for this branch, so there is nothing to compare it \
                 against",
            ));
        }
        let Some(repo_root) = repo_root else {
            return Some(base_sync_unavailable(
                base_branch,
                "no checkout is recorded for this session, so its repository could not be resolved",
            ));
        };

        let probe_root = repo_root.to_path_buf();
        let probe_branch = branch.to_string();
        let probe_base = base_branch.to_string();
        let probed = service_util::spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: compare branch against base",
            move || {
                Ok(base_sync_through_cache(
                    &probe_root,
                    &probe_branch,
                    &probe_base,
                ))
            },
        )
        .await;

        Some(match probed {
            Ok(Ok(sync)) => base_sync_view(sync),
            Ok(Err(reason)) => base_sync_unavailable(base_branch, &reason),
            // A timeout degrades this leg; it must not take the other four with it.
            Err(status) => base_sync_unavailable(
                base_branch,
                &format!("the comparison did not complete: {}", status.message()),
            ),
        })
    }

    /// Resolve what a spawn's worktree is cut from, on the daemon that owns its `stack_parent`.
    ///
    /// The parent's base is read out of the parent's own `changeset.yaml` — its stack, or its
    /// branch — so only the daemon holding that session can produce it. A parent this daemon owns
    /// (no host named, or its own) is resolved straight off its disk, exactly as before. A parent
    /// another daemon owns travels as a `ResolveStackBase` call, taken through this daemon's own
    /// handler so a forwarded resolution follows precisely the path a client's would, peer routing
    /// and refusals included.
    ///
    /// An empty `base_ref` from the owner is `None`, not `Some("")`: it says the parent named no
    /// chain base for this branch, and the caller then applies the project default. A refusal is a
    /// refusal, reported with the owner's own message.
    pub(crate) async fn resolve_chain_base_ref_status(
        &self,
        lookup: &stack_parent::StackBaseLookup<'_>,
    ) -> Result<Option<String>, Status> {
        if !lookup.selected_integration_base_ref.trim().is_empty() {
            return Ok(None);
        }
        match tddy_core::classify_stack_parent_route(
            &local_instance_id_for_config(&self.config),
            lookup.stack_parent,
            lookup.stack_parent_daemon_instance_id,
        ) {
            tddy_core::StackParentRoute::NoParent => Ok(None),
            tddy_core::StackParentRoute::Local => tddy_core::resolve_chain_base_ref(
                lookup.sessions_base,
                lookup.stack_parent,
                lookup.repo_root,
                lookup.stack_node_id,
                lookup.new_branch_name,
            )
            .map_err(Status::failed_precondition),
            tddy_core::StackParentRoute::OwnedByPeer { daemon_instance_id } => {
                let base_ref = ConnectionServiceTrait::resolve_stack_base(
                    self,
                    Request::new(ResolveStackBaseRequest {
                        session_token: lookup.session_token.to_string(),
                        daemon_instance_id,
                        stack_parent: lookup.stack_parent.unwrap_or_default().trim().to_string(),
                        project_id: lookup.project_id.trim().to_string(),
                        new_branch_name: lookup.new_branch_name.trim().to_string(),
                        // Travels with the question: the owner runs the same node-keyed lookup this
                        // daemon would, so a renamed branch does not lose the node one host over
                        // either (D34).
                        stack_node_id: lookup.stack_node_id.trim().to_string(),
                    }),
                )
                .await?
                .into_inner()
                .base_ref;
                Ok(Some(base_ref).filter(|b| !b.is_empty()))
            }
        }
    }

    /// Record on a pr-stack orchestrator's planned node the branch a child spawn just created, plus
    /// the child session as the fallback route back to that branch.
    ///
    /// The spawn paths already write the *reverse* link (`orchestrator_session_id` in the child's
    /// changeset). Without this forward link the node owns no branch, and the stack wedges:
    /// [`tddy_core::changeset::Stack::base_ref_for_spawn`] refuses every descendant ("non-merged
    /// parent … has no branch to base onto yet"), [`StackChildSpawnHandler`]'s duplicate-spawn guard
    /// never trips, and the orchestrator dashboard shows no child state or PR for a running child.
    ///
    /// Called once the child's branch exists (right after worktree setup) — that is precisely the
    /// condition `base_ref_for_spawn` gates descendants on. A new session claiming a branch a node
    /// already owns repoints the fallback to it (last writer wins): the branch is what the stack is
    /// built on, and sessions on it come and go (restart, re-attach) without changing that.
    ///
    /// Writes **this** daemon's sessions tree, and is therefore only correct where the orchestrator
    /// is this daemon's own session: the branch-derived lookup it performs needs the plan on disk.
    /// That is the orchestrator agent's own `spawn-child`, which always runs on the orchestrator's
    /// host and names no node. A spawn that *does* name one is routed to the owner instead
    /// ([`Self::record_spawn_on_stack_node`], D34/D35).
    pub(crate) fn link_stack_node_to_spawned_branch(
        sessions_base: &std::path::Path,
        stack_parent: Option<&str>,
        new_branch_name: &str,
        child_session_id: &str,
    ) -> Result<(), Status> {
        let Some(sp) = stack_parent else {
            return Ok(());
        };
        let Some((parent_dir, _stack, node_id)) =
            tddy_core::pr_stack_node_for_spawn(sessions_base, sp, "", new_branch_name)
        else {
            return Ok(());
        };
        tddy_core::changeset::link_stack_node_to_child_session(
            &parent_dir,
            &node_id,
            child_session_id,
            Some(new_branch_name.trim().to_string()),
        )
        .map_err(|e| {
            Status::internal(format!(
                "failed to link stack node '{node_id}' to child session {child_session_id}: {e}"
            ))
        })?;
        log::info!(
            target: "tddy_daemon::connection_service",
            "recorded branch '{}' on pr-stack node '{}' of orchestrator {} (child session {})",
            new_branch_name.trim(),
            node_id,
            sp,
            child_session_id
        );
        Ok(())
    }

    /// Record a spawn on the planned node it materializes, on the daemon that owns the
    /// orchestrator.
    ///
    /// Two paths, and which one applies is decided by whether the caller **named a node**:
    ///
    /// - A named node travels as a `LinkStackNode` call, taken through this daemon's own handler so
    ///   a forwarded link follows precisely the path a client's would — peer routing and refusals
    ///   included. This is the only path that works when the orchestrator lives one host over,
    ///   where its plan is not on this disk to be read or written at all (D35).
    /// - No node named keeps the branch-derived local write. That is the orchestrator agent's own
    ///   `spawn-child`, which runs in the orchestrator's own process on the orchestrator's host.
    ///
    /// The node is never derived from the branch on the routed path (D34): `new_branch_name` is the
    /// operator's to edit in the create dialog before confirming, so a rename would silently link
    /// nothing — or link the wrong node.
    pub(crate) async fn record_spawn_on_stack_node(
        &self,
        link: &stack_parent::StackNodeLink<'_>,
    ) -> Result<(), Status> {
        let node_id = link.node_id.trim();
        if node_id.is_empty() {
            return Self::link_stack_node_to_spawned_branch(
                link.sessions_base,
                Some(link.orchestrator_session_id),
                link.branch,
                link.child_session_id,
            );
        }
        ConnectionServiceTrait::link_stack_node(
            self,
            Request::new(LinkStackNodeRequest {
                session_token: link.session_token.to_string(),
                daemon_instance_id: link.orchestrator_daemon_instance_id.trim().to_string(),
                orchestrator_session_id: link.orchestrator_session_id.trim().to_string(),
                node_id: node_id.to_string(),
                child_session_id: link.child_session_id.trim().to_string(),
                branch: link.branch.trim().to_string(),
            }),
        )
        .await?;
        Ok(())
    }

    /// Handle `StartSession` for `session_type = "claude-cli"` sessions.
    ///
    /// Requires a valid, registered project. Creates a real git worktree under the project's
    /// main repo (via `tddy_core::setup_worktree_for_session_with_optional_chain_base`), then
    /// spawns the `claude` binary in a PTY.
    ///
    /// `initial_prompt` — when non-empty, passed as a positional argument to `claude` so it
    /// receives the first user turn on startup (e.g. `claude "build feature X"`).
    /// Resolve the goal a managed session should resume at: the goal persisted in `changeset.yaml`,
    /// falling back to the recipe's start goal when no meaningful state is recorded yet (empty or the
    /// default `Init`). Managed sessions persist a valid goal id on every committed transition.
    pub(crate) fn managed_resume_goal(
        session_dir: &Path,
        recipe: &Arc<dyn tddy_core::backend::WorkflowRecipe>,
    ) -> tddy_core::backend::GoalId {
        let persisted = tddy_core::read_changeset(session_dir)
            .ok()
            .map(|cs| cs.state.current.into_inner())
            .unwrap_or_default();
        let p = persisted.trim();
        if p.is_empty() || p == "Init" {
            recipe.start_goal()
        } else {
            tddy_core::backend::GoalId::new(p)
        }
    }

    /// Build the managed-workflow wiring for a claude-cli session and return the launch inputs: the
    /// [`ManagedWorkflow`](crate::session_toolcall::ManagedWorkflow) (its listener must be kept alive
    /// for the session's lifetime), the orchestration-prompt file path to append to claude's system
    /// prompt, and the per-session env (`TDDY_SOCKET` + a `PATH` that resolves `tddy-tools`) for the
    /// process that runs `tddy-tools transition`.
    ///
    /// `prompt_dir` is where the prompt file is written: the session dir for a non-sandboxed session,
    /// the jail-visible context dir for a sandboxed one. `resume_at` selects the controller's initial
    /// goal — `None` for a new session (recipe start goal), `Some` to resume an existing one.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_managed_workflow(
        &self,
        session_id: &str,
        recipe: Arc<dyn tddy_core::backend::WorkflowRecipe>,
        session_dir: &Path,
        worktree_path: &Path,
        prompt_dir: &Path,
        tddy_tools_path: &str,
        resume_at: Option<tddy_core::backend::GoalId>,
        conversation_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
    ) -> Result<ManagedLaunch, Status> {
        prepare_managed_workflow_inner(
            &self.tddy_data_dir,
            session_id,
            recipe,
            session_dir,
            worktree_path,
            prompt_dir,
            tddy_tools_path,
            resume_at,
            None,
            conversation_spawn_handler,
        )
    }

    /// The conflict a `StartSession` request must be answered with instead of creating anything, or
    /// `None` when creation may proceed.
    ///
    /// Fires only for an explicit `new_branch_from_base` with a non-empty `new_branch_name` and
    /// `on_branch_conflict = "reject"`:
    /// - a generated branch name (`claude-cli/<short-id>`, `workspace/<short-id>`) is derived from the
    ///   session uuid and cannot collide, so the empty intent is never checked;
    /// - `work_on_selected_branch` is the intent that deliberately joins an owned branch;
    /// - an empty `on_branch_conflict` keeps the suffixing behaviour every existing caller relies on.
    ///
    /// The project is resolved only once a conflict is established, so a request that may proceed
    /// keeps whatever error the session-type dispatch would have produced for its project.
    ///
    /// See docs/ft/daemon/session-branch-conflict.md.
    pub(crate) async fn owned_branch_conflict(
        &self,
        os_user: &str,
        req: &StartSessionRequest,
    ) -> Result<Option<BranchConflict>, Status> {
        if req.on_branch_conflict.trim() != "reject"
            || req.branch_worktree_intent.trim() != "new_branch_from_base"
        {
            return Ok(None);
        }
        let branch = req.new_branch_name.trim().to_string();
        if branch.is_empty() {
            return Ok(None);
        }

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let branch_for_scan = branch.clone();
        let owner = service_util::spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "StartSession: scan sessions by branch",
            move || {
                crate::branch_owner::find_session_owning_branch(&sessions_base, &branch_for_scan)
            },
        )
        .await?;
        let Some(owner) = owner else {
            return Ok(None);
        };

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, req.project_id.trim())
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let branch_for_suggestion = branch.clone();
        let suggested_branch_name = service_util::spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "StartSession: first free suffixed branch name",
            move || {
                Ok(tddy_core::worktree::first_free_suffixed_branch_name(
                    &repo_root,
                    &branch_for_suggestion,
                ))
            },
        )
        .await?;

        Ok(Some(BranchConflict {
            branch,
            owner: Some(BranchSession {
                exists: true,
                session_id: owner.session_id,
                is_active: owner.is_active,
                status: owner.status,
            }),
            suggested_branch_name,
        }))
    }
}
