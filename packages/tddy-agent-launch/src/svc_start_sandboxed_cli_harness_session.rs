//! Sandboxed start of a registered CLI harness that is cut a worktree of its own — OpenCode
//! (`opencode-cli`) and Muse Code (`muse-cli`) — hosted in a jail by `tddy-sandbox-runner`
//! (`--agent-kind opencode | muse`).
//!
//! The same shape as a sandboxed Cursor session — a daemon-cut worktree, a jail whose runner relays
//! the harness's PTY over the daemon's own connection — with the harness's own argv, environment
//! and route. A resume of such a session relaunches its jail with `--resume` (see
//! `svc_resume_sandboxed_cli_harness_session`), never the host PTY.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tddy_core::cli_harness::CliHarness;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_rpc::{Response, Status};
use tddy_sandbox::MountSpec;
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_agents::seed_codebase::{SeedCodebase, SeededAgentClones};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_task::TerminalCapture;
use tddy_worktree_service::branch_intent::{BranchIntentPolicy, BranchIntentRequest};

use crate::cli_harness_jail::{
    cli_harness_runner_argv, refuse_first_prompt, require_provider_route,
    resolve_cli_jail_binaries, sandboxed_runner_model, CliRunnerArgv,
};
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::launch_ports::LaunchSessions;
use crate::relaunch_jail_dirs::{prepare_relaunch_dirs, refresh_relaunch_context_dir};
use crate::session_provider_route::{sandboxed_start_route, session_route_sources_with_default};
use crate::stack_parent::SpawnStackParent;
use crate::svc_start_session_core::CliStart;

/// What a sandboxed CLI harness start reads: the harness and request, the session's start, and the
/// provider route the caller resolved before anything was created.
pub(crate) struct SandboxedCliHarnessStart<'a> {
    pub harness: &'static CliHarness,
    pub req: &'a StartSessionRequest,
    pub os_user: &'a str,
    pub start: CliStart,
    /// The provider route the jail runs the harness on.
    pub route: HarnessRoute,
    pub progress: &'a AttachmentProgressSink,
}

impl LaunchSessions {
    /// The sandboxed start of a CLI harness: its provider route resolved before anything is created,
    /// then the start in its jail.
    pub(crate) async fn start_sandboxed_cli_harness_from_request(
        &self,
        harness: &'static CliHarness,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        refuse_first_prompt(harness, &start.initial_prompt)?;
        let access = self.host.session_account_access(&req.session_token);
        let sources = session_route_sources_with_default(req, harness.session_type, &access)?;
        let session_dir = start
            .sessions_base
            .join(SESSIONS_SUBDIR)
            .join(&start.session_id);
        let route = require_provider_route(
            harness,
            sandboxed_start_route(
                &access,
                harness.route_harness,
                &sources,
                req.model.trim(),
                &session_dir,
            )?,
        )?;
        self.start_sandboxed_cli_harness_session(SandboxedCliHarnessStart {
            harness,
            req,
            os_user,
            start,
            route,
            progress,
        })
        .await
    }

    async fn start_sandboxed_cli_harness_session(
        &self,
        start: SandboxedCliHarnessStart<'_>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let SandboxedCliHarnessStart {
            harness,
            req,
            os_user,
            start,
            route,
            progress,
        } = start;
        let session_id = start.session_id.as_str();
        let session_token = req.session_token.as_str();
        let project_id = req.project_id.trim();
        let session_dir = start.sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;

        // The roster and its endpoints are resolved and warmed before anything is cut or spawned, as
        // a sandboxed claude or cursor start does: an unreachable subagent fails the start here.
        let mut started_agents = self
            .agent_roster
            .seeded_roster_records_placed(&req.specialized_agents, &req.clone_placed_agents)
            .await?;
        let specialized_defs = self
            .agent_roster
            .resolve_specialized_agent_defs(&req.specialized_agents)
            .await?;
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let stack_parent = match Some(req.stack_parent.trim()).filter(|s| !s.is_empty()) {
            Some(parent) => SpawnStackParent::OwnedBy {
                session_id: parent,
                daemon_instance_id: req.stack_parent_daemon_instance_id.trim(),
                stack_node_id: req.stack_node_id.trim(),
                session_token,
                host: self,
            },
            None => SpawnStackParent::NoParent,
        };
        let intent = tddy_session_split::service_util::write_initial_changeset(
            session_id,
            &BranchIntentRequest {
                branch_worktree_intent: req.branch_worktree_intent.trim(),
                new_branch_name: req.new_branch_name.trim(),
                selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
            },
            BranchIntentPolicy::cli_harness(harness.session_type),
            None,
            &session_dir,
            stack_parent.session_id(),
            None,
        )?;
        let CliWorktree {
            path: worktree_path,
            ..
        } = provision_cli_worktree(
            &CliWorktreeStart {
                session_type: harness.session_type,
                cut_label: "start_sandboxed_cli_harness_session: create worktree",
                session_id,
                session_dir: &session_dir,
                sessions_base: &start.sessions_base,
                tddy_data_dir: &self.tddy_data_dir,
                os_user,
                project_id,
                repo_path: req.repo_path.trim(),
                branch_worktree_intent: req.branch_worktree_intent.trim(),
                new_branch_name: req.new_branch_name.trim(),
                selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
                create_remote_branch: req.create_remote_branch,
                intent,
                timeout: self.config.spawn_worker_request_timeout(),
            },
            &stack_parent,
            progress,
        )
        .await?;

