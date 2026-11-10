//! Acceptance: `tddy-sandbox-app --agent <spec id>` jails the agent its spec declares
//! (`#agent-tools 10/12`).
//!
//! Docs: `docs/ft/coder/sandbox-builder.md`. The app chooses the agent from
//! the coding-agent registry and hands the runner a rendered [`ExecPlan`] instead of agent flags.
//! These tests pin that plan — the boundary between the app and the jail — against what
//! `--agent-kind claude|cursor` launched before: the same binary, model, session and permission
//! flags, pass-through args ahead of the MCP flag, `tddy-tools` registered, and the egress shim on.
//! The binaries are stub scripts, `tddy-tools` among them, named through the env overrides their
//! specs declare. The overrides are held by the fixture's tool host, never set on this process, so
//! the tests touch no real install and need not run serially.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ProcessToolHost, ToolHost};
use tddy_sandbox_app::agent_spec::{
    agent_exec_plan, choose_agent_on, AgentChoiceError, AgentJail, ChosenAgent,
};
use tddy_sandbox_runner::ExecPlan;

const SESSION_ID: &str = "0199c0de-0000-7000-8000-000000000010";

/// A `<tddyhome>` with no overrides, a directory of stub agent binaries, and the binary overrides
/// an operator would export, held here rather than in this process's environment.
struct AHost {
    tddy_home: tempfile::TempDir,
    bin: tempfile::TempDir,
    env: BTreeMap<String, String>,
}

/// This process as a tool host, except that the env overrides are the fixture's own.
struct FixtureToolHost<'a> {
    env: &'a BTreeMap<String, String>,
}

impl ToolHost for FixtureToolHost<'_> {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        ProcessToolHost.path_dirs()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        ProcessToolHost.is_executable_file(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        ProcessToolHost.canonicalize(path)
    }
}

/// Every coding agent's closure includes `tddy-tools`, so the host gets a stub of it too, named
/// through its own override. A test then never depends on one being installed on `PATH`.
fn a_host() -> AHost {
    AHost {
        tddy_home: tempfile::tempdir().expect("a tddy home"),
        bin: tempfile::tempdir().expect("a bin dir"),
        env: BTreeMap::new(),
    }
    .with_stub_binary("tddy-tools", "TDDY_TOOLS_BINARY")
}

