//! What a sandboxed CLI agent's jail is launched from, decided from its spec before the jail exists: the
//! provider route it requires (its `route.required_in_jail`), the configuration variable its route's
//! file is named in (`route.config_env`), and the agent launched through
//! [`LaunchSessions::launch_jailed`] with its argv rendered from its `run:` template. Shared by the
//! start (`svc_start_sandboxed_cli_harness_session`) and the resume
//! (`svc_resume_sandboxed_cli_harness_session`).
//!
//! The placement a sandboxed start may run on is the spec's (`placements`), refused by
//! [`crate::harness_from_spec::require_spec_placement`] before anything is created.
//!
//! The functions below the `--agent-kind` line are the legacy jail that hosted OpenCode and Muse in
//! `tddy-sandbox-runner`'s PTY. The launch no longer calls them; they remain for the tests that pin
//! them until their removal (docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md).

use std::collections::BTreeMap;
use std::path::Path;

use tddy_actions::tool::TemplateParams;
use tddy_coding_agent_specs::slots::{self, SlotInputs};
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::cli_harness::CliHarness;
use tddy_core::provider_route::HarnessRoute;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;

use crate::claude_jail::closure_defs;
use crate::harness_from_spec::CliSessionAgent;
use crate::jail_launch::{JailSpec, JailedSession};
use crate::launch_ports::LaunchSessions;
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};

/// A sandboxed CLI agent's launch, as its start or resume has settled it.
pub(crate) struct CliJail<'a> {
    pub(crate) session_agent: &'a CliSessionAgent,
    pub(crate) jail: &'a JailSession<'a>,
    pub(crate) dirs: &'a JailDirs,
    /// The spec's `run:` template, bound for the launch (`harness_template_params`).
    pub(crate) params: TemplateParams,
    /// Arguments the provider route adds to the agent's argv: a start's, not a resume's.
    pub(crate) route_args: Vec<String>,
    /// What the daemon adds for the session: the agent's environment, its route's configuration
    /// variable among them (see [`jail_route_config_env`]).
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) session_env: Vec<(String, String)>,
    /// The provider route the agent runs on; `None` for its own setup.
    pub(crate) route: Option<HarnessRoute>,
}

impl LaunchSessions {
    /// Launch the CLI agent of `launch` in a jail and return the runner's pid. Its slots are the spec's
    /// own (no MCP registration, no tool lists, no hooks, no instruction files), and a route's
    /// arguments are the caller's to give or withhold.
    pub(crate) async fn launch_jailed_cli_agent(&self, launch: CliJail<'_>) -> Result<u32, Status> {
        let CliSessionAgent { agent, agents, .. } = launch.session_agent;
        let defs = closure_defs(agents.registry(), agent)?;
        let mut slots = slots::render(
            &agent.spec,
            &SlotInputs {
                workdir: launch.dirs.context_dir.clone(),
                scratch_dir: launch.dirs.scratch_tmp.clone(),
                mcp_server: None,
                allowed_tools: Vec::new(),
                denied_tools: Vec::new(),
                permission_prompt_tool: None,
                preamble: None,
                append_system_prompt_file: None,
                hooks: None,
                home: None,
            },
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        slots.argv_extra.extend(launch.route_args);
        self.launch_jailed(
            JailSpec {
                agent,
                defs: &defs,
                host: agents.host(),
            },
            &launch.params,
            &slots,
            JailedSession {
                jail: launch.jail,
                dirs: launch.dirs,
                session_env: launch.session_env,
                env: launch.env,
                managed: None,
                managed_codebase: false,
                route: launch.route,
            },
            "",
        )
        .await
    }
}

/// The provider route a sandboxed CLI agent runs on, required when its spec says its jail needs one.
///
/// A jail's `$HOME` is the session's own scratch, so the person's own OpenCode or Muse Code setup is
/// not inside it: without a provider account the agent would start with no credentials and could not
/// authenticate. Refused up front rather than started to fail. `None` for an agent whose jail needs no
/// route, which then runs on its own setup.
pub(crate) fn require_jail_route(
    session_type: &str,
    spec: &CodingAgentSpec,
    route: Option<HarnessRoute>,
) -> Result<Option<HarnessRoute>, Status> {
    let required = spec
        .agent
        .route
        .as_ref()
        .is_some_and(|slot| slot.required_in_jail);
    if required && route.is_none() {
        return Err(Status::failed_precondition(format!(
            "a sandboxed {session_type} session needs a provider account: its jail holds no \
             credentials of its own"
        )));
    }
    Ok(route)
}

/// The variable a routed jailed agent reads its configuration file from (`coding_agent.route.config_env`,
/// `OPENCODE_CONFIG`), pointed at the file the jail writes for the route: the first of the route's
/// files, under the scratch's `files_dir` (`provider-route`). Empty for an agent whose route is
/// environment only, and for a session on its own setup.
pub(crate) fn jail_route_config_env(
    spec: &CodingAgentSpec,
    route: Option<&HarnessRoute>,
    scratch_dir: &Path,
) -> Vec<(String, String)> {
    let Some(slot) = spec.agent.route.as_ref() else {
        return Vec::new();
    };
    let (Some(config_env), Some(files_dir), Some(config)) = (
        slot.config_env.as_ref(),
        slot.files_dir.as_ref(),
        route.and_then(|route| route.files.first()),
    ) else {
        return Vec::new();
    };
    let path = scratch_dir.join(files_dir).join(&config.name);
    vec![(config_env.clone(), path.to_string_lossy().into_owned())]
}

// TODO(harness-clis): orphaned from the launch, which names the agent's `--agent-kind` nowhere. Pinned
// by `sandboxed_cli_harness_tests`; removal is deferred: docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md
/// The runner's `--agent-kind` for a CLI harness session type the runner hosts in its PTY; `None`
/// for any other session type.
#[allow(dead_code)]
pub(crate) fn sandboxed_agent_kind(session_type: &str) -> Option<&'static str> {
    match session_type {
        "opencode-cli" => Some("opencode"),
        "muse-cli" => Some("muse"),
        _ => None,
    }
}

