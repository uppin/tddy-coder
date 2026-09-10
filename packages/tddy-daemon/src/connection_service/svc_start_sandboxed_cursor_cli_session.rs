use super::DaemonRpcHandler;
use std::sync::Mutex as StdMutex;

use tddy_task::TerminalCapture;

use super::roster_replacement_pairs;

use tddy_core::Changeset;

use crate::{
    branch_intent::BranchIntentPolicy,
    connection_service::{agent_roster, seed_codebase, service_util, stack_parent},
    project_storage,
};

use crate::branch_intent::BranchIntentRequest;

use crate::branch_intent::resolve_branch_workflow;

use crate::branch_intent::ResolvedBranchWorkflow;

use tddy_core::output::SESSIONS_SUBDIR;

use crate::user_sessions_path::projects_path_for_user;

use tddy_rpc::Status;

use tddy_service::proto::connection::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use std::path::PathBuf;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Handle `StartSession` for sandboxed `cursor-cli` sessions (darwin Seatbelt / Linux cgroups).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_sandboxed_cursor_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        // Authorizes the `workspace` starts this session's seeded agents need on their own hosts
        // (see `provision_agent_clone`); the peer sees the same token the client presented here.
        session_token: &str,
        sessions_base: PathBuf,
        model: &str,
        project_id: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        stack_parent: Option<&str>,
        // The daemon whose sessions tree holds `stack_parent` (empty = this one).
        stack_parent_daemon_instance_id: &str,
        // The planned node of that orchestrator's stack this spawn materializes, as the surface
        // that started it named it. Preferred over the branch when the base is resolved (D34).
        stack_node_id: &str,
        initial_prompt: &str,
        _managed_codebase: bool,
        specialized_agents: &[String],
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, index the worktree before launch (blocking; aborts on failure) and point the
        // in-jail `SemanticSearch` tool at the per-session index.
        semantic_index: bool,
        // When true (new_branch_from_base only), push the new branch to origin at session start.
        create_remote_branch: bool,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for cursor-cli sessions",
            ));
        }
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument(
                "project_id is required for cursor-cli sessions",
            ));
        }
        // As on the sandboxed claude-cli path: the roster is resolved before anything is created,
        // and the defs it holds locally are what the jail env can carry.
        let mut started_agents = self.seeded_roster_records(specialized_agents).await?;
        let specialized_defs = self
            .resolve_specialized_agent_defs(specialized_agents)
            .await?;

        // Readiness gate: wake every specialized agent's endpoint and wait until each answers
        // before spawning the jail, so a cold/unreachable model fails session start here rather
        // than stalling the main agent's first subagent call. No fallback — the jail is never
        // spawned if warm-up fails. Resume gates separately, in `relaunch_sandboxed_runner`.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        if !repo_root.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        let ResolvedBranchWorkflow {
            intent,
            workflow: cs_workflow,
        } = resolve_branch_workflow(
            session_id,
            &BranchIntentRequest {
                branch_worktree_intent,
                new_branch_name,
                selected_integration_base_ref,
                selected_branch_to_work_on,
            },
            BranchIntentPolicy::cursor_cli(),
            project.main_branch_ref.as_deref(),
        )?;
        let mut cs = Changeset {
            workflow: Some(cs_workflow),
            orchestrator_session_id: stack_parent.map(str::to_string),
            recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
            ..Changeset::default()
        };
        if let Some(recipe) = &managed_recipe {
            tddy_core::changeset::update_state(
                &mut cs,
                tddy_core::workflow::ids::WorkflowState::new(recipe.start_goal().as_str()),
            );
        }
        tddy_core::write_changeset(&session_dir, &cs)
            .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

        let chain_base_ref = self
            .resolve_chain_base_ref_status(&stack_parent::StackBaseLookup {
                session_token,
                stack_parent,
                stack_parent_daemon_instance_id,
                project_id,
                sessions_base: &sessions_base,
                repo_root: &repo_root,
                new_branch_name,
                stack_node_id,
                selected_integration_base_ref,
            })
            .await?;
        let worktree_base_ref =
            tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);
        let repo_root_clone = repo_root.clone();
        let session_dir_clone = session_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let worktree_path = service_util::spawn_blocking_with_timeout(
            timeout,
            "start_sandboxed_cursor_cli_session: create worktree",
            move || {
                tddy_core::setup_worktree_for_session_with_optional_chain_base(
                    &repo_root_clone,
                    &session_dir_clone,
                    worktree_base_ref.as_deref(),
                )
                .map_err(|e| anyhow::anyhow!("worktree setup failed: {e}"))
            },
        )
        .await?;

        service_util::push_new_branch_to_origin_if_requested(
            create_remote_branch,
            intent,
            &session_dir,
            &worktree_path,
            timeout,
        )
        .await?;

        let hook_token = crate::cursor_cli_spawn::install_cursor_hooks_in_worktree(
            &self.config,
            &worktree_path,
            session_id,
            os_user,
        );

        // The clones this session's seeded agents read, claimed now: the worktree they mirror
        // exists, and the peers that build them must be at work before the agent that prompts them
        // is launched. Where an agent runs decides how the session is split across hosts, never
        // whether it can be seeded — the same placements the split start takes, this one takes.
        let seeded_clones = self
            .claim_co_located_seed_clones(
                session_id,
                &seed_codebase::SeedCodebase::of_a_starting_session(
                    session_dir.clone(),
                    worktree_path.clone(),
                    project_id,
                    // The jail is what puts `mcp__tddy-tools__*` in front of the main agent's file
                    // tools, which is what makes a withdrawal enforceable
                    // (`session_enforces_a_withdrawal`).
                    true,
                ),
                session_token,
                &mut started_agents,
            )
            .await?;

        let sandbox_root = session_dir.join("sandbox");
        let egress_dir = session_dir.join("egress");
        std::fs::create_dir_all(sandbox_root.join(".work").join("home"))
            .map_err(|e| Status::internal(format!("mkdir sandbox scratch: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
            .map_err(|e| Status::internal(format!("mkdir sandbox tmp: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join("context"))
            .map_err(|e| Status::internal(format!("mkdir sandbox context: {e}")))?;
        std::fs::create_dir_all(&egress_dir)
            .map_err(|e| Status::internal(format!("mkdir sandbox egress: {e}")))?;

        let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
        let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
        let scratch_dir = sandbox_root.join(".work");
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        let replacement_pairs = roster_replacement_pairs(&started_agents);
        let replacement_refs: Vec<Vec<&str>> = replacement_pairs
            .iter()
            .map(|(_, tools)| tools.iter().map(String::as_str).collect())
            .collect();
        let replacements: Vec<tddy_sandbox::SubagentReplacement<'_>> = replacement_pairs
            .iter()
            .zip(replacement_refs.iter())
            .map(|((name, _), refs)| tddy_sandbox::SubagentReplacement {
                name,
                replaced: refs,
            })
            .collect();
        let ctx = tddy_daemon_sandbox::sandbox_session::prepare_context_dir_with_subagent(
            &worktree_path,
            &replacements,
            crate::context_files::context_globs_for_session_type("cursor-cli"),
        )
        .map_err(Status::internal)?;
        tddy_daemon_sandbox::sandbox_session::copy_dir_all(ctx.path(), &context_dir)
            .map_err(Status::internal)?;

        let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
            crate::config::resolve_cursor_cli_tddy_tools_path(&self.config).as_deref(),
        );

        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut session_env: Vec<(String, String)> = Vec::new();
        if let Some(recipe) = managed_recipe.clone() {
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                &session_dir,
                &worktree_path,
                &context_dir,
                &tddy_tools_path,
                None,
                None,
            )?;
            if let Ok(prompt) = std::fs::read_to_string(&launch.prompt_file) {
                let rules_dir = worktree_path.join(".cursor").join("rules");
                let _ = std::fs::create_dir_all(&rules_dir);
                let _ = std::fs::write(rules_dir.join("tddy-managed-workflow.mdc"), prompt);
            }
            session_env = launch.env;
            managed = Some(launch.workflow);
        }

        let canonicalize_exec = |p: &str| -> String {
            if p.contains('/') {
                std::fs::canonicalize(p)
                    .map(|c| c.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| p.to_string())
            } else {
                p.to_string()
            }
        };
        let tddy_tools_path = canonicalize_exec(&tddy_tools_path);
        let sandbox_runner_path =
            canonicalize_exec(&tddy_daemon_sandbox::sandbox_session::resolve_sandbox_runner_path());
        let cursor_binary =
            canonicalize_exec(&crate::config::resolve_cursor_binary_path(&self.config));
        let cursor_home_dir = crate::config::resolve_cursor_home_dir(&self.config);
        let scratch_home = tddy_daemon_sandbox::sandbox_session::prepare_persistent_cursor_home(
            &cursor_home_dir,
            &cursor_binary,
        );

        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let profile_path = sandbox_root.join("sandbox.sb");

        let egress_shim_port = tddy_daemon_sandbox::sandbox_session::pick_free_loopback_port()
            .map_err(Status::internal)?;
        let loopback_allow_ports = vec![egress_shim_port];

        let mut runner_argv = vec![
            sandbox_runner_path,
            "--session-id".into(),
            session_id.to_string(),
            "--context-dir".into(),
            context_dir.to_string_lossy().to_string(),
            "--tool-ipc-socket".into(),
            tool_ipc_socket.to_string_lossy().to_string(),
            "--tddy-tools-path".into(),
            tddy_tools_path.clone(),
            "--ready-marker".into(),
            ready_marker.to_string_lossy().to_string(),
            "--agent-kind".into(),
            "cursor".into(),
            "--agent-binary".into(),
            cursor_binary.clone(),
            "--model".into(),
            model.to_string(),
            "--egress-shim-port".into(),
            egress_shim_port.to_string(),
            "--stdio".into(),
        ];
        let prompt = initial_prompt.trim();
        if !prompt.is_empty() {
            runner_argv.push("--agent-arg".into());
            runner_argv.push(prompt.to_string());
        }

        // TODO: pin a Cursor chat for sandboxed cursor sessions too (`--agent-arg --resume
        // --agent-arg <id>`), the way the unsandboxed path does in `spawn_cursor_cli_session_inner`.
        // Not done here because the jail runs against its own persistent cursor home
        // (`prepare_persistent_cursor_home`), so a chat minted by the host `cursor-agent
        // create-chat` is not necessarily resolvable inside the jail — that needs verifying before
        // an id is pinned. Until then a sandboxed session records no `cursor_chat_id`, and its
        // first resume adopts a fresh chat (see `resume_cursor_cli_session`).

        // Semantic index: index the worktree into the session dir before spawning the jail
        // (blocking; a missing embedder or a failed index aborts the start — no unindexed
        // fallback), and inject `TDDY_SEMANTIC_INDEX_DB` into the jail env so the in-jail
        // `SemanticSearch` tool resolves against the per-session index.
        let mut semantic_index_env_pair: Option<(String, String)> = None;
        if semantic_index {
            let embedder =
                tddy_semantic_index::production_embedder(&self.tddy_data_dir).map_err(|e| {
                    Status::failed_precondition(format!(
                        "semantic index requested but no embedder is available: {e}"
                    ))
                })?;
            tddy_semantic_index::semantic_index::run_semantic_index_blocking(
                &worktree_path,
                &session_dir,
                embedder,
                &self.task_registry,
                session_id,
            )
            .await
            .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
            semantic_index_env_pair = Some(
                tddy_semantic_index::semantic_index::semantic_index_env(&session_dir),
            );
        }

        let mut env = tddy_daemon_sandbox::sandbox_session::build_sandboxed_cursor_runner_env(
            &scratch_home,
            &scratch_tmp,
            session_id,
            &tool_ipc_socket,
            &egress_dir,
        );
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(&worktree_path));
        env.extend(semantic_index_env_pair);

        let mut handle = tddy_daemon_sandbox::sandbox_session::spawn_sandbox_runner(
            tddy_daemon_sandbox::sandbox_session::SandboxRunnerSpawn {
                project_root: sandbox_root.clone(),
                scratch_dir: scratch_dir.clone(),
                egress_dir: egress_dir.clone(),
                profile_path,
                runner_argv,
                env,
                loopback_allow_ports,
                ipc_socket: Some(tool_ipc_socket.clone()),
                mounts: vec![tddy_sandbox::MountSpec::read_write(scratch_home.clone())],
                host_home: None,
                cgroup: self.config.sandbox_cgroup_config(),
            },
        )
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            let mut status = tddy_daemon_sandbox::sandbox_session::sandbox_error_to_status(e);
            status.message = format!("{}\n{logs}", status.message);
            status
        })?;

        tddy_daemon_sandbox::sandbox_session::wait_for_sandbox_ready(
            &mut handle,
            &ready_marker,
            std::time::Duration::from_secs(120),
            &egress_dir,
        )
        .await
        .map_err(Status::deadline_exceeded)?;

        let (stdout_tx, _) = tokio::sync::broadcast::channel(256);
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();

        tddy_daemon_sandbox::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.clone(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(session_env),
            session_dir.clone(),
            self.agent_activity_hub(),
            Arc::new(DaemonRpcHandler {
                conn: self.self_arc(),
            }),
        )
        .await
        .map_err(Status::internal)?;

        let pid = handle.pid();
        let state = Arc::new(
            tddy_daemon_sandbox::sandbox_session::SandboxSessionState::new(
                tddy_daemon_sandbox::sandbox_session::SandboxSessionStateInit {
                    pid,
                    worktree_path: worktree_path.clone(),
                    stdout_tx,
                    capture,
                    stdin_tx,
                    ready_marker: ready_marker.clone(),
                    handle,
                    managed_workflow: managed.map(|w| {
                        Box::new(w)
                            as Box<dyn tddy_daemon_sandbox::sandbox_session::SessionScopedResource>
                    }),
                },
            ),
        );
        self.sandbox_manager
            .insert(session_id.to_string(), state)
            .await;

        let now = chrono::Utc::now().to_rfc3339();
        let meta = tddy_core::SessionMetadata {
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
            status: "active".to_string(),
            repo_path: Some(worktree_path.to_string_lossy().to_string()),
            pid: Some(pid),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("cursor-cli".to_string()),
            model: Some(model.to_string()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: Some(hook_token),
            sandbox: Some(true),
            agent: None,
            recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
            agents_rev: agent_roster::started_roster_rev(&started_agents),
            agents: started_agents,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
            agent_daemon_instance_id: None,
            agent_session_id: None,
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
        // The roster naming them is on disk now, so the clones belong to the session rather than to
        // the start that claimed them.
        seeded_clones.keep();

        log::info!(
            target: "tddy_daemon::connection_service",
            "started sandboxed cursor-cli session {session_id} pid={pid} worktree={}",
            worktree_path.display()
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
            branch_conflict: None,
        }))
    }
}
