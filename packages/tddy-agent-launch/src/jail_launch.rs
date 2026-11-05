//! The shared jail launcher: a coding agent declared as a tool is run in a sandbox from its
//! definition alone.
//!
//! Feature: `docs/ft/daemon/claude-cli-session.md` § The jail from the `claude` tool definition (`#agent-tools 5/12`).
//!
//! [`LaunchSessions::launch_jailed`] takes the agent's resolved closure ([`JailSpec`]), renders its
//! argv and slots for jail paths, lowers the closure's `home`, `files`, `secrets`, `policy`,
//! `network` and tools into one plan, writes the runner's [`ExecPlan`] to
//! `<session>/sandbox/`[`EXEC_PLAN_FILE`] and spawns `tddy-sandbox-runner --exec-plan` — then waits
//! for ready, bridges and registers the session as the hand-built launches did. Nothing in this
//! path names an agent: the sandboxed `claude-cli` start and relaunch are its first callers, and
//! `#agent-tools 7/12` and `8/12` move the other jailed CLIs onto it.
//!
//! [`ExecPlan`]: tddy_sandbox_runner::ExecPlan

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use tddy_actions::tool::{
    render_argv, HomeSection, ResolvedTool, TemplateParams, ToolDef, ToolHost,
};
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_core::provider_route::HarnessRoute;
use tddy_daemon_sandbox::sandbox_session::{
    dial_and_bridge_with_egress, pick_free_loopback_port, resolve_sandbox_runner_path,
    sandbox_error_to_status, spawn_sandbox_plan, wait_for_sandbox_ready, SandboxRunnerSpawn,
    SandboxSessionState, SandboxSessionStateInit,
};
use tddy_daemon_sandbox::tool_jail::{
    build_tool_jail_plan, lower_tool_jail, ToolJailGrants, ToolJailInputs,
};
use tddy_rpc::Status;
use tddy_sandbox::EgressPolicy;
use tddy_sandbox_recipes::homes::{persistent_home, HomeError};
use tddy_sandbox_runner::{EgressGuard, ExecPlan, ExecPlanFile};
use tddy_task::TerminalCapture;

use crate::claude_cli_spawn_steps::record_role_of_session_type_before_spawn;
use crate::cli_harness_jail::canonical_exec_path;
use crate::launch_ports::LaunchSessions;
use crate::spec_launch::HostAgent;
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};

/// The runner's plan, written under the session's sandbox root (`<session>/sandbox/exec-plan.json`).
pub const EXEC_PLAN_FILE: &str = "exec-plan.json";

/// How long a jail is given to write its ready marker.
const JAIL_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// `RUST_LOG` of the in-jail `tddy-tools --mcp` server: captures specialized-subagent turns and HTTP
/// activity, so failures land in the persisted log.
const MCP_RUST_LOG: &str = "info,tddy_tools=debug,tddy_discovery=debug";

/// The agent a jail runs: its resolution on the host and the definitions of its closure.
pub struct JailSpec<'a> {
    pub agent: &'a HostAgent,
    /// The definition of the agent's own tool and of each of its deps — what `home`, `files`,
    /// `secrets`, `policy` and `network` are lowered from.
    pub defs: &'a [ToolDef],
    /// Where a `host-env` secret is read: the host the agent was resolved against.
    pub host: &'a dyn ToolHost,
}

/// The session a jail is launched for, beyond what [`JailSpec`] declares.
pub struct JailedSession<'a> {
    /// The CLI session type the jail runs (`.session.yaml`'s `session_type`), which the role's early
    /// record names.
    pub session_type: &'a str,
    pub jail: &'a JailSession<'a>,
    pub dirs: &'a JailDirs,
    /// Host-side env the session's relayed commands run with (a managed workflow's `TDDY_SOCKET`).
    pub session_env: Vec<(String, String)>,
    /// Env the daemon adds for the session, on top of the plan's: subagents, LSP, the semantic
    /// index, the renderer choice. Its `TDDY_*` entries belong to the session's `tddy-tools` server
    /// ([`mcp_server_env`]); the rest is set on the agent process.
    pub env: BTreeMap<String, String>,
    pub managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    /// Recorded in the session's metadata with the role, as the host launch does.
    pub managed_codebase: bool,
    /// The provider route the agent runs on; `None` for its own vendor. A route's base-URL host
    /// joins the jail's egress policy, and `when: unrouted` items are dropped.
    pub route: Option<HarnessRoute>,
}