/// The runner's `--agent-kind` for `harness`, or the refusal of a sandboxed start of a harness the
/// runner does not host. Checked before a provider route is resolved, so the refusal names the
/// runner rather than a provider account the harness could never use.
#[allow(dead_code)]
pub(crate) fn require_sandbox_hosts(harness: &CliHarness) -> Result<&'static str, Status> {
    sandboxed_agent_kind(harness.session_type).ok_or_else(|| {
        Status::failed_precondition(format!(
            "the sandbox runner does not host {}",
            harness.session_type
        ))
    })
}

/// The model a sandboxed CLI harness is started or resumed with, on the runner's `--model`.
///
/// The runner refuses a blank model for OpenCode and Muse, and it has no channel for a route's
/// arguments. A route's own `--model` argument therefore stands in for the blank model a routed
/// OpenCode launch leaves to its configuration file: it is the same argument the unsandboxed launch
/// appends, so the jail runs the harness on the model the route names. Otherwise the session's model
/// is kept, and a session with none is refused.
#[allow(dead_code)]
pub(crate) fn sandboxed_runner_model(
    harness: &CliHarness,
    route: &HarnessRoute,
    model: &str,
) -> Result<String, Status> {
    // A routed launch of a config-file harness names the model in its configuration file, so it leaves
    // the command-line model blank (the rule `launch_model_for` applies to the spec's route).
    let launched = if harness.route_config_env.is_some() {
        ""
    } else {
        model
    };
    if !launched.is_empty() {
        return Ok(launched.to_string());
    }
    model_argument(&route.args)
        .map(str::to_string)
        .ok_or_else(|| {
            Status::invalid_argument(format!(
                "a sandboxed {} session needs a model: its jail's runner starts the harness on one",
                harness.session_type
            ))
        })
}

/// The value of the route's `--model` argument, when it names a non-blank one.
#[allow(dead_code)]
fn model_argument(args: &[String]) -> Option<&str> {
    args.windows(2)
        .find(|pair| pair[0] == "--model")
        .map(|pair| pair[1].as_str())
        .filter(|model| !model.trim().is_empty())
}

/// Refuses a first prompt for a sandboxed CLI harness: the runner hosts no first prompt for OpenCode
/// or Muse, so one would be dropped without a trace. A blank prompt is no prompt.
#[allow(dead_code)]
pub(crate) fn refuse_first_prompt(
    harness: &CliHarness,
    initial_prompt: &str,
) -> Result<(), Status> {
    if initial_prompt.trim().is_empty() {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "a first prompt cannot reach a sandboxed {} session: its jail hosts the harness without one",
        harness.session_type
    )))
}

/// The provider route a sandboxed CLI harness runs on, or a refusal when there is none.
#[allow(dead_code)]
pub(crate) fn require_provider_route(
    harness: &CliHarness,
    route: Option<HarnessRoute>,
) -> Result<HarnessRoute, Status> {
    route.ok_or_else(|| {
        Status::failed_precondition(format!(
            "a sandboxed {} session needs a provider account: its jail holds no credentials of \
             its own",
            harness.session_type
        ))
    })
}

