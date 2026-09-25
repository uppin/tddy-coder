use super::sandbox_claude_passthrough_args;

use super::WorktreeSource;

use super::session_worktree_source;

use crate::{
    branch_intent::BranchIntentPolicy,
    connection_service::{seed_codebase, service_util},
};

use crate::branch_intent::BranchIntentRequest;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use std::path::{Path, PathBuf};

use super::DaemonSessionHost;

/// The session a sandboxed start is building a jail for: who it is and where it lives on the host.
struct JailSession<'a> {
    session_id: &'a str,
    project_id: &'a str,
    session_dir: &'a Path,
    worktree_path: &'a Path,
}

/// The branch a sandboxed start works on, and the PR-stack node it materializes: what cutting its
/// worktree and linking that branch back to the orchestrator both read.
struct JailBranch<'a> {
    session_id: &'a str,
    session_token: &'a str,
    sessions_base: &'a Path,
    branch_worktree_intent: &'a str,
    new_branch_name: &'a str,
    selected_integration_base_ref: &'a str,
    selected_branch_to_work_on: &'a str,
    stack_parent: Option<&'a str>,
    stack_parent_daemon_instance_id: &'a str,
    stack_node_id: &'a str,
    create_remote_branch: bool,
}

/// The per-session jail directories, created and resolved to their canonical paths.
struct JailDirs {
    sandbox_root: PathBuf,
    egress_dir: PathBuf,
    scratch_dir: PathBuf,
    scratch_tmp: PathBuf,
    context_dir: PathBuf,
}

/// Everything the sandbox runner is spawned with, beyond the session and its directories.
struct JailLaunch {
    managed: Option<crate::session_toolcall::ManagedWorkflow>,
    session_env: Vec<(String, String)>,
    scratch_home: PathBuf,
    tool_ipc_socket: PathBuf,
    ready_marker: PathBuf,
    profile_path: PathBuf,
    loopback_allow_ports: Vec<u16>,
    runner_argv: Vec<String>,
    env: std::collections::BTreeMap<String, String>,
}

/// A managed-workflow session's controller, its orchestration prompt file and its host-side env.
type ManagedJailEnv = (
    Option<crate::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

/// What the sandbox runner's env is built from: the jail's paths, its subagents, and the semantic index.
struct JailRunnerEnv<'a> {
    session_id: &'a str,
    semantic_index: bool,
    specialized_defs: Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
    session_dir: &'a Path,
    worktree_path: &'a Path,
    jail_dirs: &'a JailDirs,
    scratch_home: &'a Path,
    tool_ipc_socket: &'a Path,
}

impl DaemonSessionHost {
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
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
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
        let (mut started_agents, specialized_defs) =
            self.warm_up_jail_agents(specialized_agents).await?;

        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        // A client-supplied `repo_path` runs against that checkout directly (no registered
        // project), so a stored default branch only applies when resolving from `project_id`.
        let project_default_branch_ref =
            self.project_default_branch_ref(os_user, project_id, repo_path);

        let intent = service_util::write_initial_changeset(
            session_id,
            &BranchIntentRequest {
                branch_worktree_intent,
                new_branch_name,
                selected_integration_base_ref,
                selected_branch_to_work_on,
            },
            BranchIntentPolicy::claude_cli(),
            project_default_branch_ref.as_deref(),
            &session_dir,
            stack_parent,
            managed_recipe.as_deref(),
        )?;