        // The clones this session's seeded agents read, claimed now: the worktree they mirror exists.
        // The jail does not yet put the tddy-tools tools in front of the harness's own tools, so a
        // withdrawal is not enforced here.
        let clones = tddy_session_agents::agent_host_callbacks::DaemonSeedCloneClaimant {
            service: self.agent_roster.clone(),
        };
        let seeded_clones = clones
            .claim_for_seed(
                session_id,
                &SeedCodebase::of_a_starting_session(
                    session_dir.clone(),
                    worktree_path.clone(),
                    project_id,
                    false,
                ),
                session_token,
                &mut started_agents,
            )
            .await?;

        let (sandbox_root, egress_dir, scratch_dir, scratch_tmp, context_dir) =
            prepare_relaunch_dirs(&session_dir)?;
        // The jail's $HOME is the session's own scratch, which lives in the session directory: a
        // resume finds the harness's session store there.
        let scratch_home = scratch_dir.join("home");
        refresh_relaunch_context_dir(
            &worktree_path,
            &started_agents,
            &context_dir,
            harness.session_type,
        )?;

        let binaries = resolve_cli_jail_binaries(&self.config, harness)?;
        let runner_model = sandboxed_runner_model(harness, &route, req.model.trim())?;

        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
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
            model: &runner_model,
            egress_shim_port,
            resume: false,
        });

        let semantic_env = semantic_index_env(
            &self.tddy_data_dir,
            session_id,
            req.semantic_index,
            &self.task_registry,
            &session_dir,
            &worktree_path,
            progress,
        )
        .await?;
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
        env.extend(self.lsp_tools_env(&worktree_path));
        env.extend(self.restructure_tools_env());
        env.extend(semantic_env);

        // The jailed agent's commits carry the project's account identity, carried by the relay that
        // runs under this daemon's environment rather than the agent's.
        let identity = self
            .host
            .session_identity(os_user, session_id, project_id, session_token);
        let (session_env, _github_credential) = self.jail_relay_identity(identity, os_user)?;

        progress.begin_phase(StartStep::Agent);
        let mut handle = tddy_daemon_sandbox::sandbox_session::spawn_sandbox_runner(
            tddy_daemon_sandbox::sandbox_session::SandboxRunnerSpawn {
                project_root: sandbox_root.clone(),
                scratch_dir: scratch_dir.clone(),
                egress_dir: egress_dir.clone(),
                profile_path,
                runner_argv,
                env,
                loopback_allow_ports: vec![egress_shim_port],
                ipc_socket: Some(tool_ipc_socket.clone()),
                mounts: vec![MountSpec::read_write(scratch_home.clone())],
                host_home: None,
                cgroup: self.config.sandbox_cgroup_config(),
                route: Some(route),
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
            Duration::from_secs(120),
            &egress_dir,
        )
        .await
        .map_err(Status::deadline_exceeded)?;

        let pid = self
            .bridge_and_register_cli_harness_jail(
                session_id,
                &session_dir,
                &worktree_path,
                ready_marker,
                session_env,
                handle,
            )
            .await?;
        progress.end_phase(StartStep::Agent);

        let meta = SessionMetadata {
            repo_path: Some(worktree_path.to_string_lossy().to_string()),
            pid: Some(pid),
            model: Some(req.model.trim().to_string()),
            sandbox: Some(true),
            agents_rev: tddy_session_agents::agent_roster::started_roster_rev(&started_agents),
            agents: started_agents,
            ..tddy_session_split::service_util::starting_session_metadata(
                session_id,
                project_id,
                harness.session_type,
            )
        };
        write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
        // The roster naming them is on disk now, so the clones belong to the session.
        seeded_clones.keep();

        log::info!(
            target: "tddy_daemon::connection_service",
            "started sandboxed {} session {session_id} pid={pid} worktree={} user={os_user}",
            harness.session_type,
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

    /// Bridge a ready jail's runner into this daemon over its stdio and register the session's
    /// sandbox state; returns the jail's pid.
    async fn bridge_and_register_cli_harness_jail(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_path: &Path,
        ready_marker: PathBuf,
        session_env: Vec<(String, String)>,
        mut handle: tddy_sandbox::SandboxHandle,
    ) -> Result<u32, Status> {
        let (stdout_tx, _) = tokio::sync::broadcast::channel(256);
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();
        tddy_daemon_sandbox::sandbox_session::dial_and_bridge(
            session_id,
            worktree_path.to_owned(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(session_env),
            session_dir.to_owned(),
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
                    worktree_path: worktree_path.to_owned(),
                    stdout_tx,
                    capture,
                    stdin_tx,
                    ready_marker,
                    handle,
                    managed_workflow: None,
                },
            ),
        );
        self.sandbox_manager
            .insert(session_id.to_string(), state)
            .await;
        Ok(pid)
    }
}
