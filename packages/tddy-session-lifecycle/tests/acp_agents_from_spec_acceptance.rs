//! Acceptance: `claude-acp`, `codex-acp` and `opencode` coding subagents are launched from their tool
//! definitions (`claude-acp.yaml`, `codex-acp.yaml` and `opencode.yaml`'s `coding_agent.acp`): the
//! command, the `session/new` `mcp_servers` and the managed-session withdrawal are what the
//! hand-built ACP arms of `DaemonCodingAgentLauncher` give today; `codex-acp` resolves beside its
//! `codex` dep, so an override that moves `codex` moves it too; and a managed session still refuses
//! `codex-acp`, whose spec declares `tool_deny: unsupported` (`#agent-tools 11/12`).
//!
//! PRD: `docs/ft/coder/changelog/2026-11-04-acp-agents-from-specs.md`.
//!
//! Every agent binary is a shell wrapper on a fixture `ToolHost` that records its argv and the
//! inline OpenCode config it was handed, then `exec`s `tddy-acp-stub`, which records each
//! `session/new` it receives (`TDDY_ACP_STUB_RECORD`). Needs `target/debug/tddy-acp-stub`
//! (`cargo build -p tddy-acp-stub`).

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::{load_agent_registry, CODEX_ACP_SPEC_YAML, CODEX_SPEC_YAML};
use tddy_coding_subagent::{
    AgentTransport, CodingAgentDef, CodingAgentLaunch, CodingAgentLauncher, CodingAgentSession,
    LaunchedAgent,
};
use tddy_core::provider_route::HarnessRoute;
use tddy_core::OpenSession;
use tddy_session_lifecycle::coding_agent_as_user::DropContext;
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;

const SESSION_ID: &str = "1780828020298-coding";
/// The model `tddy-acp-stub` advertises.
const STUB_MODEL: &str = "stub-model";
/// Where the launch's tool server resolves `tddy-tools` to.
const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";
const MINTED_TOKEN: &str = "minted-agent-token";

