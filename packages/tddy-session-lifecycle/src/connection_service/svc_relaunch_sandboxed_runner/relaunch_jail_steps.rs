use super::DaemonSessionHost;
use std::sync::Mutex as StdMutex;

use tddy_task::TerminalCapture;

use super::RelaunchedJailBridge;

use super::RelaunchedRunnerSpawn;

use super::RelaunchJailEnv;

use std::path::PathBuf;

use tddy_rpc::Status;

use super::RelaunchManagedEnv;

use std::sync::Arc;

use std::path::Path;

impl DaemonSessionHost {
    pub(super) fn relaunch_managed_workflow(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_path: &Path,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        context_dir: &Path,
        tddy_tools_path: &str,
    ) -> Result<RelaunchManagedEnv, Status> {
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
                context_dir,
                tddy_tools_path,
                Some(resume_goal),
                None,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            session_env = launch.env;
            managed = Some(launch.workflow);
        }
        Ok((managed, append_system_prompt_file, session_env))
    }

    pub(super) fn resolve_relaunch_binaries(
        &self,
        tddy_tools_path: String,
    ) -> (String, String, String, PathBuf) {
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
        (
            tddy_tools_path,
            sandbox_runner_path,
            claude_binary,
            scratch_home,
        )
    }

    pub(super) fn relaunch_jail_env(
        &self,
        launch: RelaunchJailEnv<'_>,
    ) -> Result<std::collections::BTreeMap<String, String>, Status> {
        let RelaunchJailEnv {
            session_id,
            worktree_path,
            specialized_defs,
            egress_dir,
            scratch_tmp,
            scratch_home,
            tool_ipc_socket,
        } = launch;
        let mut env = tddy_daemon_sandbox::sandbox_session::build_sandbox_runner_env(
            scratch_home,
            &scratch_tmp,
            session_id,
            tool_ipc_socket,
            egress_dir,
        );
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(worktree_path));
        Ok(env)
    }

    pub(super) async fn spawn_relaunched_runner(
        &self,
        launch: RelaunchedRunnerSpawn<'_>,
    ) -> Result<tddy_sandbox::SandboxHandle, Status> {
        let RelaunchedRunnerSpawn {
            sandbox_root,
            egress_dir,
            scratch_dir,
            scratch_home,
            tool_ipc_socket,
            ready_marker,
            profile_path,
            loopback_allow_ports,
            runner_argv,
            env,
        } = launch;
        let mut handle = tddy_daemon_sandbox::sandbox_session::spawn_sandbox_runner(
            tddy_daemon_sandbox::sandbox_session::SandboxRunnerSpawn {
                project_root: sandbox_root.clone(),
                scratch_dir: scratch_dir.clone(),
                egress_dir: egress_dir.to_path_buf(),
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
            let logs = tddy_sandbox::format_egress_logs(egress_dir);
            let mut status = tddy_daemon_sandbox::sandbox_session::sandbox_error_to_status(e);
            status.message = format!("{}\n{logs}", status.message);
            status
        })?;
        tddy_daemon_sandbox::sandbox_session::wait_for_sandbox_ready(
            &mut handle,
            ready_marker,
            std::time::Duration::from_secs(120),
            egress_dir,
        )
        .await
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(egress_dir);
            Status::deadline_exceeded(format!("wait for sandbox ready: {e}\n{logs}"))
        })?;
        Ok(handle)
    }

    pub(super) async fn bridge_relaunched_jail(
        &self,
        launch: RelaunchedJailBridge<'_>,
    ) -> Result<u32, Status> {
        let RelaunchedJailBridge {
            session_id,
            session_dir,
            worktree_path,
            egress_dir,
            managed,
            session_env,
            ready_marker,
            mut handle,
        } = launch;
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
