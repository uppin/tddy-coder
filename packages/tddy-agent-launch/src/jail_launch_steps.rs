use std::sync::Mutex as StdMutex;
use tddy_session_split::service_util;

use crate::launch_ports::LaunchSessions;

use tddy_task::TerminalCapture;

use crate::svc_start_sandboxed_claude_cli_session::JailLaunch;

use crate::svc_start_sandboxed_claude_cli_session::JailDirs;

use std::path::PathBuf;

use crate::svc_start_sandboxed_claude_cli_session::ManagedJailEnv;

use std::sync::Arc;

use std::path::Path;

use crate::svc_start_sandboxed_claude_cli_session::JailSession;

use tddy_rpc::Status;

impl LaunchSessions {
    pub(super) async fn warm_up_jail_agents(
        &self,
        specialized_agents: &[String],
    ) -> Result<
        (
            Vec<tddy_core::SessionAgentRecord>,
            Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
        ),
        Status,
    > {
        let started_agents = self
            .agent_roster
            .seeded_roster_records(specialized_agents)
            .await?;
        // The defs behind those records, which the jail env can only carry for agents this host
        // holds — the records above are what carries the rest.
        let specialized_defs = self
            .agent_roster
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
        Ok((started_agents, specialized_defs))
    }

    pub(super) fn managed_jail_env(
        &self,
        jail: &JailSession<'_>,
        os_user: &str,
        sessions_base: &Path,
        managed_recipe: &Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        context_dir: &Path,
        tddy_tools_path: &str,
    ) -> Result<ManagedJailEnv, Status> {
        let mut managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        // The agent in the jail edits and commits through the host-side Shell relay, which runs
        // under this env — so the project's account identity belongs here, managed or not. The
        // token does not: the agent's tools ask the host for it over the session's own socket.
        let identity = self.host.session_identity(
            os_user,
            jail.session_id,
            jail.project_id,
            jail.session_token,
        );
        let mut session_env: Vec<(String, String)> = identity.git_environment;
        if let Some(recipe) = managed_recipe.clone() {
            // A grill-me session gets a conversation-spawn handler bound to its toolcall listener so
            // the agent's `spawn_conversation` relay can start a fresh implementation conversation.
            let conversation_spawn_handler = self.conversation_spawn_handler_for(
                &recipe,
                os_user,
                jail.session_id,
                jail.project_id,
                sessions_base,
                jail.session_dir,
                self.host.session_account_access(jail.session_token),
            );
            let launch = self.prepare_managed_workflow(
                jail.session_id,
                recipe,
                jail.session_dir,
                jail.worktree_path,
                context_dir,
                tddy_tools_path,
                None,
                conversation_spawn_handler,
                // The host-side `tddy-tools` the jail's Shell relay runs reaches this listener, so
                // the token is asked of the host per call, as in a co-located session.
                identity.github_credential,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            session_env.extend(launch.env);
            managed = Some(launch.workflow);
        }
        Ok((managed, append_system_prompt_file, session_env))
    }

    pub(super) async fn jail_semantic_index_env(
        &self,
        session_id: &str,
        semantic_index: bool,
        session_dir: &Path,
        worktree_path: &Path,
    ) -> Result<Option<(String, String)>, Status> {
        let mut semantic_index_env_pair: Option<(String, String)> = None;
        if semantic_index {
            service_util::index_session_worktree(
                &self.tddy_data_dir,
                &self.task_registry,
                session_id,
                worktree_path,
                session_dir,
            )
            .await?;
            semantic_index_env_pair = Some(
                tddy_semantic_index::semantic_index::semantic_index_env(session_dir),
            );
        }
        Ok(semantic_index_env_pair)
    }

    pub(super) async fn launch_jail(
        &self,
        jail: &JailSession<'_>,
        jail_dirs: &JailDirs,
        launch: JailLaunch,
    ) -> Result<u32, Status> {
        let JailSession {
            session_id,
            session_dir,
            worktree_path,
            ..
        } = *jail;
        let JailDirs {
            sandbox_root,
            egress_dir,
            scratch_dir,
            ..
        } = jail_dirs;
        let JailLaunch {
            managed,
            session_env,
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
            let logs = tddy_sandbox::format_egress_logs(egress_dir);
            let mut status = tddy_daemon_sandbox::sandbox_session::sandbox_error_to_status(e);
            status.message = format!("{}\n{logs}", status.message);
            status
        })?;
        tddy_daemon_sandbox::sandbox_session::wait_for_sandbox_ready(
            &mut handle,
            &ready_marker,
            std::time::Duration::from_secs(120),
            egress_dir,
        )
        .await
        .map_err(Status::deadline_exceeded)?;
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
            Arc::clone(&self.agent_activity_hub),
            self.host.sandbox_rpc_handler(session_id, session_dir),
        )
        .await
        .map_err(Status::internal)?;
        let pid = handle.pid();
        let state = Arc::new(
            tddy_daemon_sandbox::sandbox_session::SandboxSessionState::new(
                tddy_daemon_sandbox::sandbox_session::SandboxSessionStateInit {
                    pid,
                    worktree_path: worktree_path.to_path_buf(),
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
        Ok(pid)
    }
}
