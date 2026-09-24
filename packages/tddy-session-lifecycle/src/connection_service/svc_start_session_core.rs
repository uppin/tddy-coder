use crate::connection_service::service_util;
use tddy_spawn::spawner;

use std::sync::Arc;

use super::AttachmentMaterialization;

use uuid::Uuid;

use tddy_core::session_lifecycle::unified_session_dir_path;

use super::CodebasePlacement;

use super::resolve_split_agent_placement;

use super::resolve_caller_chosen_session_id;

use super::{classify_placement, PlacementRequest};

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use super::AttachmentProgressSink;

use tddy_service::proto::session::StartSessionRequest;

use super::DaemonSessionHost;

use tddy_daemon_kernel::trim_to_option;

mod tool_spawn_plan;
pub(in crate::connection_service) use tool_spawn_plan::*;

/// What a CLI-agent start holds once its prelude has run: where the session lives, the id it was
/// given, and the first prompt, with any attached changeset named in it.
struct CliStart {
    sessions_base: std::path::PathBuf,
    session_id: String,
    initial_prompt: String,
}

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
        let os_user = &self
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
        let (local_id, eligible_ids) = self.eligible_daemon_ids();
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
                let inner = self.forward_start_session(&req, peer_instance_id).await?;
                return Ok(Response::new(inner));
            }
            tddy_daemon_livekit::livekit_peer_discovery::StartSessionPeerRoute::Local => {}
        }

        // The agent runs here; where its worktree goes is the second, independent placement. A
        // refused split is a malformed request, so it is classified before anything is created and
        // before the project is provisioned — a session whose codebase host is wrong should not
        // leave a clone behind on the way to being rejected.
        let placement = classify_start_placement(&req, local_id, eligible_ids)?;

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

        // Neither placement below puts a repository where the agent runs, so both skip the project
        // auto-provision and the whole worktree-bearing dispatch that follows: a split resolves the
        // project on the codebase host's filesystem, and a jailed codebase resolves it on this one
        // but inside a `workspace` session of its own.
        match &placement {
            CodebasePlacement::Split {
                codebase_instance_id,
            } => {
                return self
                    .start_split_claude_cli_session(os_user, codebase_instance_id, &req, progress)
                    .await;
            }
            CodebasePlacement::SandboxedCodebase => {
                return self
                    .start_sandboxed_codebase_session(os_user, &req, progress)
                    .await;
            }
            CodebasePlacement::CoLocated => {}
        }

        // Auto-provision the project's working copy on this host before dispatching to any session
        // type: if the project isn't cloned here yet (registered-but-missing, or known only on a
        // peer), clone it into the host's base location so the session can start on a host that
        // doesn't have the project yet. A truly unknown project surfaces as NotFound.
        {
            self.provision_project_for_start(&req, os_user).await?;
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
            self.validate_stack_seed_against_project(&req, os_user, sessions_base, project_id)?;
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
                let (started, codebase, seeded) = self
                    .seed_and_start_workspace_session(
                        &req,
                        os_user,
                        paired_agent,
                        &sessions_base,
                        &session_id,
                        timeout,
                    )
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
                        self.remove_unconfined_workspace_session(
                            &req,
                            os_user,
                            &sessions_base,
                            &session_id,
                            codebase,
                            seeded,
                        )
                        .await;
                        return Err(status);
                    }
                }
                return Ok(started);
            };
            let started = self
                .start_agent_clone_workspace_session(
                    &req,
                    os_user,
                    sessions_base,
                    session_id,
                    timeout,
                    placement,
                )
                .await?;
            return Ok(started);
        }

        // --- claude-cli branch: no LiveKit; resolves project and creates a real git worktree ---
        if req.session_type.trim() == "claude-cli" {
            // A child of a planned PR is told where its boundaries are, however it was started: the
            // dialog opens with the node's documents pre-attached but carries only the node's title
            // and description as the prompt, so the line is added here rather than in the browser.
            // Derived from what materialized, so it can only name a document the child holds.
            let start = self.cli_start_prelude(&req, progress, os_user).await?;
            let stack_parent_for_claude_cli = trim_to_option(&req.stack_parent);
            // A managed-codebase claude-cli session with a recipe is launched workflow-aware. An
            // unknown recipe is a request error (never silently ignored). Non-managed sessions and
            // managed sessions without a recipe keep the plain launch (managed_recipe = None).
            let managed_recipe = managed_recipe_for(&req)?;
            if req.sandbox {
                return self
                    .start_sandboxed_claude_cli_from_request(
                        &req,
                        os_user,
                        start,
                        stack_parent_for_claude_cli.as_deref(),
                        managed_recipe,
                    )
                    .await;
            }
            return self
                .start_claude_cli_from_request(
                    &req,
                    os_user,
                    start,
                    stack_parent_for_claude_cli,
                    managed_recipe,
                )
                .await;
        }

        // --- cursor-cli branch: no LiveKit; spawns Cursor Agent CLI in a PTY worktree ---
        if req.session_type.trim() == "cursor-cli" {
            // Same rule as the claude-cli branch above: which agent runs a planned PR's child is
            // not a reason for it to come up without its boundaries.
            let start = self.cli_start_prelude(&req, progress, os_user).await?;
            let managed_recipe = managed_recipe_for(&req)?;
            if req.sandbox {
                return self
                    .start_sandboxed_cursor_cli_from_request(&req, os_user, start, managed_recipe)
                    .await;
            }
            // Resolved before the spawn, not after: an agent the request names and this daemon
            // cannot resolve fails the start, exactly as it does on the sandboxed paths, rather
            // than persisting a roster entry that resolves to nothing on the next resume.
            let started_agents = self.seeded_roster_records(&req.specialized_agents).await?;
            let clones = self.seed_clone_claimant();
            return self
                .spawn_cursor_cli_from_request(
                    &req,
                    os_user,
                    start,
                    managed_recipe,
                    started_agents,
                    clones,
                )
                .await;
        }

        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;

        let project_id_req = req.project_id.trim();
        if project_id_req.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let (_, project) =
            service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id_req)?;
        service_util::project_repo_root(&project)?;

        let result = self
            .spawn_tool_session(req, progress, os_user, agent_def, livekit, &project)
            .await?;
        Ok(Response::new(StartSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
            branch_conflict: None,
        }))
    }
}

mod start_request_checks;

mod workspace_branch_start;

mod cli_branch_starts;

mod tool_session_spawn;

fn classify_start_placement(
    req: &StartSessionRequest,
    local_id: String,
    eligible_ids: Vec<String>,
) -> Result<CodebasePlacement, Status> {
    let placement = classify_placement(&PlacementRequest {
        local_instance_id: local_id.clone(),
        requested_codebase_id: req.codebase_daemon_instance_id.clone(),
        eligible_ids: eligible_ids.clone(),
        managed_codebase: req.managed_codebase,
        sandbox: req.sandbox,
        sandboxed_codebase: req.sandboxed_codebase,
        session_type: req.session_type.trim().to_string(),
        recipe: req.recipe.clone(),
        dangerously_skip_permissions: req.dangerously_skip_permissions,
    })
    .map_err(|msg| {
        log::info!("StartSession: rejected codebase placement: {msg}");
        Status::invalid_argument(msg)
    })?;
    Ok(placement)
}

fn managed_recipe_for(
    req: &StartSessionRequest,
) -> Result<Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>, Status> {
    let managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>> =
        if req.managed_codebase && !req.recipe.trim().is_empty() {
            Some(
                tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(req.recipe.trim())
                    .map_err(Status::invalid_argument)?,
            )
        } else {
            None
        };
    Ok(managed_recipe)
}