/// The absolute path of the harness binary a jail runs. The sandbox profile grants reads to the
/// executable by its path, so a bare name is searched for on `path_var`, the `PATH` the daemon runs
/// under. A name that is not there is refused rather than handed to the jail unresolved. A name with
/// a separator is a path already, and is taken as it is named.
#[allow(dead_code)]
pub(crate) fn resolve_jail_agent_binary(binary: &str, path_var: &str) -> Result<String, Status> {
    if binary.contains('/') {
        return Ok(canonical_exec_path(binary));
    }
    std::env::split_paths(path_var)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
        .map(|found| canonical_exec_path(&found.to_string_lossy()))
        .ok_or_else(|| {
            Status::failed_precondition(format!(
                "{binary} is not on the daemon's PATH, so a sandboxed session cannot run it"
            ))
        })
}

/// The executables a sandboxed CLI harness's runner is started from, resolved as the sandbox profile
/// needs them, and the runner's `--agent-kind` for the harness.
#[allow(dead_code)]
pub(crate) struct CliJailBinaries {
    pub tddy_tools_path: String,
    pub sandbox_runner_path: String,
    pub agent_binary: String,
    pub agent_kind: &'static str,
}

/// Resolve the executables a sandboxed CLI harness's runner is started from.
#[allow(dead_code)]
pub(crate) fn resolve_cli_jail_binaries(
    config: &DaemonConfig,
    harness: &CliHarness,
) -> Result<CliJailBinaries, Status> {
    let agent_kind = require_sandbox_hosts(harness)?;
    Ok(CliJailBinaries {
        tddy_tools_path: canonical_exec_path(
            &tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
                tddy_daemon_kernel::config::resolve_cursor_cli_tddy_tools_path(config).as_deref(),
            ),
        ),
        sandbox_runner_path: canonical_exec_path(
            &tddy_daemon_sandbox::sandbox_session::resolve_sandbox_runner_path(),
        ),
        agent_binary: resolve_jail_agent_binary(
            &crate::opencode_cli_spawn::harness_binary(config, harness),
            &std::env::var("PATH").unwrap_or_default(),
        )?,
        agent_kind,
    })
}

/// What `tddy-sandbox-runner` is started with for a CLI harness.
#[allow(dead_code)]
pub(crate) struct CliRunnerArgv<'a> {
    pub sandbox_runner_path: &'a str,
    pub session_id: &'a str,
    pub context_dir: &'a Path,
    pub tool_ipc_socket: &'a Path,
    pub tddy_tools_path: &'a str,
    pub ready_marker: &'a Path,
    pub agent_kind: &'a str,
    pub agent_binary: &'a str,
    pub model: &'a str,
    pub egress_shim_port: u16,
    /// A resume: the runner continues the harness's own last session.
    pub resume: bool,
}

/// The argv `tddy-sandbox-runner` is started with to host a CLI harness in its PTY.
#[allow(dead_code)]
pub(crate) fn cli_harness_runner_argv(spec: &CliRunnerArgv<'_>) -> Vec<String> {
    let mut argv = vec![
        spec.sandbox_runner_path.to_string(),
        "--session-id".into(),
        spec.session_id.to_string(),
        "--context-dir".into(),
        spec.context_dir.to_string_lossy().into_owned(),
        "--tool-ipc-socket".into(),
        spec.tool_ipc_socket.to_string_lossy().into_owned(),
        "--tddy-tools-path".into(),
        spec.tddy_tools_path.to_string(),
        "--ready-marker".into(),
        spec.ready_marker.to_string_lossy().into_owned(),
        "--agent-kind".into(),
        spec.agent_kind.to_string(),
        "--agent-binary".into(),
        spec.agent_binary.to_string(),
        "--model".into(),
        spec.model.to_string(),
        "--egress-shim-port".into(),
        spec.egress_shim_port.to_string(),
        "--stdio".into(),
    ];
    if spec.resume {
        argv.push("--resume".into());
    }
    argv
}

/// `path` made absolute where it names a file, as the sandbox profile needs it. A bare name is left
/// to the `PATH` the jail resolves it on.
pub(crate) fn canonical_exec_path(path: &str) -> String {
    if path.contains('/') {
        std::fs::canonicalize(path)
            .map(|canonical| canonical.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}
