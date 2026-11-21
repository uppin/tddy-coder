//! The daemon's [`TabJailSpawn`]: a jailed shell tab's jail, launched from the session's own jail
//! closure (`#session-liveness` 5/8).
//!
//! The jail is lowered as the session's main jail is ([`lower_tool_jail`] over the session agent's
//! closure, [`build_tool_jail_plan`], [`spawn_sandbox_plan`], the host relay), so it has the same
//! worktree mount, egress policy and tool relay — but its runner execs the session user's login
//! shell — the one [`TabJailLaunch::login_shell`] names, the session's OS user's — and every file it
//! writes is the tab's own ([`TabJailLaunch::paths`]). It shares the extracted pieces of
//! [`LaunchSessions::launch_jailed`] rather than growing it.
//!
//! **The grants are lowered on the session's provider route**, re-resolved from the owner's vault as
//! a resume's is ([`tab_route`]): the route's host joins the shell's egress, and a `when: unrouted`
//! item is dropped, exactly as in the main jail. The route's credentials are not given to the
//! shell: they belong to the agent.
//!
//! **A tab jail's records are the main's alone.** It writes no usage context and no capture
//! recording: the session's usage is its agent's, and its capture is its agent's terminal.
//!
//! [`lower_tool_jail`]: tddy_daemon_sandbox::tool_jail::lower_tool_jail
//! [`build_tool_jail_plan`]: tddy_daemon_sandbox::tool_jail::build_tool_jail_plan
//! [`spawn_sandbox_plan`]: tddy_daemon_sandbox::sandbox_session::spawn_sandbox_plan
//!
//! PRD: docs/ft/daemon/session-liveness.md (A sandboxed session's tabs)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tddy_core::provider_route::HarnessRoute;
use tddy_daemon_sandbox::sandbox_session::{
    dial_and_bridge_with_egress, pick_free_loopback_port, sandbox_error_to_status,
    spawn_sandbox_plan, wait_for_sandbox_ready, SandboxRunnerSpawn, SandboxSessionState,
    SandboxSessionStateInit,
};
use tddy_daemon_sandbox::tab_jails::{TabJailLaunch, TabJailPaths, TabJailSpawn};
use tddy_daemon_sandbox::tool_jail::{
    build_tool_jail_plan, lower_tool_jail, ToolJailGrants, ToolJailInputs,
};
use tddy_rpc::Status;
use tddy_sandbox::{EgressPolicy, MountSpec};
use tddy_sandbox_runner::{EgressGuard, ExecPlan};
use tddy_task::TerminalCapture;

use crate::cli_harness_jail::{closure_defs, require_jail_route};
use crate::harness_from_spec::{cli_launcher, resolve_session_agent, CliSessionAgent};
use crate::jail_launch::{
    host_home, jail_path, runner_argv, write_exec_plan, RunnerArgv, JAIL_READY_TIMEOUT,
};
use crate::launch_ports::LaunchSessions;
use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::resume_route;
use crate::svc_start_sandboxed_claude_cli_session::JailDirs;

impl LaunchSessions {
    /// What starts a tab's jail: the double a test gave the host, or this topic's own launch.
    pub fn tab_jail_spawn(&self) -> Arc<dyn TabJailSpawn> {
        match &self.tab_jail_spawn {
            Some(spawn) => Arc::clone(spawn),
            None => Arc::new(self.clone()),
        }
    }
}

