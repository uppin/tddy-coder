use std::path::PathBuf;

use crate::{
    connection_service::{service_util, stack_parent},
    project_storage,
    user_sessions_path::projects_path_for_user,
};

use tddy_service::proto::types::BranchSession;

use tddy_service::proto::session::BranchConflict;

use tddy_service::proto::session::StartSessionRequest;

use super::prepare_managed_workflow_inner;

use super::ManagedLaunch;

use std::sync::Arc;

use tddy_service::proto::pr_stack::LinkStackNodeRequest;

use tddy_service::proto::pr_stack::ResolveStackBaseRequest;

use tddy_rpc::Request;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_rpc::Status;

use std::path::Path;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Resolve what a spawn's worktree is cut from, on the daemon that owns its `stack_parent`.
    ///
    /// The parent's base is read out of the parent's own `changeset.yaml` — its stack, or its
    /// branch — so only the daemon holding that session can produce it. A parent this daemon owns
    /// (no host named, or its own) is resolved straight off its disk, exactly as before. A parent
    /// another daemon owns travels as a `ResolveStackBase` call, taken through this daemon's own
    /// PR-stack handler so a forwarded resolution follows precisely the path a client's would, peer
    /// routing and refusals included. That handler lives above this crate and is reached through
    /// the [`DaemonRpcFamilies`](crate::DaemonRpcFamilies) port, so a host never given it refuses
    /// with `FAILED_PRECONDITION` rather than resolve the parent off the wrong disk.
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
                let base_ref = self
                    .rpc_families()?
                    .pr_stack_handler()
                    .resolve_stack_base(Request::new(ResolveStackBaseRequest {
                        session_token: lookup.session_token.to_string(),
                        daemon_instance_id,
                        stack_parent: lookup.stack_parent.unwrap_or_default().trim().to_string(),
                        project_id: lookup.project_id.trim().to_string(),
                        new_branch_name: lookup.new_branch_name.trim().to_string(),
                        // Travels with the question: the owner runs the same node-keyed lookup this
                        // daemon would, so a renamed branch does not lose the node one host over
                        // either (D34).
                        stack_node_id: lookup.stack_node_id.trim().to_string(),
                    }))
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
    /// - A named node travels as a `LinkStackNode` call, taken through this daemon's own PR-stack
    ///   handler — reached through the [`DaemonRpcFamilies`](crate::DaemonRpcFamilies) port — so a
    ///   forwarded link follows precisely the path a client's would, peer routing and refusals
    ///   included. This is the only path that works when the orchestrator lives one host over,
    ///   where its plan is not on this disk to be read or written at all (D35). A host never given
    ///   the port refuses with `FAILED_PRECONDITION` rather than skip the link.
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
        self.rpc_families()?
            .pr_stack_handler()
            .link_stack_node(Request::new(LinkStackNodeRequest {
                session_token: link.session_token.to_string(),
                daemon_instance_id: link.orchestrator_daemon_instance_id.trim().to_string(),
                orchestrator_session_id: link.orchestrator_session_id.trim().to_string(),
                node_id: node_id.to_string(),
                child_session_id: link.child_session_id.trim().to_string(),
                branch: link.branch.trim().to_string(),
            }))
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
        recipe: &Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>,
    ) -> tddy_core::workflow::ids::GoalId {
        let persisted = tddy_core::read_changeset(session_dir)
            .ok()
            .map(|cs| cs.state.current.into_inner())
            .unwrap_or_default();
        let p = persisted.trim();
        if p.is_empty() || p == "Init" {
            recipe.start_goal()
        } else {
            tddy_core::workflow::ids::GoalId::new(p)
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
        recipe: Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>,
        session_dir: &Path,
        worktree_path: &Path,
        prompt_dir: &Path,
        tddy_tools_path: &str,
        resume_at: Option<tddy_core::workflow::ids::GoalId>,
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
                crate::branch_owner::find_session_owning_branch(
                    &crate::session_reader::DaemonSessionListing,
                    &sessions_base,
                    &branch_for_scan,
                )
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