impl AHost {
    /// Write an executable stub `name` and point `env` at it, as an operator's override would.
    fn with_stub_binary(mut self, name: &str, env: &str) -> Self {
        let path = self.bin.path().join(name);
        std::fs::write(&path, "#!/bin/sh\necho \"$0 $*\" >> \"$0.argv\"\n").expect("a stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("an executable stub");
        self.env
            .insert(env.to_string(), path.to_string_lossy().into_owned());
        self
    }

    /// The stub's path as a resolved binary is spelled: canonical (macOS's tempdir is a symlink).
    fn stub(&self, name: &str) -> String {
        std::fs::canonicalize(self.bin.path().join(name))
            .expect("a stub on disk")
            .to_string_lossy()
            .into_owned()
    }

    fn choose(&self, id: &str) -> Result<ChosenAgent, AgentChoiceError> {
        choose_agent_on(
            id,
            self.tddy_home.path(),
            &FixtureToolHost { env: &self.env },
        )
    }
}

/// The jail `tddy-sandbox-app` lays out under a session dir, with `model` and the pass-through args
/// `-- --add-dir /extra`.
fn a_jail(model: &str) -> AgentJail {
    let root = PathBuf::from("/tmp/tddy-sandbox-app-agent-test/sandbox");
    AgentJail {
        session_id: SESSION_ID.to_string(),
        model: Some(model.to_string()),
        permission_mode: "auto".to_string(),
        agent_args: vec!["--add-dir".to_string(), "/extra".to_string()],
        home: PathBuf::from("/tmp/tddy-sandbox-app-agent-test/home"),
        tmp: root.join(".work/tmp"),
        context_dir: root.join("context"),
        cwd: PathBuf::from("/tmp/tddy-sandbox-app-agent-test/repo"),
        tddy_tools_path: PathBuf::from("/opt/tddy/bin/tddy-tools"),
        tool_ipc_socket: PathBuf::from("/tmp/tddy-ipc-0010.sock"),
    }
}

/// What a launch looks like from outside: the facts `--agent-kind` launches were relied on for.
#[derive(Debug, PartialEq, Eq)]
struct ObservedLaunch {
    binary: String,
    model: Option<String>,
    session_id: Option<String>,
    permission_mode: Option<String>,
    /// The pass-through args, in order, ahead of every MCP flag.
    pass_through_precedes_mcp: bool,
    /// The file that registers `tddy-tools --mcp`, by its jail path.
    mcp_registration: Option<PathBuf>,
    egress_shim: bool,
}

fn value_after(argv: &[String], flag: &str) -> Option<String> {
    argv.iter()
        .position(|arg| arg == flag)
        .and_then(|i| argv.get(i + 1))
        .cloned()
}

/// Observe `plan`; `mcp_flag` is the agent's MCP flag (`None` for a placed registration).
fn observe(plan: &ExecPlan, jail: &AgentJail, mcp_flag: Option<&str>) -> ObservedLaunch {
    let argv = &plan.argv;
    let pass_through = argv
        .windows(2)
        .position(|pair| pair[0] == "--add-dir" && pair[1] == "/extra");
    let first_mcp_flag = mcp_flag.and_then(|flag| argv.iter().position(|arg| arg == flag));
    let tddy_tools = jail.tddy_tools_path.to_string_lossy().into_owned();
    let mcp_registration = plan
        .files
        .iter()
        .find(|file| String::from_utf8_lossy(&file.contents).contains(&tddy_tools))
        .map(|file| file.path.clone());
    ObservedLaunch {
        binary: argv.first().cloned().unwrap_or_default(),
        model: value_after(argv, "--model"),
        session_id: value_after(argv, "--session-id"),
        permission_mode: value_after(argv, "--permission-mode"),
        pass_through_precedes_mcp: match (pass_through, first_mcp_flag) {
            (Some(args), Some(mcp)) => args < mcp,
            (Some(_), None) => mcp_flag.is_none(),
            (None, _) => false,
        },
        mcp_registration,
        egress_shim: plan.egress_shim,
    }
}

#[test]
fn sandbox_app_agent_claude_starts_a_jail_from_the_spec_with_todays_behaviour() {
    // Given
    let host = a_host().with_stub_binary("claude", "TDDY_CLAUDE_BINARY");
    let jail = a_jail("opus");
    let claude = host.choose("claude").expect("claude is in the registry");

    // When
    let plan = agent_exec_plan(&claude, &jail).expect("claude renders for the jail");

    // Then
    let observed = observe(&plan, &jail, Some("--mcp-config"));
    let mcp_config = value_after(&plan.argv, "--mcp-config")
        .map(PathBuf::from)
        .expect("claude is passed an --mcp-config");
    assert_eq!(
        observed,
        ObservedLaunch {
            binary: host.stub("claude"),
            model: Some("opus".to_string()),
            session_id: Some(SESSION_ID.to_string()),
            permission_mode: Some("auto".to_string()),
            pass_through_precedes_mcp: true,
            mcp_registration: Some(mcp_config),
            egress_shim: true,
        }
    );
}

#[test]
fn sandbox_app_agent_cursor_does_the_same() {
    // Given
    let host = a_host().with_stub_binary("agent", "TDDY_CURSOR_AGENT");
    let jail = a_jail("composer-2.5");
    let cursor = host.choose("cursor").expect("cursor is in the registry");

    // When
    let plan = agent_exec_plan(&cursor, &jail).expect("cursor renders for the jail");

    // Then — Cursor reads its MCP servers from `$HOME/.cursor/mcp.json`, placed in the jail's home
    assert_eq!(
        observe(&plan, &jail, None),
        ObservedLaunch {
            binary: host.stub("agent"),
            model: Some("composer-2.5".to_string()),
            session_id: None,
            permission_mode: None,
            pass_through_precedes_mcp: true,
            mcp_registration: Some(Path::new(&jail.home).join(".cursor/mcp.json")),
            egress_shim: true,
        }
    );
}

#[test]
fn sandbox_app_agent_opencode_is_refused_without_a_route_as_today() {
    // Given
    let host = a_host();

    // When
    let refusals: Vec<String> = ["opencode", "muse"]
        .into_iter()
        .map(|id| {
            host.choose(id)
                .expect_err("a route-only agent is refused")
                .to_string()
        })
        .collect();

    // Then
    assert_eq!(
        refusals,
        vec![
            "--agent opencode is not started by tddy-sandbox-app yet: a jailed opencode session \
             runs on a provider account, and this app passes none"
                .to_string(),
            "--agent muse is not started by tddy-sandbox-app yet: a jailed muse session runs on a \
             provider account, and this app passes none"
                .to_string(),
        ]
    );
}

/// The agents a PTY sandbox can run: every coding agent whose spec's `run` is a PTY launch. The
/// ACP-only agents (`claude-acp`, `codex-acp`) are in the registry but not among them.
fn the_pty_runnable_agents() -> Vec<String> {
    [
        "claude", "codex", "cursor", "muse", "opencode", "oz", "warp",
    ]
    .map(str::to_string)
    .to_vec()
}

#[test]
fn the_pty_sandbox_offers_only_agents_with_a_pty_run() {
    // Given
    let host = a_host();

    // When an id no coding agent has is chosen
    let refused = host.choose("gemini");

    // Then the refusal offers the agents this app can run, and no ACP-only one
    assert_eq!(
        refused,
        Err(AgentChoiceError::Unknown {
            id: "gemini".to_string(),
            known: the_pty_runnable_agents(),
        })
    );
}

#[test]
fn choosing_an_acp_only_agent_in_the_pty_sandbox_is_refused_naming_the_runnable_ones() {
    // Given
    let host = a_host();

    // When each ACP-only agent is chosen
    let refusals: Vec<Result<ChosenAgent, AgentChoiceError>> = ["claude-acp", "codex-acp"]
        .map(|id| host.choose(id))
        .to_vec();

    // Then each is refused for having no PTY run, naming the agents this app can run
    assert_eq!(
        refusals,
        vec![
            Err(AgentChoiceError::NotPtyRunnable {
                id: "claude-acp".to_string(),
                runnable: the_pty_runnable_agents(),
            }),
            Err(AgentChoiceError::NotPtyRunnable {
                id: "codex-acp".to_string(),
                runnable: the_pty_runnable_agents(),
            }),
        ]
    );
}

#[test]
fn the_acp_refusal_reads_as_the_agent_it_cannot_run_and_the_ones_it_can() {
    // Given
    let refusal = AgentChoiceError::NotPtyRunnable {
        id: "claude-acp".to_string(),
        runnable: vec!["claude".to_string(), "codex".to_string()],
    };

    // When
    let message = refusal.to_string();

    // Then
    assert_eq!(
        message,
        "--agent claude-acp has no PTY run, so tddy-sandbox-app cannot start it; it runs claude, \
         codex"
    );
}