/// A host with nothing of the real machine: the env it is given, and executable files.
struct AFixtureHost {
    env: BTreeMap<String, String>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// `tddy-acp-stub` in the target directory cargo builds into: `CARGO_TARGET_DIR` when set (a
/// relative one read from the workspace root), else the workspace's `target/`.
fn the_acp_stub() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|dir| workspace.join(dir))
        .unwrap_or_else(|| workspace.join("target"));
    let stub = target.join("debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// A directory of agent binaries: each ACP agent a wrapper that records its argv (one entry per
/// line) to `<name>.argv` and its `OPENCODE_CONFIG_CONTENT` to `<name>.config-content`, then runs
/// `tddy-acp-stub` recording into `<name>.record.jsonl`; `codex` and `tddy-tools` do nothing.
struct TheAgentBinaries {
    dir: tempfile::TempDir,
}

fn the_agent_binaries() -> TheAgentBinaries {
    let binaries = TheAgentBinaries {
        dir: tempfile::tempdir().expect("binaries dir"),
    };
    for sub in ["bin", "codex-elsewhere"] {
        std::fs::create_dir(binaries.dir.path().join(sub)).expect("binaries sub dir");
    }
    for agent in [
        "bin/claude-agent-acp",
        "bin/codex-acp",
        "bin/opencode",
        "codex-elsewhere/codex-acp",
    ] {
        binaries.write(agent, &binaries.wrapper(agent));
    }
    for quiet in ["bin/codex", "codex-elsewhere/codex", "bin/tddy-tools"] {
        binaries.write(quiet, "#!/bin/sh\n");
    }
    binaries
}

impl TheAgentBinaries {
    fn wrapper(&self, agent: &str) -> String {
        let record = |suffix: &str| {
            self.dir
                .path()
                .join(format!("{}.{suffix}", agent.replace('/', "-")))
                .display()
                .to_string()
        };
        format!(
            r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
printf '%s' "$OPENCODE_CONFIG_CONTENT" > '{config}'
TDDY_ACP_STUB_RECORD='{record}' exec '{stub}'
"#,
            argv = record("argv"),
            config = record("config-content"),
            record = record("record.jsonl"),
            stub = the_acp_stub().display(),
        )
    }

    fn write(&self, name: &str, script: &str) {
        let path = self.dir.path().join(name);
        std::fs::write(&path, script).expect("write binary");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn path(&self, name: &str) -> PathBuf {
        std::fs::canonicalize(self.dir.path().join(name)).expect("binary path")
    }

    fn env_value(&self, name: &str) -> String {
        self.path(name).to_string_lossy().into_owned()
    }

    fn recorded(&self, agent: &str, suffix: &str) -> String {
        let file = self
            .dir
            .path()
            .join(format!("{}.{suffix}", agent.replace('/', "-")));
        std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("the {agent} wrapper ran: {} {e}", file.display()))
    }

    fn argv(&self, agent: &str) -> Vec<String> {
        self.recorded(agent, "argv")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn inline_config(&self, agent: &str) -> String {
        self.recorded(agent, "config-content")
    }

    /// The one `session/new` the stub behind `agent` received: its `mcp_servers` names and `_meta`.
    fn session_new(&self, agent: &str) -> (Vec<String>, serde_json::Value) {
        let opened: Vec<serde_json::Value> = self
            .recorded(agent, "record.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a record line is JSON"))
            .filter(|line: &serde_json::Value| line["event"] == "new_session")
            .collect();
        assert_eq!(opened.len(), 1, "{agent} received one session/new");
        (
            serde_json::from_value(opened[0]["mcp_servers"].clone()).expect("server names"),
            opened[0]["meta"].clone(),
        )
    }
}

fn this_user() -> String {
    tddy_session_activity::user_sessions_path::username_for_uid(unsafe { libc::geteuid() })
        .expect("the test user has a passwd entry")
}

fn the_daemons_own_identity() -> DropContext {
    let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
    DropContext {
        uid,
        gid,
        privileged: false,
        setpriv: None,
    }
}

/// A launcher holding the coding agents of `<tddy_data_dir>/tools`, resolved against a host whose
/// environment is `env`.
fn a_launcher_with_coding_agents(
    tools_dir: &Path,
    env: BTreeMap<String, String>,
) -> (DaemonCodingAgentLauncher, Arc<HostCodingAgents>) {
    a_launcher_routed_to(tools_dir, env, None)
}

/// [`a_launcher_with_coding_agents`] whose every provider account resolves to `route`.
fn a_launcher_routed_to(
    tools_dir: &Path,
    env: BTreeMap<String, String>,
    route: Option<HarnessRoute>,
) -> (DaemonCodingAgentLauncher, Arc<HostCodingAgents>) {
    let registry = load_agent_registry(tools_dir).expect("the agent registry loads");
    let agents = Arc::new(HostCodingAgents::new(
        registry,
        Arc::new(AFixtureHost { env }),
    ));
    let launcher = DaemonCodingAgentLauncher::with_identity(
        the_daemons_own_identity(),
        Arc::new(move |_| Ok(route.clone())),
        Arc::new(|_| {
            Ok((
                PathBuf::from(TDDY_TOOLS_PATH),
                vec![(
                    "TDDY_REMOTE_SESSION_TOKEN".to_string(),
                    MINTED_TOKEN.to_string(),
                )],
            ))
        }),
    )
    .with_coding_agents(Arc::clone(&agents));
    (launcher, agents)
}

/// The host environment naming the wrappers: `TDDY_CLAUDE_AGENT_ACP_CLI` and `TDDY_OPENCODE_CLI`
/// outright, `codex-acp` only through its `codex` dep (`TDDY_CODEX_CLI`).
fn an_env_naming(binaries: &TheAgentBinaries) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "TDDY_CLAUDE_AGENT_ACP_CLI".to_string(),
            binaries.env_value("bin/claude-agent-acp"),
        ),
        (
            "TDDY_CODEX_CLI".to_string(),
            binaries.env_value("bin/codex"),
        ),
        (
            "TDDY_OPENCODE_CLI".to_string(),
            binaries.env_value("bin/opencode"),
        ),
        (
            "TDDY_TOOLS_BINARY".to_string(),
            binaries.env_value("bin/tddy-tools"),
        ),
    ])
}

