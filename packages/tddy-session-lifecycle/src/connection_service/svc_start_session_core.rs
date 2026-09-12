use crate::{
    connection_service::{seed_codebase, service_util, stack_parent},
    project_storage, session_deletion, workspace_session,
};
use tddy_spawn::{
    spawn_worker,
    spawner::{self, SpawnOptions},
};

use super::recipe_enables_conversation_spawn;

use std::sync::Arc;

use super::AttachmentMaterialization;

use uuid::Uuid;

use tddy_core::session_lifecycle::unified_session_dir_path;

use std::path::Path;

use super::validate_stack_seed_base_session;

use crate::user_sessions_path::projects_path_for_user;

use super::CodebasePlacement;

use super::resolve_split_agent_placement;

use super::resolve_caller_chosen_session_id;

use super::classify_codebase_placement;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use super::AttachmentProgressSink;

use tddy_service::proto::session::StartSessionRequest;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// The one implementation behind both `StartSession` and `StreamStartSession`.
    ///
    /// `progress` is where attachment materialization reports to: the stream's sender for the
    /// streaming entry point, [`AttachmentProgressSink::discarding`] for the unary one. Nothing
    /// else differs between the two, so the unary path stays byte-for-byte what it was.
    pub(crate) async fn start_session_core(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // An agent id names either a coding backend from the config allowlist or an assistant in
        // this daemon's registry — the registry is a def source on equal footing with the YAML
        // (`resolvable_agent_defs`), so an assistant `ListAgents` offers must also be startable.
        let agent_trim = req.agent.trim();
        let agent_def = match agent_trim.is_empty() {
            true => None,
            false => self.agent_def_for_spawn(agent_trim, &github_user).await?,
        };
        if !agent_trim.is_empty() && agent_def.is_none() {
            let allowed = self.config.allowed_agents();
            if !allowed.is_empty() && !allowed.iter().any(|a| a.id == agent_trim) {
                return Err(Status::invalid_argument(format!(
                    "agent id {:?} is not listed in allowed_agents (configure daemon YAML) and is \
                     not an assistant in this daemon's registry",
                    agent_trim
                )));
            }
        }

        let requested_daemon = req.daemon_instance_id.trim();
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
        let eligible_ids: Vec<String> = eligible_rows
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        let route =
            match tddy_daemon_livekit::livekit_peer_discovery::classify_start_session_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Ok(r) => r,
                Err(msg) => {
                    log::info!("StartSession: rejected daemon routing: {}", msg);
                    return Err(Status::failed_precondition(msg));
                }
            };

        match route {
            tddy_daemon_livekit::livekit_peer_discovery::StartSessionPeerRoute::Forward {
                peer_instance_id,
            } => {
                log::info!(
                    "StartSession: forwarding RPC to remote daemon_instance_id={}",
                    peer_instance_id
                );
                let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                    Status::failed_precondition(
                        "cannot forward StartSession: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                    )
                })?;
                let inner =
                    tddy_daemon_livekit::livekit_peer_discovery::forward_start_session_via_livekit(
                        slot,
                        &peer_instance_id,
                        &req,
                    )
                    .await?;
                log::info!(
                    "StartSession: forward succeeded session_id={} livekit_server_identity={}",
                    inner.session_id,
                    inner.livekit_server_identity
                );
                return Ok(Response::new(inner));
            }
            tddy_daemon_livekit::livekit_peer_discovery::StartSessionPeerRoute::Local => {}
        }

        // The agent runs here; where its worktree goes is the second, independent placement. A
        // refused split is a malformed request, so it is classified before anything is created and
        // before the project is provisioned — a session whose codebase host is wrong should not
        // leave a clone behind on the way to being rejected.
        let placement = classify_codebase_placement(
            &local_id,
            &req.codebase_daemon_instance_id,
            &eligible_ids,
            req.managed_codebase,
            req.session_type.trim(),
        )
        .map_err(|msg| {
            log::info!("StartSession: rejected codebase placement: {msg}");
            Status::invalid_argument(msg)
        })?;

        // Checked here, alongside the other request-shape decisions, so a session type that does not
        // honour a caller-chosen id refuses it before anything is created rather than generating one
        // and leaving the caller pointing at a session that does not exist.
        let caller_chosen_session_id =
            resolve_caller_chosen_session_id(&req.requested_session_id, req.session_type.trim())?;
        let paired_agent = resolve_split_agent_placement(
            req.split_agent.as_ref(),
            req.session_type.trim(),
            req.agent_clone.is_some(),
        )?;

        // Validate cheap, session-type-specific inputs before the (potentially expensive) project
        // auto-provision below: claude-cli always requires a model, so reject an empty one up front
        // — a bad request should fail fast with INVALID_ARGUMENT, not a project NotFound. The
        // per-session-type handlers re-check as defense-in-depth (and for the resume/child paths).
        if req.session_type.trim() == "claude-cli" && req.model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }
        if req.session_type.trim() == "cursor-cli" && req.model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for cursor-cli sessions",
            ));
        }

        // A split session has no repository here, so it skips the project auto-provision below and
        // the whole worktree-bearing dispatch: the codebase host resolves the project against its
        // own filesystem.
        if let CodebasePlacement::Split {
            codebase_instance_id,
        } = &placement
        {
            return self
                .start_split_claude_cli_session(os_user, codebase_instance_id, &req, progress)
                .await;
        }

        // Auto-provision the project's working copy on this host before dispatching to any session
        // type: if the project isn't cloned here yet (registered-but-missing, or known only on a
        // peer), clone it into the host's base location so the session can start on a host that
        // doesn't have the project yet. A truly unknown project surfaces as NotFound.
        {
            let project_id = req.project_id.trim();
            if !project_id.is_empty() {
                let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve projects path"))?;
                self.ensure_project_available_for_start(
                    os_user,
                    &projects_dir,
                    project_id,
                    &req.session_token,
                    req.agent_clone.as_ref(),
                )
                .await?;
            }
        }

        // A base session that cannot seed a stack is refused here, before the session-type dispatch
        // and before anything is created, so the new-session form can show the reason in its error
        // strip rather than navigating away from a session that came up unseeded.
        if !req.pr_stack_base_session_id.trim().is_empty() {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            // The requesting project's repository, which the base session's must be. Resolved here
            // rather than reusing the tool branch's lookup further down, because the whole value of
            // this refusal is that it happens before any of that runs. A seeded orchestrator without
            // a project has no repository to be scoped to, so it is refused rather than exempted.
            let project_id = req.project_id.trim();
            if project_id.is_empty() {
                return Err(Status::invalid_argument(
                    "project_id is required to seed a PR stack from a base session",
                ));
            }
            let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve projects path"))?;
            let project = project_storage::find_project(&projects_dir, project_id)
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("project not found"))?;
            validate_stack_seed_base_session(
                &sessions_base,
                &req.recipe,
                &req.pr_stack_base_session_id,
                Path::new(&project.main_repo_path),
            )
            .map_err(tddy_service::to_rpc_status)?;
        }

        // A requested new branch another session already owns is refused here, before the
        // session-type dispatch — so one check covers tool, claude-cli, cursor-cli and workspace, and
        // so nothing has been created yet when it fires.
        if let Some(conflict) = self.owned_branch_conflict(os_user, &req).await? {
            log::info!(
                "StartSession: refusing branch {:?} owned by session {}",
                conflict.branch,
                conflict
                    .owner
                    .as_ref()
                    .map(|o| o.session_id.as_str())
                    .unwrap_or_default()
            );
            return Ok(Response::new(StartSessionResponse {
                branch_conflict: Some(conflict),
                ..Default::default()
            }));
        }

        // --- workspace branch: no LiveKit, no PTY; resolves project, creates a git worktree ---
        if req.session_type.trim() == "workspace" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = match caller_chosen_session_id {
                // Creating a session over an existing one would overwrite its `.session.yaml` and
                // leave that session's worktree with nothing pointing at it, so a taken id is
                // refused rather than reused.
                Some(chosen) => {
                    if unified_session_dir_path(&sessions_base, &chosen).exists() {
                        return Err(Status {
                            code: tddy_rpc::Code::AlreadyExists,
                            message: format!(
                                "requested_session_id {chosen:?} already names a session on this daemon"
                            ),
                        });
                    }
                    chosen
                }
                None => Uuid::now_v7().to_string(),
            };
            self.prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user,
                sessions_base: &sessions_base,
                session_id: &session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
            let timeout = self.config.spawn_worker_request_timeout();
            // The room serves this very service: a participant of a session room reaches the same
            // `ExecuteTool` / `ReadHostDocument` surface a caller reaches over the common room or
            // HTTP, on the same daemon, rooted at the checkout it just made.
            // An agent clone is a workspace session with a job: its checkout is a mirror rather than
            // a branch to work on, so it is cut differently, and it then joins the facilitating
            // daemon's session room and keeps itself equal to that session's worktree. Readiness is
            // reported from there, because only this daemon can say when the mirror has caught up.
            let Some(placement) = req.agent_clone.clone() else {
                // Resolved before the worktree is cut: an agent reference that names nothing is a
                // request error, and refusing one after the checkout existed would mean tearing a
                // worktree down to report a typo. This is also the host that must resolve them —
                // for the codebase half of a split session the roster lives here, so "co-located"
                // means "owned by this daemon" and an agent of any other host is the one that needs
                // a clone (docs/ft/daemon/session-agent-roster.md § Remote agents).
                let seed = self.seeded_roster_records(&req.specialized_agents).await?;
                let started = workspace_session::start_workspace_session(
                    os_user,
                    &session_id,
                    sessions_base.clone(),
                    req.project_id.trim(),
                    &workspace_session::WorkspaceBranchIntent {
                        branch_worktree_intent: req.branch_worktree_intent.trim(),
                        new_branch_name: req.new_branch_name.trim(),
                        selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                        selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
                    },
                    paired_agent.as_ref(),
                    req.sandbox,
                    &self.tddy_data_dir,
                    timeout,
                )
                .await?;
                // Written before this call answers, because the answer is what releases the agent
                // host to spawn its agent — and that spawn fixes the tool allowlist at launch. A
                // roster written afterwards would leave a seeded agent's `replaces` unenforced until
                // the first resume. The seed takes its own artifacts back out on failure; the
                // session it was recorded on is the caller's to reclaim, which is what the split
                // start's teardown does with the id it minted.
                let session_dir = unified_session_dir_path(&sessions_base, &session_id);
                let codebase = seed_codebase::SeedCodebase::read(&session_id, &session_dir)?;
                let seeded = self
                    .seed_session_agent_roster(&session_id, &codebase, &req.session_token, seed)
                    .await?;
                if req.semantic_index {
                    // Unwound here rather than inside the seed, because the seed cannot see this
                    // step: a start that answers with an error but leaves its roster behind leaves
                    // an agent the operator can see on a session that never came up, holding a
                    // withdrawal against a main agent that was never spawned — and, for a
                    // peer-owned seed, a claimed clone on that peer with nothing left to release
                    // it.
                    if let Err(status) = self
                        .index_workspace_worktree(&sessions_base, &session_id)
                        .await
                    {
                        self.unwind_seeded_roster(
                            &session_id,
                            &codebase,
                            &req.session_token,
                            seeded,
                        )
                        .await;
                        return Err(status);
                    }
                }
                // The jail is built last, and deliberately after the index: indexing reads the host
                // worktree directly, so it belongs before a jail exists rather than through one.
                if req.sandbox {
                    if let Err(status) = self
                        .provision_workspace_tool_sandbox(&sessions_base, &session_id)
                        .await
                    {
                        // The failed index's unwind, plus the session directory itself: a session
                        // surviving a start that answered with an error is one the operator can
                        // see, list and resume, whose tools were never confined.
                        self.unwind_seeded_roster(
                            &session_id,
                            &codebase,
                            &req.session_token,
                            seeded,
                        )
                        .await;
                        let projects_dir =
                            projects_path_for_user(os_user, Some(&self.tddy_data_dir));
                        if let Err(e) = session_deletion::delete_session_directory(
                            &sessions_base,
                            &session_id,
                            projects_dir.as_deref(),
                        ) {
                            log::warn!(
                                "StartSession: could not remove session {session_id} after its \
                                 jail could not be provisioned: {}",
                                e.message()
                            );
                        }
                        return Err(status);
                    }
                }
                return Ok(started);
            };
            let started = workspace_session::start_agent_clone_session(
                os_user,
                &session_id,
                sessions_base.clone(),
                req.project_id.trim(),
                &self.tddy_data_dir,
                timeout,
            )
            .await?;
            self.start_hosted_agent_clone(
                &placement,
                &sessions_base,
                &session_id,
                req.project_id.trim(),
                &req.session_token,
            )
            .await?;
            return Ok(started);
        }

        // --- claude-cli branch: no LiveKit; resolves project and creates a real git worktree ---
        if req.session_type.trim() == "claude-cli" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
            let materialized = self
                .prepare_session_attachments(&AttachmentMaterialization {
                    session_token: &req.session_token,
                    os_user,
                    sessions_base: &sessions_base,
                    session_id: &session_id,
                    attachments: &req.attachments,
                    progress,
                })
                .await?;
            // A child of a planned PR is told where its boundaries are, however it was started: the
            // dialog opens with the node's documents pre-attached but carries only the node's title
            // and description as the prompt, so the line is added here rather than in the browser.
            // Derived from what materialized, so it can only name a document the child holds.
            let initial_prompt = crate::stack_doc_attachments::prompt_with_attached_changeset(
                req.initial_prompt.trim(),
                &materialized,
            );
            let stack_parent_for_claude_cli: Option<String> = {
                let t = req.stack_parent.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            };
            // A managed-codebase claude-cli session with a recipe is launched workflow-aware. An
            // unknown recipe is a request error (never silently ignored). Non-managed sessions and
            // managed sessions without a recipe keep the plain launch (managed_recipe = None).
            let managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>> = if req
                .managed_codebase
                && !req.recipe.trim().is_empty()
            {
                Some(
                    tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(req.recipe.trim())
                        .map_err(Status::invalid_argument)?,
                )
            } else {
                None
            };

            if req.sandbox {
                return self
                    .start_sandboxed_claude_cli_session(
                        os_user,
                        &session_id,
                        &req.session_token,
                        sessions_base,
                        req.model.trim(),
                        req.project_id.trim(),
                        req.repo_path.trim(),
                        req.branch_worktree_intent.trim(),
                        req.new_branch_name.trim(),
                        req.selected_integration_base_ref.trim(),
                        req.selected_branch_to_work_on.trim(),
                        &initial_prompt,
                        &req.claude_args,
                        req.permission_mode.trim(),
                        req.dangerously_skip_permissions,
                        stack_parent_for_claude_cli.as_deref(),
                        req.stack_parent_daemon_instance_id.trim(),
                        req.stack_node_id.trim(),
                        req.managed_codebase,
                        &req.specialized_agents,
                        managed_recipe,
                        req.semantic_index,
                        req.create_remote_branch,
                    )
                    .await;
            }
            return self
                .start_claude_cli_session(
                    os_user,
                    &session_id,
                    sessions_base,
                    req.model.trim(),
                    req.project_id.trim(),
                    req.branch_worktree_intent.trim(),
                    req.new_branch_name.trim(),
                    req.selected_integration_base_ref.trim(),
                    req.selected_branch_to_work_on.trim(),
                    &initial_prompt,
                    req.permission_mode.trim(),
                    req.dangerously_skip_permissions,
                    stack_parent_for_claude_cli.as_deref(),
                    req.stack_parent_daemon_instance_id.trim(),
                    req.stack_node_id.trim(),
                    &req.session_token,
                    managed_recipe,
                    req.semantic_index,
                    req.create_remote_branch,
                )
                .await;
        }

        // --- cursor-cli branch: no LiveKit; spawns Cursor Agent CLI in a PTY worktree ---
        if req.session_type.trim() == "cursor-cli" {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            let session_id = Uuid::now_v7().to_string();
            let materialized = self
                .prepare_session_attachments(&AttachmentMaterialization {
                    session_token: &req.session_token,
                    os_user,
                    sessions_base: &sessions_base,
                    session_id: &session_id,
                    attachments: &req.attachments,
                    progress,
                })
                .await?;
            // Same rule as the claude-cli branch above: which agent runs a planned PR's child is
            // not a reason for it to come up without its boundaries.
            let initial_prompt = crate::stack_doc_attachments::prompt_with_attached_changeset(
                req.initial_prompt.trim(),
                &materialized,
            );
            let managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>> = if req
                .managed_codebase
                && !req.recipe.trim().is_empty()
            {
                Some(
                    tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(req.recipe.trim())
                        .map_err(Status::invalid_argument)?,
                )
            } else {
                None
            };
            if req.sandbox {
                return self
                    .start_sandboxed_cursor_cli_session(
                        os_user,
                        &session_id,
                        &req.session_token,
                        sessions_base,
                        req.model.trim(),
                        req.project_id.trim(),
                        req.branch_worktree_intent.trim(),
                        req.new_branch_name.trim(),
                        req.selected_integration_base_ref.trim(),
                        req.selected_branch_to_work_on.trim(),
                        Some(req.stack_parent.trim()).filter(|s| !s.is_empty()),
                        req.stack_parent_daemon_instance_id.trim(),
                        req.stack_node_id.trim(),
                        &initial_prompt,
                        req.managed_codebase,
                        &req.specialized_agents,
                        managed_recipe,
                        req.semantic_index,
                        req.create_remote_branch,
                    )
                    .await;
            }
            // Resolved before the spawn, not after: an agent the request names and this daemon
            // cannot resolve fails the start, exactly as it does on the sandboxed paths, rather
            // than persisting a roster entry that resolves to nothing on the next resume.
            let mut started_agents = self.seeded_roster_records(&req.specialized_agents).await?;
            let clones = self.seed_clone_claimant();
            return crate::cursor_cli_spawn::spawn_cursor_cli_session_inner(
                &self.config,
                &self.tddy_data_dir,
                &self.claude_cli_manager,
                os_user,
                &session_id,
                &req.session_token,
                sessions_base,
                req.model.trim(),
                req.project_id.trim(),
                req.branch_worktree_intent.trim(),
                req.new_branch_name.trim(),
                req.selected_integration_base_ref.trim(),
                req.selected_branch_to_work_on.trim(),
                req.repo_path.trim(),
                match Some(req.stack_parent.trim()).filter(|s| !s.is_empty()) {
                    Some(session_id) => stack_parent::SpawnStackParent::OwnedBy {
                        session_id,
                        daemon_instance_id: req.stack_parent_daemon_instance_id.trim(),
                        stack_node_id: req.stack_node_id.trim(),
                        session_token: &req.session_token,
                        host: self,
                    },
                    None => stack_parent::SpawnStackParent::NoParent,
                },
                &initial_prompt,
                req.managed_codebase,
                &mut started_agents,
                managed_recipe,
                req.semantic_index,
                req.create_remote_branch,
                &self.task_registry,
                &clones,
            )
            .await;
        }

        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;

        let project_id_req = req.project_id.trim();
        if project_id_req.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id_req)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;

        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        log::debug!("StartSession: entering spawn_blocking session_id=new");
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let os_user = os_user.to_string();
        // The spawn closure below takes ownership; the presenter observer started afterwards needs
        // the same user to resolve the session's label from its sessions directory.
        let observer_os_user = os_user.clone();
        let tool_path = req.tool_path.clone();
        let tddy_data_dir_for_spawn = self.tddy_data_dir.clone();
        let repo_path = repo_path.to_path_buf();
        let livekit = livekit.clone();
        let pid_for_spawn = project.project_id.clone();
        let agent_for_spawn: Option<String> = {
            let t = req.agent.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        // A spawned `tddy-coder` resolves `--agent` against the builtins and `<tddyhome>/agents`
        // only; this daemon's registry is a source it cannot read. So the def this daemon already
        // resolved travels with the spawn as `--agent-def`, and the child creates its backend from
        // that rather than falling through to a different agent entirely.
        let agent_def_for_spawn: Option<String> = agent_def
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| Status::internal(format!("failed to serialize agent def: {e}")))?;
        let recipe_for_spawn: Option<String> = {
            let t = req.recipe.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let stack_parent_for_spawn: Option<String> = {
            let t = req.stack_parent.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        // The planned node the surface that rendered Start-session named. Carried to the child as
        // `--stack-node-id`, which is what puts the association in its participant metadata (D37).
        let stack_node_id_for_spawn: Option<String> = {
            let t = req.stack_node_id.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        // Already validated above; the orchestrator's own process is what seeds the stack, because
        // the session that owns a `changeset.yaml` is the process that writes it.
        let stack_seed_base_session_for_spawn: Option<String> = {
            let t = req.pr_stack_base_session_id.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let model_for_spawn: Option<String> = {
            let t = req.model.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let timeout = self.config.spawn_worker_request_timeout();
        let daemon_log = self.config.log.clone();
        let startup_watch = spawner::StartupWatch::from_config(&self.config);
        let coder_config_path = self.config.coder_config_path.clone();
        // Grill-me tool sessions relay `spawn_conversation` back over a per-session unix socket.
        // Because the coder needs the socket path (and orchestrator id) at spawn time — and the
        // socket path is what crosses the forked `spawn_worker` boundary — bind it and pre-generate
        // the session id BEFORE the spawn, so both the worker and direct paths carry it identically.
        let enable_conversation_spawn = recipe_for_spawn
            .as_deref()
            .map(recipe_enables_conversation_spawn)
            .unwrap_or(false);
        let (mut pre_session_id, host_session_socket): (Option<String>, Option<String>) =
            if enable_conversation_spawn {
                let sid = Uuid::now_v7().to_string();
                let sock = self
                    .spawn_host_session_socket(
                        &sid,
                        &os_user,
                        &pid_for_spawn,
                        model_for_spawn.clone(),
                    )
                    .await;
                (Some(sid), sock)
            } else {
                (None, None)
            };
        let tool_session_id = pre_session_id
            .clone()
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        if enable_conversation_spawn || !req.attachments.is_empty() {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                &os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            self.prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user: &os_user,
                sessions_base: &sessions_base,
                session_id: &tool_session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
            pre_session_id = Some(tool_session_id);
        }
        let result = match tddy_spawn::supervisor_client::spawn_backend_choice(&self.config) {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                let coder_log_yaml = spawner::coder_log_config_yaml(coder_config_path.as_deref());
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        new_session_id: pre_session_id.as_deref(),
                        project_id: Some(pid_for_spawn.as_str()),
                        agent: agent_for_spawn.as_deref(),
                        agent_def_json: agent_def_for_spawn.as_deref(),
                        mouse: spawn_mouse,
                        recipe: recipe_for_spawn.as_deref(),
                        stack_parent: stack_parent_for_spawn.as_deref(),
                        stack_node_id: stack_node_id_for_spawn.as_deref(),
                        stack_seed_base_session: stack_seed_base_session_for_spawn.as_deref(),
                        model: model_for_spawn.as_deref(),
                        host_session_socket: host_session_socket.as_deref(),
                    },
                    daemon_log.as_ref(),
                    coder_log_yaml,
                    startup_watch,
                );
                service_util::await_supervised_with_timeout(
                    timeout,
                    "StartSession: spawn via tddy-supervisor",
                    tddy_spawn::supervisor_spawn::spawn_session_via_supervisor(
                        &socket_path,
                        &spawn_req,
                    ),
                )
                .await?
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                service_util::spawn_blocking_with_timeout(
                    timeout,
                    "StartSession: spawn",
                    move || {
                        log::debug!(
                            "StartSession: spawn_blocking running, using_spawn_worker={}",
                            spawn_client.is_some()
                        );
                        let pid = Some(pid_for_spawn.as_str());
                        let agent = agent_for_spawn.as_deref();
                        let agent_def = agent_def_for_spawn.as_deref();
                        let recipe = recipe_for_spawn.as_deref();
                        let stack_parent = stack_parent_for_spawn.as_deref();
                        let stack_node_id = stack_node_id_for_spawn.as_deref();
                        let stack_seed_base_session = stack_seed_base_session_for_spawn.as_deref();
                        let model = model_for_spawn.as_deref();
                        let new_session_id = pre_session_id.as_deref();
                        let host_socket = host_session_socket.as_deref();
                        let coder_log_yaml =
                            spawner::coder_log_config_yaml(coder_config_path.as_deref());
                        if let Some(ref client) = spawn_client {
                            let spawn_req = spawn_worker::build_spawn_request(
                                &os_user,
                                &tool_path,
                                &tddy_data_dir_for_spawn,
                                &repo_path,
                                &livekit,
                                SpawnOptions {
                                    resume_session_id: None,
                                    new_session_id,
                                    project_id: pid,
                                    agent,
                                    agent_def_json: agent_def,
                                    mouse: spawn_mouse,
                                    recipe,
                                    stack_parent,
                                    stack_node_id,
                                    stack_seed_base_session,
                                    model,
                                    host_session_socket: host_socket,
                                },
                                daemon_log.as_ref(),
                                coder_log_yaml,
                                startup_watch,
                            );
                            client.spawn(spawn_req)
                        } else {
                            let (child_log_level, child_log_format) =
                                spawner::child_log_yaml_tuning(daemon_log.as_ref());
                            spawner::spawn_as_user(
                                &os_user,
                                &tool_path,
                                &tddy_data_dir_for_spawn,
                                &repo_path,
                                &livekit,
                                SpawnOptions {
                                    resume_session_id: None,
                                    new_session_id,
                                    project_id: pid,
                                    agent,
                                    agent_def_json: agent_def,
                                    mouse: spawn_mouse,
                                    recipe,
                                    stack_parent,
                                    stack_node_id,
                                    stack_seed_base_session,
                                    model,
                                    host_session_socket: host_socket,
                                },
                                child_log_level.as_str(),
                                child_log_format.as_str(),
                                coder_log_yaml.as_deref(),
                                startup_watch,
                            )
                        }
                    },
                )
                .await?
            }
        };
        log::debug!(
            "StartSession: spawn returned, session_id={}",
            result.session_id
        );
        self.maybe_spawn_presenter_observer(
            &observer_os_user,
            &result.session_id,
            result.grpc_port,
        );
        Ok(Response::new(StartSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
            branch_conflict: None,
        }))
    }
}
