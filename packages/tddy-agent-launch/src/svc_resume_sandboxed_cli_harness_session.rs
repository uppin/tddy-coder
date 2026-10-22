//! Resume of a sandboxed CLI harness session (`opencode-cli`, `muse-cli`): its jail is relaunched
//! with the runner's resume (`--continue` for the harness), on the provider route its start recorded
//! and re-resolved from the owner's vault as it stands now. A sandboxed session never resumes on the
//! host's PTY.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_core::cli_harness::CliHarness;
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{write_session_metadata, SessionAgentRecord, SessionMetadata};
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::{ResumeSessionRequest, ResumeSessionResponse};
use tddy_session_agents::agent_roster;

use crate::launch_ports::LaunchSessions;
use crate::relaunch_jail_dirs::{prepare_relaunch_dirs, refresh_relaunch_context_dir};
use crate::session_provider_route::resume_route;
use crate::svc_relaunch_sandboxed_runner::{
    RelaunchJailEnv, RelaunchedJailBridge, RelaunchedRunnerSpawn,
};
use crate::svc_start_sandboxed_cli_harness_session::{
    cli_harness_runner_argv, resolve_cli_jail_binaries, sandboxed_runner_model, CliRunnerArgv,
};

/// What relaunching a sandboxed CLI harness session's jail is built from: the session, and the
/// route and identity its resume re-resolved.
struct CliHarnessRelaunch<'a> {
    harness: &'static CliHarness,
    session_id: &'a str,
    session_dir: &'a Path,
    worktree_path: &'a Path,
    agents: &'a [SessionAgentRecord],
    /// The model the runner starts the harness on (see `sandboxed_runner_model`).
    runner_model: &'a str,
    route: Option<HarnessRoute>,
    session_env: Vec<(String, String)>,
}

impl LaunchSessions {
    /// Handle `ResumeSession` for a sandboxed CLI harness session.
    pub(crate) async fn resume_sandboxed_cli_harness(
        &self,
        harness: &'static CliHarness,
        os_user: &str,
        req: &ResumeSessionRequest,
        session_dir: &Path,
        metadata: SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if let Some(state) = self.sandbox_manager.remove(&req.session_id).await {
            state.stop();
        } else if let Some(pid) = metadata.pid {
            tddy_daemon_sandbox::sandbox_session::terminate_sandbox_process(pid);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;

        let worktree_path = metadata
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| Status::internal("sandbox session missing repo_path in metadata"))?;
        // The account the start recorded, re-resolved from the owner's vault as it stands now: a
        // removed account refuses the resume rather than running it on the vendor.
        let route = resume_route(
            &self.host.session_account_access(&req.session_token),
            harness.route_harness,
            session_dir,
        )?;
        let runner_model = sandboxed_runner_model(
            harness,
            route.as_ref(),
            metadata.model.as_deref().unwrap_or_default(),
        )?;
        let identity = self.host.session_identity(
            os_user,
            &req.session_id,
            &metadata.project_id,
            &req.session_token,
        );
        // No managed workflow is re-wired: a CLI harness session has none to drive.
        let (session_env, _github_credential) = self.jail_relay_identity(identity, os_user)?;

        let pid = self
            .relaunch_sandboxed_cli_harness(CliHarnessRelaunch {
                harness,
                session_id: &req.session_id,
                session_dir,
                worktree_path: &worktree_path,
                agents: &metadata.agents,
                runner_model: &runner_model,
                route,
                session_env,
            })
            .await?;

        let updated = SessionMetadata {
            updated_at: chrono::Utc::now().to_rfc3339(),
            status: "active".to_string(),
            pid: Some(pid),
            ..metadata
        };
        write_session_metadata(session_dir, &updated)
            .map_err(|e| Status::internal(format!("failed to update session metadata: {e}")))?;

        Ok(Response::new(ResumeSessionResponse {
            session_id: req.session_id.clone(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
        }))
    }

    /// Spawn the runner in resume mode in the session's jail and bridge it into this daemon.
    async fn relaunch_sandboxed_cli_harness(
        &self,
        relaunch: CliHarnessRelaunch<'_>,
    ) -> Result<u32, Status> {
        let CliHarnessRelaunch {
            harness,
            session_id,
            session_dir,
            worktree_path,
            agents,
            runner_model,
            route,
            session_env,
        } = relaunch;

        // The readiness gate a start applies: a resumed subagent is only as usable as a fresh one if
        // its endpoint is awake before the jail comes back up. No fallback — the jail is not relaunched
        // when warm-up fails.
        let specialized_defs = self
            .agent_roster
            .resolve_specialized_agent_defs(&agent_roster::roster_agent_ids(agents))
            .await?;
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let (sandbox_root, egress_dir, scratch_dir, scratch_tmp, context_dir) =
            prepare_relaunch_dirs(session_dir)?;
        refresh_relaunch_context_dir(worktree_path, agents, &context_dir, harness.session_type)?;
        // The jail's $HOME is the session's own scratch, as at the start: the harness finds its
        // session store where it left it.
        let scratch_home = scratch_dir.join("home");
        let binaries = resolve_cli_jail_binaries(&self.config, harness)?;

        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let _ = std::fs::remove_file(&tool_ipc_socket);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let _ = std::fs::remove_file(&ready_marker);
        let profile_path = sandbox_root.join("sandbox.sb");
        let egress_shim_port = tddy_daemon_sandbox::sandbox_session::pick_free_loopback_port()
            .map_err(Status::internal)?;
        let runner_argv = cli_harness_runner_argv(&CliRunnerArgv {
            sandbox_runner_path: &binaries.sandbox_runner_path,
            session_id,
            context_dir: &context_dir,
            tool_ipc_socket: &tool_ipc_socket,
            tddy_tools_path: &binaries.tddy_tools_path,
            ready_marker: &ready_marker,
            agent_kind: binaries.agent_kind,
            agent_binary: &binaries.agent_binary,
            model: runner_model,
            egress_shim_port,
            resume: true,
        });

        let env = self.relaunch_jail_env(RelaunchJailEnv {
            session_id,
            worktree_path,
            specialized_defs,
            egress_dir: &egress_dir,
            scratch_tmp,
            scratch_home: &scratch_home,
            tool_ipc_socket: &tool_ipc_socket,
        })?;

        let handle = self
            .spawn_relaunched_runner(RelaunchedRunnerSpawn {
                sandbox_root,
                egress_dir: &egress_dir,
                scratch_dir,
                scratch_home: scratch_home.clone(),
                tool_ipc_socket: tool_ipc_socket.clone(),
                ready_marker: &ready_marker,
                profile_path,
                loopback_allow_ports: vec![egress_shim_port],
                runner_argv,
                env,
                route,
            })
            .await?;

        self.bridge_relaunched_jail(RelaunchedJailBridge {
            session_id,
            session_dir,
            worktree_path,
            egress_dir,
            managed: None,
            session_env,
            ready_marker,
            handle,
        })
        .await
    }
}