/// The AF_UNIX socket the session's in-jail `tddy-tools` dials. It must fit `SUN_LEN`, so it is a
/// short out-of-tree path the jail's profile grants explicitly.
pub(crate) fn tool_ipc_socket(session_id: &str) -> PathBuf {
    tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id)
}

/// The `env` block of the `tddy-tools --mcp` server a jailed agent registers: what it needs to reach
/// the host — the session's tool-IPC socket and id — where it logs, and the daemon's `TDDY_*`
/// additions for the session. The agent itself is told none of it.
pub(crate) fn mcp_server_env(
    session_id: &str,
    dirs: &JailDirs,
    session_env: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let path = |path: &Path| path.to_string_lossy().into_owned();
    let mut env = BTreeMap::from([
        (
            "TDDY_SANDBOX_TOOL_IPC".to_string(),
            path(&tool_ipc_socket(session_id)),
        ),
        (
            "TDDY_SANDBOX_SESSION_ID".to_string(),
            session_id.to_string(),
        ),
        (
            "TDDY_TOOLS_LOG_FILE".to_string(),
            path(&dirs.egress_dir.join("tddy-tools.mcp.log")),
        ),
        (
            "TDDY_TOOLS_ACCOUNTING_FILE".to_string(),
            path(&dirs.egress_dir.join("accounting.json")),
        ),
        ("RUST_LOG".to_string(), MCP_RUST_LOG.to_string()),
    ]);
    env.extend(
        session_env
            .iter()
            .filter(|(name, _)| name.starts_with("TDDY_"))
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    env
}

impl LaunchSessions {
    /// Launch `spec` in a jail for `session` and return the runner's pid.
    ///
    /// `role` is the session's granted role (`project-home`; empty for none). It is written into
    /// the session's metadata before the spawn, as the host launch does, so a jailed project-home
    /// main is listed with its role and a resume re-applies it.
    pub async fn launch_jailed(
        &self,
        spec: JailSpec<'_>,
        params: &TemplateParams,
        slots: &RenderedSlots,
        session: JailedSession<'_>,
        role: &str,
    ) -> Result<u32, Status> {
        let JailSession {
            session_id,
            project_id,
            session_dir,
            worktree_path,
            ..
        } = *session.jail;
        record_role_of_session_type_before_spawn(
            session_dir,
            session_id,
            project_id,
            role,
            session.session_type,
            worktree_path,
            session.managed_codebase,
        )?;

        let closure: Vec<ResolvedTool> = spec
            .agent
            .deps
            .iter()
            .chain([&spec.agent.tool])
            .cloned()
            .collect();
        let grants = lower_tool_jail(&ToolJailInputs {
            tool: &spec.agent.tool,
            closure: &closure,
            defs: spec.defs,
            tddy_home: &self.tddy_data_dir,
            host_home: &host_home()?,
            scratch_dir: &session.dirs.scratch_dir,
            host: spec.host,
            route: session.route.as_ref(),
        })?;

        let shim_port = match grants.egress {
            EgressPolicy::None => None,
            EgressPolicy::Any | EgressPolicy::Hosts(_) => {
                Some(pick_free_loopback_port().map_err(Status::internal)?)
            }
        };
        let exec_plan_path = session.dirs.sandbox_root.join(EXEC_PLAN_FILE);
        let plan = exec_plan(&spec, params, slots, &session, &grants, shim_port.is_some())?;
        write_exec_plan(&exec_plan_path, &plan)?;

        let tool_ids: Vec<String> = closure.iter().map(|tool| tool.id.clone()).collect();
        let egress_guard = EgressGuard::new(grants.egress.clone(), session_id, tool_ids.clone());
        let ready_marker = session.dirs.sandbox_root.join("sandbox.ready");
        let ipc_socket = tool_ipc_socket(session_id);
        // A stale socket or marker of an earlier jail of this session would be read as this one's.
        let _ = std::fs::remove_file(&ipc_socket);
        let _ = std::fs::remove_file(&ready_marker);
        let runner_argv = runner_argv(RunnerArgv {
            session_id,
            dirs: session.dirs,
            ipc_socket: &ipc_socket,
            ready_marker: &ready_marker,
            exec_plan: &exec_plan_path,
            shim_port,
            egress: &grants.egress,
            tool_ids: &tool_ids,
        });
        let jail_env = tddy_sandbox::scratch_runner_env(
            &session.dirs.scratch_dir.join("home"),
            &session.dirs.scratch_tmp,
            session_id,
            &ipc_socket,
            &session.dirs.egress_dir,
        );
        let spawn = SandboxRunnerSpawn {
            project_root: session.dirs.sandbox_root.clone(),
            scratch_dir: session.dirs.scratch_dir.clone(),
            egress_dir: session.dirs.egress_dir.clone(),
            profile_path: session.dirs.sandbox_root.join("sandbox.sb"),
            runner_argv,
            env: jail_env,
            loopback_allow_ports: shim_port.into_iter().collect(),
            ipc_socket: Some(ipc_socket),
            mounts: grants.mounts.clone(),
            // The home is seeded by its definition's `files`, not copied per session.
            host_home: None,
            cgroup: self.config.sandbox_cgroup_config(),
            route: session.route,
            context_dir: Some(session.dirs.context_dir.clone()),
            stdio: true,
        };
        let egress_dir = &session.dirs.egress_dir;
        let plan = build_tool_jail_plan(spawn, grants).map_err(sandbox_error_to_status)?;
        let mut handle = spawn_sandbox_plan(plan).map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(egress_dir);
            let mut status = sandbox_error_to_status(e);
            status.message = format!("{}\n{logs}", status.message);
            status
        })?;
        wait_for_sandbox_ready(&mut handle, &ready_marker, JAIL_READY_TIMEOUT, egress_dir)
            .await
            .map_err(|e| {
                let logs = tddy_sandbox::format_egress_logs(egress_dir);
                Status::deadline_exceeded(format!("wait for sandbox ready: {e}\n{logs}"))
            })?;

        let (stdout_tx, _) = tokio::sync::broadcast::channel(256);
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();
        dial_and_bridge_with_egress(
            session_id,
            worktree_path.to_path_buf(),
            &mut handle,
            self.task_registry.clone(),
            stdout_tx.clone(),
            Arc::clone(&capture),
            stdin_rx,
            Arc::new(session.session_env),
            session_dir.to_path_buf(),
            Arc::clone(&self.agent_activity_hub),
            self.host.sandbox_rpc_handler(session_id, session_dir),
            egress_guard,
        )
        .await
        .map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(egress_dir);
            Status::internal(format!("dial sandbox SessionChannel: {e}\n{logs}"))
        })?;
        let pid = handle.pid();
        let state = Arc::new(SandboxSessionState::new(SandboxSessionStateInit {
            pid,
            worktree_path: worktree_path.to_path_buf(),
            stdout_tx,
            capture,
            stdin_tx,
            ready_marker,
            handle,
            managed_workflow: session.managed.map(|workflow| {
                Box::new(workflow)
                    as Box<dyn tddy_daemon_sandbox::sandbox_session::SessionScopedResource>
            }),
        }));
        self.sandbox_manager
            .insert(session_id.to_string(), state)
            .await;
        self.record_jailed_main(session_id).await;
        Ok(pid)
    }
}