fn a_launch(harness: &str, managed: bool, worktree: &Path) -> CodingAgentLaunch {
    CodingAgentLaunch {
        def: CodingAgentDef {
            name: format!("fix-the-parser-{harness}"),
            label: None,
            harness: harness.to_string(),
            model: STUB_MODEL.to_string(),
            provider_account: None,
        },
        session_id: SESSION_ID.to_string(),
        session_os_user: this_user(),
        worktree: worktree.to_path_buf(),
        managed,
        session_token: "caller-session-token".to_string(),
        conversation_id: Some(format!("conv-{harness}")),
    }
}

/// Open `launch`'s ACP session as `tddy-session-agents` does: the launcher's MCP servers and
/// `_meta`, then its command.
async fn open(
    launcher: &DaemonCodingAgentLauncher,
    launch: &CodingAgentLaunch,
) -> CodingAgentSession {
    let harness = &launch.def.harness;
    let mcp_servers = launcher
        .mcp_servers_for(launch)
        .await
        .unwrap_or_else(|e| panic!("{harness}'s MCP servers are planned: {e}"));
    let meta = launcher
        .session_meta_for(launch)
        .await
        .unwrap_or_else(|e| panic!("{harness}'s session meta is planned: {e}"));
    let command = launcher
        .command_for(launch)
        .await
        .unwrap_or_else(|e| panic!("{harness}'s command is planned: {e}"));
    let open = OpenSession {
        cwd: launch.worktree.clone(),
        resume: None,
        model: Some(STUB_MODEL.to_string()),
        mcp_servers,
        meta,
    };
    CodingAgentSession::start(
        command,
        open,
        STUB_MODEL.to_string(),
        tddy_coding_subagent::no_launch_hooks(),
    )
    .await
    .unwrap_or_else(|e| panic!("{harness}'s session opens: {e}"))
}

#[tokio::test]
async fn claude_acp_codex_acp_and_opencode_coding_subagents_launch_from_specs_against_tddy_acp_stub_with_todays_command_env_mcp_and_withdrawal(
) {
    // Given — claude-acp and opencode in a managed session; codex-acp, which cannot be confined, in
    // an unmanaged one
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let binaries = the_agent_binaries();
    let (launcher, _) =
        a_launcher_with_coding_agents(&data_dir.path().join("tools"), an_env_naming(&binaries));

    // When
    let _claude = open(&launcher, &a_launch("claude-acp", true, worktree.path())).await;
    let _codex = open(&launcher, &a_launch("codex-acp", false, worktree.path())).await;
    let _opencode = open(&launcher, &a_launch("opencode", true, worktree.path())).await;

    // Then — the binary `TDDY_CLAUDE_AGENT_ACP_CLI` names alone, `codex-acp` beside `codex`, and
    // `opencode acp`; the tool server as `mcp_servers` on a managed launch; Claude's tools withdrawn
    // through `_meta`, OpenCode's through inline config
    let inline_config: serde_json::Value =
        serde_json::from_str(&binaries.inline_config("bin/opencode")).expect("inline JSON");
    assert_eq!(
        (
            binaries.argv("bin/claude-agent-acp"),
            binaries.session_new("bin/claude-agent-acp"),
            binaries.argv("bin/codex-acp"),
            binaries.session_new("bin/codex-acp"),
            binaries.inline_config("bin/codex-acp"),
            binaries.argv("bin/opencode"),
            binaries.session_new("bin/opencode"),
            inline_config,
        ),
        (
            Vec::<String>::new(),
            (
                vec!["tddy-tools".to_string()],
                serde_json::json!({ "claudeCode": { "options": { "disallowedTools": [
                    "Read", "Write", "Edit", "MultiEdit", "NotebookEdit", "Bash", "BashOutput",
                    "KillShell", "Glob", "Grep", "LS",
                ] } } }),
            ),
            Vec::<String>::new(),
            (Vec::<String>::new(), serde_json::Value::Null),
            String::new(),
            vec!["acp".to_string()],
            (vec!["tddy-tools".to_string()], serde_json::Value::Null),
            serde_json::json!({ "tools": {
                "bash": false, "read": false, "write": false, "edit": false,
                "patch": false, "glob": false, "grep": false, "list": false,
            } }),
        )
    );
}