#[async_trait]
impl TabJailSpawn for LaunchSessions {
    async fn spawn_shell(
        &self,
        launch: &TabJailLaunch,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        let session_agent = self.session_agent_of_tab(launch)?;
        let defs = closure_defs(session_agent.agents.registry(), &session_agent.agent)?;
        let closure: Vec<_> = session_agent
            .agent
            .deps
            .iter()
            .chain([&session_agent.agent.tool])
            .cloned()
            .collect();
        log::debug!(
            "tab {} of session {}: a jail running {} for {}",
            launch.terminal_id,
            launch.session_id,
            launch.login_shell,
            launch.os_user
        );
        let dirs = prepare_tab_dirs(launch)?;
        let route = tab_route(
            &self.host.subject_account_access(&launch.subject),
            &session_agent,
            &launch.session_dir,
        )?;
        // The session's grants, lowered as its main jail's are, on the route its main runs on.
        let mut grants = lower_tool_jail(&ToolJailInputs {
            tool: &session_agent.agent.tool,
            closure: &closure,
            defs: &defs,
            tddy_home: &self.tddy_data_dir,
            host_home: &host_home()?,
            scratch_dir: &dirs.scratch_dir,
            host: session_agent.agents.host(),
            route: route.as_ref(),
        })?;
        // The session's worktree is the shell's whole point: mounted writable, as the tab's cwd.
        grants.mounts.push(MountSpec::read_write(&launch.worktree));
        self.run_tab_jail(launch, &dirs, grants, &closure_ids(&closure))
            .await
    }
}

impl LaunchSessions {
    /// The coding agent whose grants a tab of `launch`'s session runs on: the session must be
    /// sandboxed and run a CLI coding agent, or there are no grants to lower.
    fn session_agent_of_tab(&self, launch: &TabJailLaunch) -> Result<CliSessionAgent, Status> {
        let metadata = tddy_core::read_session_metadata(&launch.session_dir).map_err(|error| {
            Status::internal(format!(
                "read the record of session {}: {error}",
                launch.session_id
            ))
        })?;
        if metadata.sandbox != Some(true) {
            return Err(Status::failed_precondition(format!(
                "session {} is not sandboxed, so it has no grants to run a tab jail with",
                launch.session_id
            )));
        }
        let session_type = metadata.session_type.as_deref().unwrap_or_default();
        let kind = cli_launcher(Some(&self.coding_agents), session_type)?.ok_or_else(|| {
            Status::failed_precondition(format!(
                "session {} runs no coding agent ({session_type:?}), so it has no tool grants \
                 to run a tab jail with",
                launch.session_id
            ))
        })?;
        resolve_session_agent(&self.coding_agents, kind)
    }
}

/// The provider route the session's tab jail is lowered on: the account the session's start
/// recorded, re-resolved from the owner's vault as it stands now, as a resume re-resolves it.
/// `None` for a session on its agent's own vendor; a session whose account is gone or whose vault is
/// locked is refused, as its resume is — a shell on the vendor's egress would be a wider jail than
/// the session's own.
pub(crate) fn tab_route(
    access: &SessionAccountAccess,
    session_agent: &CliSessionAgent,
    session_dir: &Path,
) -> Result<Option<HarnessRoute>, Status> {
    require_jail_route(
        session_agent.kind.name,
        &session_agent.agent.spec,
        resume_route(access, (&session_agent.kind.route).into(), session_dir)?,
    )
}

fn closure_ids(closure: &[tddy_actions::tool::ResolvedTool]) -> Vec<String> {
    closure.iter().map(|tool| tool.id.clone()).collect()
}

/// The directories a tab jail's runner is given, created and resolved to their canonical paths
/// (the profile and the socket bind are evaluated against resolved paths, as the main's are).
fn prepare_tab_dirs(launch: &TabJailLaunch) -> Result<JailDirs, Status> {
    let paths = &launch.paths;
    let context_dir = launch.session_dir.join("sandbox").join("context");
    for dir in [
        paths.scratch_dir.join("home"),
        paths.scratch_dir.join("tmp"),
        paths.egress_dir.clone(),
        context_dir.clone(),
    ] {
        std::fs::create_dir_all(&dir)
            .map_err(|e| Status::internal(format!("mkdir {}: {e}", dir.display())))?;
    }
    let canonical = |dir: &Path| std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let sandbox_root = canonical(&paths.root);
    let scratch_dir = sandbox_root.join(".work");
    Ok(JailDirs {
        egress_dir: canonical(&paths.egress_dir),
        scratch_tmp: scratch_dir.join("tmp"),
        scratch_dir,
        context_dir: canonical(&context_dir),
        sandbox_root,
    })
}