/// The `HOME` a jail of `def` runs with, resolved as [`LaunchSessions::launch_jailed`] resolves it,
/// so a placed slot naming `{home}` can be rendered before the jail is lowered: the persistent
/// `<tddyhome>/homes/<key>`, or `session_home` for a home that lives and dies with the session.
pub(crate) fn jail_home(
    def: &ToolDef,
    tddy_home: &Path,
    session_home: &Path,
) -> Result<PathBuf, Status> {
    match &def.home {
        HomeSection::Persistent { key } => {
            persistent_home(tddy_home, key).map_err(|error| match error {
                HomeError::BothExist { .. } => Status::failed_precondition(error.to_string()),
                HomeError::Io { .. } => Status::internal(error.to_string()),
            })
        }
        HomeSection::Session => Ok(session_home.to_path_buf()),
    }
}

/// The host user's home, which a tool definition's `from_host` paths are relative to.
fn host_home() -> Result<PathBuf, Status> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or_else(|| {
            Status::failed_precondition(
                "HOME is not set to an absolute path: a jailed tool's files are seeded from it",
            )
        })
}

/// The plan the runner execs: the agent's argv rendered with `params` and the `slots`, its env, and
/// the slot files it reads.
fn exec_plan(
    spec: &JailSpec<'_>,
    params: &TemplateParams,
    slots: &RenderedSlots,
    session: &JailedSession<'_>,
    grants: &ToolJailGrants,
    egress_shim: bool,
) -> Result<ExecPlan, Status> {
    let params = params.clone().with_list("slots", slots.argv_extra.clone());
    let argv = render_argv(&spec.agent.spec.run, &spec.agent.tool, &params)
        .map_err(|e| Status::internal(format!("render the {} argv: {e}", spec.agent.spec.id)))?;
    let mut env: BTreeMap<String, String> = spec
        .agent
        .spec
        .run
        .env
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    // The daemon's additions, but for the `TDDY_*` ones the `tddy-tools` server is registered with.
    env.extend(
        session
            .env
            .iter()
            .filter(|(name, _)| !name.starts_with("TDDY_"))
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    env.extend(grants.env.clone());
    let baseline_path = tddy_sandbox::process_jail_env(Path::new(""), Path::new(""))
        .remove("PATH")
        .unwrap_or_default();
    env.insert(
        "PATH".to_string(),
        format!("{}:{baseline_path}", grants.bin_dir.display()),
    );
    Ok(ExecPlan {
        argv,
        env,
        files: slots
            .files
            .iter()
            .map(|(path, contents)| ExecPlanFile {
                path: path.clone(),
                contents: contents.clone(),
            })
            .collect(),
        egress_shim,
    })
}

fn write_exec_plan(path: &Path, plan: &ExecPlan) -> Result<(), Status> {
    let json = serde_json::to_vec_pretty(plan)
        .map_err(|e| Status::internal(format!("serialize the exec plan: {e}")))?;
    tddy_core::atomic_file::write_atomic(path, json)
        .map_err(|e| Status::internal(format!("write the exec plan {}: {e}", path.display())))
}

/// What the runner's argv is built from.
struct RunnerArgv<'a> {
    session_id: &'a str,
    dirs: &'a JailDirs,
    ipc_socket: &'a Path,
    ready_marker: &'a Path,
    exec_plan: &'a Path,
    shim_port: Option<u16>,
    egress: &'a EgressPolicy,
    tool_ids: &'a [String],
}