        // Resolve the session's worktree. A client-supplied `repo_path` is used directly (arbitrary
        // local checkout, edited via the host-side tool relay as the caller's mapped OS user); it is
        // never wrapped in a daemon-managed git worktree and never removed on session end. Otherwise
        // fall back to the registered project and create a git worktree as before.
        let branch = JailBranch {
            session_id,
            session_token,
            sessions_base: &sessions_base,
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
            stack_parent,
            stack_parent_daemon_instance_id,
            stack_node_id,
            create_remote_branch,
        };
        let worktree_path = match session_worktree_source(repo_path, project_id) {
            WorktreeSource::Project(pid) => {
                if pid.is_empty() {
                    return Err(Status::invalid_argument(
                        "project_id is required for claude-cli sessions",
                    ));
                }
                let (projects_dir, project) =
                    service_util::find_registered_project(&self.tddy_data_dir, os_user, &pid)?;
                let repo_root = service_util::project_repo_root(&project)?;
                let wt = self
                    .create_jail_project_worktree(&branch, &session_dir, intent, &pid, &repo_root)
                    .await?;
                // The branch this spawn works on now exists — record it on the orchestrator's planned
                // node (see `link_stack_node_to_spawned_branch`), keyed on the effective branch so a
                // resumed branch re-links its node. Only this arm resolves a project worktree; a
                // client-supplied `repo_path` materializes no planned node.
                self.link_jail_branch_to_stack_node(
                    &branch,
                    &session_dir,
                    &pid,
                    &projects_dir,
                    &repo_root,
                )
                .await?;
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

        let jail = JailSession {
            session_id,
            project_id,
            session_dir: &session_dir,
            worktree_path: &worktree_path,
        };
        let jail_dirs = jail_session_files::prepare_jail_dirs(&session_dir)?;

        jail_session_files::prepare_jail_context_dir(
            &started_agents,
            &worktree_path,
            &jail_dirs.context_dir,
        )?;

        let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );

        // Managed workflow: build the per-session controller + toolcall listener, write the recipe's
        // orchestration prompt into the jail-visible context dir, and prepare the per-session env
        // (TDDY_SOCKET + PATH) applied to host-side `tddy-tools transition` (run via the Shell relay).
        let (managed, append_system_prompt_file, session_env) = self.managed_jail_env(
            &jail,
            os_user,
            &sessions_base,
            &managed_recipe,
            &jail_dirs.context_dir,
            &tddy_tools_path,
        )?;

        // Canonicalize the binary paths the runner will exec: the SBPL allow-list is built
        // from the canonical (symlink-resolved) parent dirs, so a symlinked spelling (e.g. a
        // binary under /tmp -> /private/tmp) would be denied at exec time ("doesn't exist /
        // Operation not permitted"). A relative/PATH-resolved name (no '/') is left as-is.
        let (tddy_tools_path, sandbox_runner_path) = canonical_jail_exec_paths(tddy_tools_path);
        // Resolve the real `claude` to an absolute path (skipping wrapper shims). Overridable via
        // TDDY_CLAUDE_BINARY or `claude_cli.binary_path`. A bare name would give binary_exec_reads
        // an empty parent → `(subpath "")` → macOS sandbox-exec rejects the profile.
        let claude_binary = crate::config::resolve_claude_binary_path(&self.config);
        let claude_binary = claude_binary.as_str();

        // Persistent daemon-wide jail $HOME: reused across sessions and mounted read-write below, so
        // refreshed OAuth tokens, session history, and settings survive. Seeded non-clobbering.
        let claude_home_dir = crate::config::resolve_claude_home_dir(&self.config);
        let scratch_home = tddy_daemon_sandbox::sandbox_session::prepare_persistent_claude_home(
            &claude_home_dir,
            claude_binary,
        );

        // The tool-IPC AF_UNIX socket must fit within SUN_LEN (104 bytes on macOS); the
        // canonical session dir is far too deep, so use a short out-of-tree path that the
        // SBPL profile grants an explicit literal allow (see SandboxSpec::ipc_socket).
        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = jail_dirs.sandbox_root.join("sandbox.ready");
        let profile_path = jail_dirs.sandbox_root.join("sandbox.sb");

        let perm = if permission_mode.trim().is_empty() {
            "auto"
        } else {
            permission_mode.trim()
        };

        let egress_shim_port = tddy_daemon_sandbox::sandbox_session::pick_free_loopback_port()
            .map_err(Status::internal)?;
        let loopback_allow_ports = vec![egress_shim_port];

        let mut runner_argv = vec![
            sandbox_runner_path,
            "--session-id".into(),
            session_id.to_string(),
            "--context-dir".into(),
            jail_dirs.context_dir.to_string_lossy().to_string(),
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
        let env = self
            .jail_runner_env(JailRunnerEnv {
                session_id,
                semantic_index,
                specialized_defs,
                session_dir: &session_dir,
                worktree_path: &worktree_path,
                jail_dirs: &jail_dirs,
                scratch_home: &scratch_home,
                tool_ipc_socket: &tool_ipc_socket,
            })
            .await?;

        let pid = self
            .launch_jail(
                &jail,
                &jail_dirs,
                JailLaunch {
                    managed,
                    session_env,
                    scratch_home,
                    tool_ipc_socket,
                    ready_marker,
                    profile_path,
                    loopback_allow_ports,
                    runner_argv,
                    env,
                },
            )
            .await?;

        jail_session_files::write_jail_session_metadata(
            &jail,
            model,
            managed_recipe,
            started_agents,
            pid,
        )?;
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

    async fn jail_runner_env(
        &self,
        launch: JailRunnerEnv<'_>,
    ) -> Result<std::collections::BTreeMap<String, String>, Status> {
        let JailRunnerEnv {
            session_id,
            semantic_index,
            specialized_defs,
            session_dir,
            worktree_path,
            jail_dirs,
            scratch_home,
            tool_ipc_socket,
        } = launch;
        let semantic_index_env_pair = self
            .jail_semantic_index_env(session_id, semantic_index, session_dir, worktree_path)
            .await?;
        let mut env = tddy_daemon_sandbox::sandbox_session::build_sandbox_runner_env(
            scratch_home,
            &jail_dirs.scratch_tmp,
            session_id,
            tool_ipc_socket,
            &jail_dirs.egress_dir,
        );
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(worktree_path));
        env.extend(semantic_index_env_pair);
        Ok(env)
    }
}

fn canonical_jail_exec_paths(tddy_tools_path: String) -> (String, String) {
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
    (tddy_tools_path, sandbox_runner_path)
}

mod jail_worktree;

mod jail_launch_steps;

mod jail_session_files;
