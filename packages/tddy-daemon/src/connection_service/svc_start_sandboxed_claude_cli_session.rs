use super::DaemonRpcHandler;
use std::sync::Mutex as StdMutex;

use tddy_task::TerminalCapture;

use super::sandbox_claude_passthrough_args;

use super::roster_replacement_pairs;

use super::WorktreeSource;

use super::session_worktree_source;

use tddy_core::Changeset;

use crate::{
    branch_intent::BranchIntentPolicy,
    connection_service::{agent_roster, hooks_and_urls, seed_codebase, service_util, stack_parent},
    project_storage,
};

use crate::branch_intent::BranchIntentRequest;

use crate::branch_intent::resolve_branch_workflow;

use crate::branch_intent::ResolvedBranchWorkflow;

use crate::user_sessions_path::projects_path_for_user;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::connection::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use std::path::PathBuf;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Handle `StartSession` for sandboxed `claude-cli` sessions (darwin Seatbelt, local gRPC).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_sandboxed_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        // Authorizes the `workspace` starts this session's seeded agents need on their own hosts
        // (see `provision_agent_clone`); the peer sees the same token the client presented here.
        session_token: &str,
        sessions_base: PathBuf,
        model: &str,
        project_id: &str,
        // Client-supplied local checkout to run against directly (StartSessionRequest.repo_path).
        // When non-empty it wins over `project_id`: the session's worktree IS this path (no git
        // worktree is created, no registered project is required, and it is never removed on
        // session end). Empty → resolve the worktree from the registered `project_id` as before.
        repo_path: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        // Passed to `claude` as a trailing positional (first user turn) after any pass-through args.
        initial_prompt: &str,
        // Extra args forwarded verbatim to the in-jail `claude` (StartSessionRequest.claude_args).
        claude_args: &[String],
        permission_mode: &str,
        dangerously_skip_permissions: bool,
        stack_parent: Option<&str>,
        // The daemon whose sessions tree holds `stack_parent` (empty = this one).
        stack_parent_daemon_instance_id: &str,
        // The planned node of that parent's stack this child materializes (empty = derive it from
        // the branch, locally — see `record_spawn_on_stack_node`).
        stack_node_id: &str,
        // Specialized subagents (see docs/ft/coder/specialized-subagents.md). This sandboxed path
        // already never mounts the repo (`mounts: vec![]` below, unconditionally) —
        // `managed_codebase` is accepted for request-shape/UI-intent clarity, not to toggle mount
        // behavior. Names resolve against `<tddyhome>/agents` (+ builtins) and are wired into the
        // jail env; all configuration (model, base_url, max_turns, replaces) comes exclusively
        // from the resolved def.
        _managed_codebase: bool,
        specialized_agents: &[String],
        // When `Some`, launch workflow-aware: inject the recipe's orchestration prompt and route the
        // agent's host-side `tddy-tools transition` to a per-session `WorkflowController`.
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, index the worktree before launch (blocking; aborts on failure) and expose the
        // in-jail `SemanticSearch` tool backed by that per-session index.
        semantic_index: bool,
        // When true (new_branch_from_base + registered project), push the new branch to origin.
        create_remote_branch: bool,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }
        let project_id = project_id.trim();
        let repo_path = repo_path.trim();
        // The roster this session starts with, resolved before anything is created for it: it is
        // what the withdrawal is computed from and what `.session.yaml` persists, and a reference
        // naming no agent must fail the start rather than leave the main agent holding tools it was
        // told it had given away.
        let mut started_agents = self.seeded_roster_records(specialized_agents).await?;
        // The defs behind those records, which the jail env can only carry for agents this host
        // holds — the records above are what carries the rest.
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

        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        // A client-supplied `repo_path` runs against that checkout directly (no registered
        // project), so a stored default branch only applies when resolving from `project_id`.
        let project_default_branch_ref: Option<String> =
            if repo_path.is_empty() && !project_id.is_empty() {
                projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .and_then(|dir| {
                        project_storage::find_project(&dir, project_id)
                            .ok()
                            .flatten()
                    })
                    .and_then(|p| p.main_branch_ref)
            } else {
                None
            };

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
            BranchIntentPolicy::claude_cli(),
            project_default_branch_ref.as_deref(),
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

        // Resolve the session's worktree. A client-supplied `repo_path` is used directly (arbitrary
        // local checkout, edited via the host-side tool relay as the caller's mapped OS user); it is
        // never wrapped in a daemon-managed git worktree and never removed on session end. Otherwise
        // fall back to the registered project and create a git worktree as before.
        let worktree_path = match session_worktree_source(repo_path, project_id) {
            WorktreeSource::Project(pid) => {
                if pid.is_empty() {
                    return Err(Status::invalid_argument(
                        "project_id is required for claude-cli sessions",
                    ));
                }
                let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve projects path"))?;
                let project = project_storage::find_project(&projects_dir, &pid)
                    .map_err(|e| Status::internal(e.to_string()))?
                    .ok_or_else(|| Status::not_found("project not found"))?;
                let repo_root = PathBuf::from(&project.main_repo_path);
                if !repo_root.exists() {
                    return Err(Status::invalid_argument(
                        "project main repo path does not exist",
                    ));
                }
                let chain_base_ref = self
                    .resolve_chain_base_ref_status(&stack_parent::StackBaseLookup {
                        session_token,
                        stack_parent,
                        stack_parent_daemon_instance_id,
                        project_id: &pid,
                        sessions_base: &sessions_base,
                        repo_root: &repo_root,
                        new_branch_name,
                        stack_node_id,
                        selected_integration_base_ref,
                    })
                    .await?;
                let worktree_base_ref = tddy_core::select_worktree_base_ref(
                    selected_integration_base_ref,
                    chain_base_ref,
                );
                let repo_root_clone = repo_root.clone();
                let session_dir_clone = session_dir.clone();
                let timeout = self.config.spawn_worker_request_timeout();
                let wt = service_util::spawn_blocking_with_timeout(
                    timeout,
                    "start_sandboxed_claude_cli_session: create worktree",
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
                    &wt,
                    timeout,
                )
                .await?;
                // The branch this spawn works on now exists — record it on the orchestrator's planned
                // node (see `link_stack_node_to_spawned_branch`), keyed on the effective branch so a
                // resumed branch re-links its node. Only this arm resolves a project worktree; a
                // client-supplied `repo_path` materializes no planned node.
                let remote = project_storage::effective_remote_name_for_project(
                    &projects_dir,
                    &pid,
                    &repo_root,
                )
                .map_err(|e| Status::internal(e.to_string()))?;
                let spawned_branch = hooks_and_urls::spawned_branch_of_session(
                    &session_dir,
                    hooks_and_urls::effective_spawn_branch(
                        branch_worktree_intent,
                        new_branch_name,
                        selected_branch_to_work_on,
                        &remote,
                    ),
                );
                // A failed link never fails the spawn (D36) — see the same call in
                // `spawn_claude_cli_session_inner`.
                if let Some(orchestrator) = stack_parent {
                    if let Err(status) = self
                        .record_spawn_on_stack_node(&stack_parent::StackNodeLink {
                            session_token,
                            orchestrator_session_id: orchestrator,
                            orchestrator_daemon_instance_id: stack_parent_daemon_instance_id,
                            node_id: stack_node_id,
                            child_session_id: session_id,
                            // The branch as the session recorded it, suffix and all — see
                            // `spawned_branch_of_session`.
                            branch: &spawned_branch,
                            sessions_base: &sessions_base,
                        })
                        .await
                    {
                        log::error!(
                            target: "tddy_daemon::connection_service",
                            "session {session_id}: could not record its branch on pr-stack orchestrator {orchestrator} (node {stack_node_id:?}, daemon {stack_parent_daemon_instance_id:?}): {}; the node keeps no branch and its descendants stay unspawnable until it is re-linked",
                            status.message()
                        );
                    }
                }
                wt
            }
            WorktreeSource::RepoPath(path) => {
                let canonical = std::fs::canonicalize(&path).map_err(|e| {
                    Status::invalid_argument(format!(
                        "repo_path {} is not accessible: {e}",
                        path.display()
                    ))
                })?;
                if !canonical.is_dir() {
                    return Err(Status::invalid_argument(format!(
                        "repo_path {} is not a directory",
                        canonical.display()
                    )));
                }
                log::info!(
                    target: "tddy_daemon::connection_service",
                    "start_sandboxed_claude_cli_session {session_id}: using client-supplied repo_path {} directly as worktree (not daemon-managed; not removed on session end)",
                    canonical.display()
                );
                canonical
            }
        };

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

        // Resolve to the real (symlink-free) paths now that the dirs exist. Seatbelt
        // evaluates file rules — including AF_UNIX socket bind — against the fully
        // resolved path, so the socket/marker paths the runner binds must match the
        // canonical paths baked into the SBPL profile. Session dirs live under TMPDIR,
        // which on macOS is reached via the /tmp -> /private/tmp symlink; without this
        // the tool-IPC socket bind fails with "Operation not permitted".
        let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
        let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
        let scratch_dir = sandbox_root.join(".work");
        // scratch_home (jail $HOME) is the persistent daemon-wide claude home, resolved and mounted
        // below — not a per-session dir — so auth/history persist across sessions.
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
        let ctx = crate::sandbox_session::prepare_context_dir_with_subagent(
            &worktree_path,
            &replacements,
            crate::context_files::context_globs_for_session_type("claude-cli"),
        )
        .map_err(Status::internal)?;
        crate::sandbox_session::copy_dir_all(ctx.path(), &context_dir).map_err(Status::internal)?;

        let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );

        // Managed workflow: build the per-session controller + toolcall listener, write the recipe's
        // orchestration prompt into the jail-visible context dir, and prepare the per-session env
        // (TDDY_SOCKET + PATH) applied to host-side `tddy-tools transition` (run via the Shell relay).
        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        let mut session_env: Vec<(String, String)> = Vec::new();
        if let Some(recipe) = managed_recipe.clone() {
            // A grill-me session gets a conversation-spawn handler bound to its toolcall listener so
            // the agent's `spawn_conversation` relay can start a fresh implementation conversation.
            let conversation_spawn_handler = self.conversation_spawn_handler_for(
                &recipe,
                os_user,
                session_id,
                project_id,
                &sessions_base,
                &session_dir,
            );
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                &session_dir,
                &worktree_path,
                &context_dir,
                &tddy_tools_path,
                None,
                conversation_spawn_handler,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            session_env = launch.env;
            managed = Some(launch.workflow);
        }

        // Canonicalize the binary paths the runner will exec: the SBPL allow-list is built
        // from the canonical (symlink-resolved) parent dirs, so a symlinked spelling (e.g. a
        // binary under /tmp -> /private/tmp) would be denied at exec time ("doesn't exist /
        // Operation not permitted"). A relative/PATH-resolved name (no '/') is left as-is.
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
            canonicalize_exec(&crate::sandbox_session::resolve_sandbox_runner_path());
        // Resolve the real `claude` to an absolute path (skipping wrapper shims). Overridable via
        // TDDY_CLAUDE_BINARY or `claude_cli.binary_path`. A bare name would give binary_exec_reads
        // an empty parent → `(subpath "")` → macOS sandbox-exec rejects the profile.
        let claude_binary = crate::config::resolve_claude_binary_path(&self.config);
        let claude_binary = claude_binary.as_str();

        // Persistent daemon-wide jail $HOME: reused across sessions and mounted read-write below, so
        // refreshed OAuth tokens, session history, and settings survive. Seeded non-clobbering.
        let claude_home_dir = crate::config::resolve_claude_home_dir(&self.config);
        let scratch_home =
            crate::sandbox_session::prepare_persistent_claude_home(&claude_home_dir, claude_binary);

        // The tool-IPC AF_UNIX socket must fit within SUN_LEN (104 bytes on macOS); the
        // canonical session dir is far too deep, so use a short out-of-tree path that the
        // SBPL profile grants an explicit literal allow (see SandboxSpec::ipc_socket).
        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let profile_path = sandbox_root.join("sandbox.sb");

        let perm = if permission_mode.trim().is_empty() {
            "auto"
        } else {
            permission_mode.trim()
        };

        let egress_shim_port =
            crate::sandbox_session::pick_free_loopback_port().map_err(Status::internal)?;
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
            "--claude-binary".into(),
            claude_binary.to_string(),
            "--model".into(),
            model.to_string(),
            "--permission-mode".into(),
            perm.to_string(),
            "--egress-shim-port".into(),
            egress_shim_port.to_string(),
            "--stdio".into(),
        ];
        // The runner reconciles this with --permission-mode (they are mutually exclusive; when set,
        // the in-jail claude argv drops --permission-mode). See build_claude_base_argv.
        if dangerously_skip_permissions {
            runner_argv.push("--dangerously-skip-permissions".into());
        }
        if let Some(prompt_path) = &append_system_prompt_file {
            runner_argv.push("--append-system-prompt-file".into());
            runner_argv.push(prompt_path.to_string_lossy().to_string());
        }
        // Forward client-supplied pass-through args (+ a trailing positional prompt) to the in-jail
        // `claude`. Each token becomes a `--claude-arg` occurrence the runner replays verbatim after
        // claude's fixed flags and before the MCP allowlist args.
        for token in sandbox_claude_passthrough_args(claude_args, initial_prompt) {
            runner_argv.push("--claude-arg".into());
            runner_argv.push(token);
        }

        // Semantic index: index the worktree into the session dir before spawning the jail
        // (blocking; a missing embedder or a failed index aborts the start — no unindexed
        // fallback), and inject `TDDY_SEMANTIC_INDEX_DB` into the jail env. Its presence both points
        // the in-jail `SemanticSearch` tool at the per-session index and signals the runner to keep
        // `SemanticSearch` in the tool set (it is otherwise folded into the replaced set).
        let mut semantic_index_env_pair: Option<(String, String)> = None;
        if semantic_index {
            let embedder =
                tddy_semantic_index::production_embedder(&self.tddy_data_dir).map_err(|e| {
                    Status::failed_precondition(format!(
                        "semantic index requested but no embedder is available: {e}"
                    ))
                })?;
            crate::semantic_index::run_semantic_index_blocking(
                &worktree_path,
                &session_dir,
                embedder,
                &self.task_registry,
                session_id,
            )
            .await
            .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
            semantic_index_env_pair = Some(crate::semantic_index::semantic_index_env(&session_dir));
        }

        let mut env = crate::sandbox_session::build_sandbox_runner_env(
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

        let mut handle = crate::sandbox_session::spawn_sandbox_runner(
            crate::sandbox_session::SandboxRunnerSpawn {
                project_root: sandbox_root.clone(),
                scratch_dir: scratch_dir.clone(),
                egress_dir: egress_dir.clone(),
                profile_path,
                runner_argv,
                env,
                loopback_allow_ports,
                ipc_socket: Some(tool_ipc_socket.clone()),
                // Mount the persistent jail $HOME read-write so it survives the session.
                mounts: vec![tddy_sandbox::MountSpec::read_write(scratch_home.clone())],
                // Persistent home is seeded separately (non-clobbering); disable the recipe's
                // per-session credential copy so it can't overwrite a refreshed jail token.
                host_home: None,
                cgroup: self.config.sandbox_cgroup_config(),
            },
        )
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            let mut status = crate::sandbox_session::sandbox_error_to_status(e);
            status.message = format!("{}\n{logs}", status.message);
            status
        })?;

        crate::sandbox_session::wait_for_sandbox_ready(
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

        crate::sandbox_session::dial_and_bridge(
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
        let state = Arc::new(crate::sandbox_session::SandboxSessionState::new(
            crate::sandbox_session::SandboxSessionStateInit {
                pid,
                worktree_path: worktree_path.clone(),
                stdout_tx,
                capture,
                stdin_tx,
                ready_marker: ready_marker.clone(),
                handle,
                managed_workflow: managed,
            },
        ));
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
            session_type: Some("claude-cli".to_string()),
            model: Some(model.to_string()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
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
            "started sandboxed claude-cli session {session_id} pid={pid} worktree={}",
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