/// `tddy-sandbox-runner --exec-plan <file>` and the flags every jail takes. No flag names an agent.
fn runner_argv(argv: RunnerArgv<'_>) -> Vec<String> {
    let path = |path: &Path| path.to_string_lossy().into_owned();
    let mut runner_argv = vec![
        canonical_exec_path(&resolve_sandbox_runner_path()),
        "--session-id".to_string(),
        argv.session_id.to_string(),
        "--context-dir".to_string(),
        path(&argv.dirs.context_dir),
        "--tool-ipc-socket".to_string(),
        path(argv.ipc_socket),
        "--ready-marker".to_string(),
        path(argv.ready_marker),
        "--exec-plan".to_string(),
        path(argv.exec_plan),
        "--stdio".to_string(),
    ];
    if let Some(port) = argv.shim_port {
        runner_argv.extend([
            "--egress-shim-port".to_string(),
            port.to_string(),
            "--egress-policy".to_string(),
            argv.egress.to_runner_arg(),
        ]);
        for tool in argv.tool_ids {
            runner_argv.extend(["--egress-tool".to_string(), tool.clone()]);
        }
    } else {
        // No shim port: the jail's policy reaches no host. The runner is told so, or it starts the
        // shim its default policy implies and the profile (which grants no loopback bind) refuses it.
        runner_argv.extend(["--egress-policy".to_string(), argv.egress.to_runner_arg()]);
    }
    runner_argv
}