/// `path` of `paths`, re-rooted at the canonical `root` of `dirs`.
fn under_canonical_root(paths: &TabJailPaths, dirs: &JailDirs, path: &Path) -> PathBuf {
    match path.strip_prefix(&paths.root) {
        Ok(relative) => dirs.sandbox_root.join(relative),
        Err(_) => path.to_path_buf(),
    }
}

/// What the tab jail's runner execs: the login shell, with the grants' env and tool links first on
/// its `PATH`.
fn shell_exec_plan(launch: &TabJailLaunch, grants: &ToolJailGrants, egress_shim: bool) -> ExecPlan {
    let mut env: BTreeMap<String, String> = grants.env.clone();
    env.insert("PATH".to_string(), jail_path(&grants.bin_dir));
    ExecPlan {
        argv: vec![launch.login_shell.clone()],
        env,
        files: Vec::new(),
        egress_shim,
    }
}

/// A tab jail's runner, planned and not yet spawned: its plan, the marker it writes when ready, and
/// the guard that holds its relayed tunnels to its egress policy.
struct PlannedTabJail {
    plan: tddy_sandbox::SandboxPlan,
    ready_marker: PathBuf,
    egress_guard: EgressGuard,
}

impl LaunchSessions {
    /// Spawn the runner of `launch` with `grants`, wait for it, bridge its terminal, and return its
    /// state — the tab jail's counterpart of the main's launch, over the tab's own files.
    async fn run_tab_jail(
        &self,
        launch: &TabJailLaunch,
        dirs: &JailDirs,
        grants: ToolJailGrants,
        tool_ids: &[String],
    ) -> Result<Arc<SandboxSessionState>, Status> {
        let PlannedTabJail {
            plan,
            ready_marker,
            egress_guard,
        } = self.plan_tab_jail(launch, dirs, grants, tool_ids)?;
        let mut handle = spawn_sandbox_plan(plan).map_err(|e| {
            let mut status = sandbox_error_to_status(e);
            status.message = format!("{}\n{}", status.message, egress_logs(dirs));
            status
        })?;
        wait_for_sandbox_ready(
            &mut handle,
            &ready_marker,
            JAIL_READY_TIMEOUT,
            &dirs.egress_dir,
        )
        .await
        .map_err(|e| {
            let logs = egress_logs(dirs);
            Status::deadline_exceeded(format!("wait for the tab jail to be ready: {e}\n{logs}"))
        })?;
        self.bridge_tab_terminal(launch, dirs, handle, ready_marker, egress_guard)
            .await
    }

    /// Write the shell's exec plan and plan the runner over the tab's own files: socket, marker, plan
    /// and profile re-rooted under the canonical sandbox root, no stale socket or marker of an earlier
    /// jail of this tab left to be read as this one's.
    fn plan_tab_jail(
        &self,
        launch: &TabJailLaunch,
        dirs: &JailDirs,
        grants: ToolJailGrants,
        tool_ids: &[String],
    ) -> Result<PlannedTabJail, Status> {
        let paths = &launch.paths;
        let session_id = launch.session_id.as_str();
        let ready_marker = under_canonical_root(paths, dirs, &paths.ready_marker);
        let exec_plan_path = under_canonical_root(paths, dirs, &paths.exec_plan);
        let shim_port = match grants.egress {
            EgressPolicy::None => None,
            EgressPolicy::Any | EgressPolicy::Hosts(_) => {
                Some(pick_free_loopback_port().map_err(Status::internal)?)
            }
        };
        write_exec_plan(
            &exec_plan_path,
            &shell_exec_plan(launch, &grants, shim_port.is_some()),
        )?;
        let egress_guard = EgressGuard::new(grants.egress.clone(), session_id, tool_ids.to_vec());
        let _ = std::fs::remove_file(&paths.ipc_socket);
        let _ = std::fs::remove_file(&ready_marker);
        let mut runner_argv = runner_argv(RunnerArgv {
            session_id,
            dirs,
            ipc_socket: &paths.ipc_socket,
            ready_marker: &ready_marker,
            exec_plan: &exec_plan_path,
            shim_port,
            egress: &grants.egress,
            tool_ids,
        });
        runner_argv.extend([
            "--cwd".to_string(),
            launch.worktree.to_string_lossy().into_owned(),
        ]);
        let spawn = self.tab_runner_spawn(launch, dirs, runner_argv, &grants, shim_port);
        let plan = build_tool_jail_plan(spawn, grants).map_err(sandbox_error_to_status)?;
        Ok(PlannedTabJail {
            plan,
            ready_marker,
            egress_guard,
        })
    }