#[tokio::test]
async fn codex_acp_resolves_through_its_codex_dep_and_a_tddyhome_codex_override_moves_both() {
    // Given — `<tddyhome>/tools/codex.yaml` names `codex` by another variable, which points at a
    // directory holding its own `codex-acp`; `TDDY_CODEX_CLI` still names the first one
    let data_dir = tempfile::tempdir().expect("data dir");
    let tools_dir = data_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).expect("tools dir");
    std::fs::write(
        tools_dir.join("codex.yaml"),
        CODEX_SPEC_YAML.replace("env: TDDY_CODEX_CLI", "env: TDDY_OTHER_CODEX"),
    )
    .expect("write the override");
    let worktree = tempfile::tempdir().expect("worktree");
    let binaries = the_agent_binaries();
    let mut env = an_env_naming(&binaries);
    env.insert(
        "TDDY_OTHER_CODEX".to_string(),
        binaries.env_value("codex-elsewhere/codex"),
    );
    let (launcher, agents) = a_launcher_with_coding_agents(&tools_dir, env);

    // When
    let command = launcher
        .command_for(&a_launch("codex-acp", false, worktree.path()))
        .await
        .expect("codex-acp is planned");
    let resolved = agents
        .resolve("codex-acp", None)
        .expect("codex-acp resolves");

    // Then — `codex-acp` beside the overridden `codex`, which is also the dep its login runs
    assert_eq!(
        (
            command.program,
            resolved.tool.binary.clone(),
            resolved.dep("codex").map(|codex| codex.binary.clone()),
        ),
        (
            binaries.path("codex-elsewhere/codex-acp"),
            binaries.path("codex-elsewhere/codex-acp"),
            Some(binaries.path("codex-elsewhere/codex")),
        )
    );
}

#[tokio::test]
async fn a_managed_session_still_refuses_codex_acp_because_its_spec_declares_tool_deny_unsupported()
{
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let binaries = the_agent_binaries();
    let (launcher, _) =
        a_launcher_with_coding_agents(&data_dir.path().join("tools"), an_env_naming(&binaries));

    // When — codex-acp, and claude-acp as the control, both in a managed session
    let refused = launcher
        .command_for(&a_launch("codex-acp", true, worktree.path()))
        .await
        .map(|command| command.program)
        .map_err(|e| e.to_string());
    let admitted = launcher
        .command_for(&a_launch("claude-acp", true, worktree.path()))
        .await
        .map(|command| command.program)
        .map_err(|e| e.to_string());

    // Then
    assert_eq!(
        (refused, admitted),
        (
            Err(
                "coding agent 'fix-the-parser-codex-acp' (codex-acp) cannot be launched: the \
                 'codex-acp' agent cannot withdraw its native file-system and shell tools (its \
                 spec declares tool_deny: unsupported), and a managed session's main agent has \
                 none, so its subagent must not either: attach a coding agent that can be \
                 confined (claude-acp, opencode) or a model agent"
                    .to_string()
            ),
            Ok(binaries.path("bin/claude-agent-acp")),
        )
    );
}

