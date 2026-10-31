//! Acceptance: `claude-acp`, `codex-acp` and `opencode` coding subagents are launched from their tool
//! definitions (`claude-acp.yaml`, `codex-acp.yaml` and `opencode.yaml`'s `coding_agent.acp`): the
//! command, the `session/new` `mcp_servers` and the managed-session withdrawal are what the
//! hand-built ACP arms of `DaemonCodingAgentLauncher` give today; `codex-acp` resolves beside its
//! `codex` dep, so an override that moves `codex` moves it too; and a managed session still refuses
//! `codex-acp`, whose spec declares `tool_deny: unsupported` (`#agent-tools 11/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md`.
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
use tddy_coding_agent_specs::{load_agent_registry, CODEX_SPEC_YAML};
use tddy_coding_subagent::{
    CodingAgentDef, CodingAgentLaunch, CodingAgentLauncher, CodingAgentSession,
};
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
    let registry = load_agent_registry(tools_dir).expect("the agent registry loads");
    let agents = Arc::new(HostCodingAgents::new(
        registry,
        Arc::new(AFixtureHost { env }),
    ));
    let launcher = DaemonCodingAgentLauncher::with_identity(
        the_daemons_own_identity(),
        Arc::new(|_| Ok(None)),
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
    CodingAgentSession::start(command, open, STUB_MODEL.to_string())
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