    /// The runner spawn of a tab's jail: the tab's own socket, profile and scratch, the grants' mounts,
    /// and the host's cgroup delegation.
    fn tab_runner_spawn(
        &self,
        launch: &TabJailLaunch,
        dirs: &JailDirs,
        runner_argv: Vec<String>,
        grants: &ToolJailGrants,
        shim_port: Option<u16>,
    ) -> SandboxRunnerSpawn {
        let paths = &launch.paths;
        SandboxRunnerSpawn {
            project_root: dirs.sandbox_root.clone(),
            scratch_dir: dirs.scratch_dir.clone(),
            egress_dir: dirs.egress_dir.clone(),
            profile_path: under_canonical_root(paths, dirs, &paths.profile),
            runner_argv,
            env: tddy_sandbox::scratch_runner_env(
                &dirs.scratch_dir.join("home"),
                &dirs.scratch_tmp,
                &launch.session_id,
                &paths.ipc_socket,
                &dirs.egress_dir,
            ),
            loopback_allow_ports: shim_port.into_iter().collect(),
            ipc_socket: Some(paths.ipc_socket.clone()),
            mounts: grants.mounts.clone(),
            host_home: None,
            cgroup: self.config.sandbox_cgroup_config(),
            // The route's env and credentials are the agent's: the shell's jail is lowered on the
            // route's egress ([`tab_route`]) and is given none of them.
            route: None,
            context_dir: Some(dirs.context_dir.clone()),
            stdio: true,
        }
    }

    /// Dial the ready runner's `SessionChannel`, bridge its terminal and relay, and return the tab
    /// jail's state: its capture is the tab's own, and nothing of it is recorded.
    async fn bridge_tab_terminal(
        &self,
        launch: &TabJailLaunch,
        dirs: &JailDirs,
        mut handle: tddy_sandbox::SandboxHandle,
        ready_marker: PathBuf,
        egress_guard: EgressGuard,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        let session_id = launch.session_id.as_str();
        let (stdout_tx, _) = tokio::sync::broadcast::channel(256);
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();
        dial_and_bridge_with_egress(
            session_id,
            launch.worktree.clone(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(Vec::new()),
            launch.session_dir.clone(),
            Arc::clone(&self.agent_activity_hub),
            self.host
                .sandbox_rpc_handler(session_id, &launch.session_dir),
            egress_guard,
        )
        .await
        .map_err(|e| {
            let logs = egress_logs(dirs);
            Status::internal(format!("dial the tab jail's SessionChannel: {e}\n{logs}"))
        })?;
        let pid = handle.pid();
        Ok(Arc::new(SandboxSessionState::new(
            SandboxSessionStateInit {
                pid,
                worktree_path: launch.worktree.clone(),
                stdout_tx,
                capture,
                stdin_tx,
                ready_marker,
                handle,
                managed_workflow: None,
            },
        )))
    }
}

/// The tab jail's egress log, for an error naming why its runner did not come up.
fn egress_logs(dirs: &JailDirs) -> String {
    tddy_sandbox::format_egress_logs(&dirs.egress_dir)
}