// ── The launch prepares every declared hooks adapter; route args follow the spec (`#agent-tools 27/28`)
//
// `launch_agent` hands back the agent with the hooks of the adapter its spec declares
// (`coding_agent.acp.hooks_adapter`): `codex-acp-login` runs `codex login` with the `codex` dep and
// retries once on an auth-like `session/new` error. Which agent takes a route as `-c` overrides is
// its spec's `route.shape: toml_args`, never its name.
//
// Changeset: `docs/dev/1-WIP/2026-11-06-acp-subagent-leftovers.md`.

/// How long a `codex login` the hooks run may take.
const LOGIN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// `a_launch(harness, managed, worktree)` on a provider account.
fn a_routed_launch(harness: &str, managed: bool, worktree: &Path) -> CodingAgentLaunch {
    let mut launch = a_launch(harness, managed, worktree);
    launch.def.provider_account = Some("acct-route".to_string());
    launch
}

/// A provider route naming its model the way `codex` takes it (`-m <model>`).
fn a_route_naming_a_model() -> HarnessRoute {
    HarnessRoute {
        args: vec!["-m".to_string(), "gpt-5.2".to_string()],
        ..HarnessRoute::default()
    }
}

/// The agent `launcher` launches for `launch`, which must be one this daemon spawns itself.
async fn the_launched_agent(
    launcher: &DaemonCodingAgentLauncher,
    launch: &CodingAgentLaunch,
) -> (
    tddy_core::AcpAgentCommand,
    tddy_core::adapters::PreparedAcpHooks,
) {
    let LaunchedAgent { transport, hooks } = launcher
        .launch_agent(launch)
        .await
        .unwrap_or_else(|e| panic!("{} is launched: {e}", launch.def.harness));
    match transport {
        AgentTransport::Command(command) => (command, hooks),
        AgentTransport::Stream(_) => panic!("the daemon's own user needs no relay"),
    }
}

