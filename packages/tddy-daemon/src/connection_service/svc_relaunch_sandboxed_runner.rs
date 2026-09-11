use crate::connection_service::agent_roster;
use std::sync::Mutex as StdMutex;

use tddy_task::TerminalCapture;

use std::path::Path;

use std::path::PathBuf;

use super::roster_replacement_pairs;

use tddy_rpc::Status;

use std::sync::Arc;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Spawn sandbox-runner + SessionChannel bridge for an existing session directory.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn relaunch_sandboxed_runner(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_path: &Path,
        model: &str,
        permission_mode: &str,
        // The session's **persisted** roster, not the names its start request carried: an agent
        // attached while the session ran is in the former and in neither the latter nor the jail's
        // previous seed, and a relaunch that read the request would hand the main agent back a tool
        // the operator had withdrawn from it (PRD AC25).
        agents: &[tddy_core::SessionAgentRecord],
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, spawn the runner with `--resume` so the jailed `claude` continues the existing
        // on-disk transcript (`--resume <id>`) instead of assigning the id to a fresh session
        // (`--session-id <id>`). The persistent sandbox claude HOME keeps the transcript across
        // daemon restarts, so a fresh `--session-id` would abort with "Session ID already in use".
        resume: bool,
    ) -> Result<u32, Status> {
        // The defs are re-resolved for what the jail's *seed* and the warm-up need — an endpoint to
        // wake and a registry to start from. What the main agent loses comes from the roster below,
        // never from these: a def edited since the attach must not change a running session's tools.
        let specialized_defs = self
            .resolve_specialized_agent_defs(&agent_roster::roster_agent_ids(agents))
            .await?;

        // The same readiness gate the start paths apply: a resumed session's subagents are only as
        // usable as a fresh one's if their endpoints are awake before the jail comes back up.
        // Without this, resume would hand the agent a subagent whose first call stalls on a cold
        // model. No fallback — the runner is not relaunched if warm-up fails.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

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
        // scratch_home (jail $HOME) is the persistent daemon-wide claude home, resolved and mounted
        // below — not a per-session dir — so auth/history persist across sessions.
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        let replacement_pairs = roster_replacement_pairs(agents);
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
            worktree_path,
            &replacements,
            // The relaunch path serves `claude-cli` alone (`resume_sandboxed_claude_cli_session` is
            // its only caller), so the agent's allow-list is that backend's.
            crate::context_files::context_globs_for_session_type("claude-cli"),
        )
        .map_err(|e| Status::internal(format!("prepare context dir: {e}")))?;
        if context_dir.exists() {
            std::fs::remove_dir_all(&context_dir)
                .map_err(|e| Status::internal(format!("clear context dir: {e}")))?;
        }
        std::fs::create_dir_all(&context_dir)
            .map_err(|e| Status::internal(format!("mkdir context dir: {e}")))?;
        tddy_daemon_sandbox::sandbox_session::copy_dir_all(ctx.path(), &context_dir)
            .map_err(|e| Status::internal(format!("copy context dir: {e}")))?;

        let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );

        // Re-wire managed-workflow orchestration on resume of a managed sandboxed session; the
        // controller resumes at the goal persisted in changeset.yaml. The prompt goes into the
        // jail-visible context dir and the per-session env carries TDDY_SOCKET for host-side
        // `tddy-tools transition` (relayed via the Shell tool).
        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        let mut session_env: Vec<(String, String)> = Vec::new();
        if let Some(recipe) = managed_recipe {
            let resume_goal = Self::managed_resume_goal(session_dir, &recipe);
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                session_dir,
                worktree_path,
                &context_dir,
                &tddy_tools_path,
                Some(resume_goal),
                None,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
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
        // See the sibling call site above: resolve the real `claude` (overridable via
        // TDDY_CLAUDE_BINARY / `claude_cli.binary_path`); a bare name breaks the sandbox profile.
        let claude_binary = crate::config::resolve_claude_binary_path(&self.config);

        // Persistent daemon-wide jail $HOME (see sibling site above): mounted read-write, seeded
        // non-clobbering, so auth/history persist across sessions.
        let claude_home_dir = crate::config::resolve_claude_home_dir(&self.config);
        let scratch_home = tddy_daemon_sandbox::sandbox_session::prepare_persistent_claude_home(
            &claude_home_dir,
            &claude_binary,
        );

        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let _ = std::fs::remove_file(&tool_ipc_socket);
        let _ = std::fs::remove_file(&ready_marker);
        let profile_path = sandbox_root.join("sandbox.sb");
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
            context_dir.to_string_lossy().to_string(),
            "--tool-ipc-socket".into(),
            tool_ipc_socket.to_string_lossy().to_string(),
            "--tddy-tools-path".into(),
            tddy_tools_path.clone(),
            "--ready-marker".into(),
            ready_marker.to_string_lossy().to_string(),
            "--claude-binary".into(),
            claude_binary,
            "--model".into(),
            model.to_string(),
            "--permission-mode".into(),
            perm.to_string(),
            "--egress-shim-port".into(),
            egress_shim_port.to_string(),
            "--stdio".into(),
        ];
        if resume {
            runner_argv.push("--resume".into());
        }
        if let Some(prompt_path) = &append_system_prompt_file {
            runner_argv.push("--append-system-prompt-file".into());
            runner_argv.push(prompt_path.to_string_lossy().to_string());
        }

        let mut env = tddy_daemon_sandbox::sandbox_session::build_sandbox_runner_env(
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
        env.extend(self.lsp_tools_env(worktree_path));

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
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            Status::deadline_exceeded(format!("wait for sandbox ready: {e}\n{logs}"))
        })?;

        let (stdout_tx, _) = tokio::sync::broadcast::channel(256);
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();

        tddy_daemon_sandbox::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.to_path_buf(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(session_env),
            session_dir.to_path_buf(),
            self.agent_activity_hub(),
            self.sandbox_rpc_handler(),
        )
        .await
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(&egress_dir);
            Status::internal(format!("dial sandbox SessionChannel: {e}\n{logs}"))
        })?;

        let pid = handle.pid();
        let state = Arc::new(
            tddy_daemon_sandbox::sandbox_session::SandboxSessionState::new(
                tddy_daemon_sandbox::sandbox_session::SandboxSessionStateInit {
                    pid,
                    worktree_path: worktree_path.to_path_buf(),
                    stdout_tx,
                    capture,
                    stdin_tx,
                    ready_marker,
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
        Ok(pid)
    }
}