impl TheAgentBinaries {
    /// Make `bin/codex` record each argv it is run with to `codex.argv` and succeed.
    fn with_a_codex_that_records_its_runs(self) -> Self {
        let record = self.dir.path().join("codex.argv");
        self.write(
            "bin/codex",
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n",
                record.display()
            ),
        );
        self
    }

    /// Every run of `bin/codex`, its arguments joined by spaces.
    fn codex_runs(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("codex.argv"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

#[tokio::test]
async fn a_codex_acp_launch_runs_codex_login_and_retries_once_on_an_auth_like_session_error() {
    // Given — codex-acp declares `acp.hooks_adapter: codex-acp-login`
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let login_dir = tempfile::tempdir().expect("login dir");
    let binaries = the_agent_binaries().with_a_codex_that_records_its_runs();
    let (launcher, _) =
        a_launcher_with_coding_agents(&data_dir.path().join("tools"), an_env_naming(&binaries));
    let (_command, hooks) =
        the_launched_agent(&launcher, &a_launch("codex-acp", false, worktree.path())).await;

    // When — an open its hooks bracket fails as an agent that is not signed in does
    let opens = hooks
        .opens
        .clone()
        .expect("codex-acp-login brackets each open");
    opens.begin_open(Some(login_dir.path().to_path_buf())).await;
    let verdict = tokio::time::timeout(
        LOGIN_DEADLINE,
        hooks.hooks.on_session_error("authentication required"),
    )
    .await
    .expect("codex login finished in time");
    let login_failure = opens.end_open().await.map(|e| e.to_string());

    // Then — `codex login` ran with the codex dep, and the open is retried once
    assert_eq!(
        (
            matches!(verdict, tddy_core::SessionErrorVerdict::RetryOnce),
            binaries.codex_runs(),
            login_failure,
        ),
        (true, vec!["login".to_string()], None)
    );
}

#[tokio::test]
async fn a_route_reaches_an_acp_agent_as_overrides_only_when_its_spec_declares_toml_args() {
    // Given — `<tddyhome>/tools/codex-acp.yaml` declares its route as environment, not `-c` args
    let data_dir = tempfile::tempdir().expect("data dir");
    let tools_dir = data_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).expect("tools dir");
    std::fs::write(
        tools_dir.join("codex-acp.yaml"),
        CODEX_ACP_SPEC_YAML.replace("shape: toml_args", "shape: env"),
    )
    .expect("write the override");
    let worktree = tempfile::tempdir().expect("worktree");
    let binaries = the_agent_binaries();
    let (launcher, _) = a_launcher_routed_to(
        &tools_dir,
        an_env_naming(&binaries),
        Some(a_route_naming_a_model()),
    );

    // When
    let (command, _hooks) = the_launched_agent(
        &launcher,
        &a_routed_launch("codex-acp", false, worktree.path()),
    )
    .await;

    // Then — no `-c model=…`: the agent's name does not make its route `-c` overrides
    assert_eq!(command.args, Vec::<String>::new());
}

#[tokio::test]
async fn a_codex_acp_route_still_reaches_it_as_one_model_override_under_its_built_in_spec() {
    // Given — the built-in `codex-acp.yaml`, `route.shape: toml_args`
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let binaries = the_agent_binaries();
    let (launcher, _) = a_launcher_routed_to(
        &data_dir.path().join("tools"),
        an_env_naming(&binaries),
        Some(a_route_naming_a_model()),
    );

    // When
    let (command, _hooks) = the_launched_agent(
        &launcher,
        &a_routed_launch("codex-acp", false, worktree.path()),
    )
    .await;

    // Then — exactly once, whether the launcher or the declared adapter adds it
    assert_eq!(
        command.args,
        vec!["-c".to_string(), "model=\"gpt-5.2\"".to_string()]
    );
}

#[tokio::test]
async fn an_agent_whose_spec_declares_no_hooks_adapter_is_launched_with_the_command_it_plans() {
    // Given — claude-acp declares no `acp.hooks_adapter`
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let binaries = the_agent_binaries();
    let (launcher, _) =
        a_launcher_with_coding_agents(&data_dir.path().join("tools"), an_env_naming(&binaries));
    let launch = a_launch("claude-acp", true, worktree.path());

    // When
    let (launched, hooks) = the_launched_agent(&launcher, &launch).await;
    let planned = launcher.command_for(&launch).await.expect("planned");

    // Then — the planned command, and hooks that bracket no open
    assert_eq!(
        (
            launched.program,
            launched.args,
            launched.env,
            launched.cwd,
            hooks.opens.is_none()
        ),
        (
            planned.program,
            planned.args,
            planned.env,
            planned.cwd,
            true
        )
    );
}

#[tokio::test]
async fn a_routed_opencode_launch_keeps_its_route_inline_and_names_no_route_file() {
    // Given — opencode declares `acp.hooks_adapter: opencode-route-file`, and its route carries a file
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let binaries = the_agent_binaries();
    let route = HarnessRoute {
        files: vec![tddy_core::provider_route::RouteFile {
            name: "opencode.json".to_string(),
            contents: r#"{"provider":{"acct":{}}}"#.to_string(),
        }],
        ..HarnessRoute::default()
    };
    let (launcher, _) = a_launcher_routed_to(
        &data_dir.path().join("tools"),
        an_env_naming(&binaries),
        Some(route),
    );

    // When
    let (command, _hooks) = the_launched_agent(
        &launcher,
        &a_routed_launch("opencode", false, worktree.path()),
    )
    .await;

    // Then — a daemon-written route file would be unreadable by the session's OS user: the route
    // stays inline, and no variable names a file
    let names: Vec<&str> = command.env.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        (
            names.contains(&"OPENCODE_CONFIG"),
            names.contains(&"OPENCODE_CONFIG_CONTENT")
        ),
        (false, true)
    );
}
