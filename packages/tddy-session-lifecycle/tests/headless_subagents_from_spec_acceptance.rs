//! Acceptance: headless `cursor`, `codex` and `oz` coding subagents are launched from their tool
//! definitions (`cursor.yaml`'s, `codex.yaml`'s and `oz.yaml`'s `coding_agent.headless`), each
//! registering `tddy-tools` the way its spec's MCP slot declares — a worktree file, inline `-c`
//! overrides, an `--mcp` file — and the worktree file a Cursor conversation writes goes with the
//! conversation: on its close, and on the start-up sweep of a daemon that lost it
//! (`#agent-tools 9/12`).
//!
//! PRD: `docs/ft/coder/changelog/2026-11-03-headless-cursor-codex-oz-from-specs.md`.
//!
//! The daemon config names CLIs that do not exist, so only the binaries the specs resolve — stubs
//! on a fixture `ToolHost` — can run. Each stub records its argv and the registration it was
//! handed, then prints one recorded turn in its CLI's output format.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::load_agent_registry;
use tddy_coding_subagent::launcher::close_headless_conversation;
use tddy_coding_subagent::{CodingAgentDef, CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::{ConversationDriver, HeadlessDriver};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::coding_agent_as_user::DropContext;
use tddy_session_lifecycle::coding_agent_launcher::{
    sweep_headless_files_left_by_this_daemon, DaemonCodingAgentLauncher,
};

const TURN_DEADLINE: Duration = Duration::from_secs(10);
const SESSION_ID: &str = "1780828020298-coding";
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

/// Stub `agent`, `codex` and `oz` CLIs and a stub `tddy-tools`, in one directory. Each CLI records
/// its argv (one entry per line) to `<name>.argv`; `agent` copies the worktree's
/// `.cursor/mcp.json` it runs with to `agent.mcp`, `oz` the `--mcp` file to `oz.mcp`, and `codex`
/// its environment to `codex.env`.
struct TheStubClis {
    dir: tempfile::TempDir,
}

fn the_stub_clis() -> TheStubClis {
    let dir = tempfile::tempdir().expect("stub dir");
    let record = |name: &str| dir.path().join(name).display().to_string();
    let scripts = [
        (
            "agent",
            format!(
                r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
/bin/cat .cursor/mcp.json > '{mcp}' 2>/dev/null
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"chat-1"}}'
printf '%s\n' '{{"type":"result","subtype":"success","result":"Fixed it.","session_id":"chat-1"}}'
"#,
                argv = record("agent.argv"),
                mcp = record("agent.mcp"),
            ),
        ),
        (
            "codex",
            format!(
                r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
/usr/bin/env > '{env}'
printf '%s\n' '{{"type":"thread.started","thread_id":"th-1"}}'
printf '%s\n' '{{"type":"item.completed","item":{{"id":"item_1","type":"agent_message","text":"Fixed it."}}}}'
printf '%s\n' '{{"type":"turn.completed"}}'
"#,
                argv = record("codex.argv"),
                env = record("codex.env"),
            ),
        ),
        (
            "oz",
            format!(
                r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
mcp=""; prev=""
for a in "$@"; do [ "$prev" = "--mcp" ] && mcp="$a"; prev="$a"; done
/bin/cat "$mcp" > '{mcp}' 2>/dev/null
printf '%s\n' '{{"type":"system","event_type":"conversation_started","conversation_id":"oz-1"}}'
printf '%s\n' '{{"type":"agent","text":"Fixed it."}}'
"#,
                argv = record("oz.argv"),
                mcp = record("oz.mcp"),
            ),
        ),
        ("tddy-tools", "#!/bin/sh\n".to_string()),
    ];
    for (name, script) in scripts {
        let path = dir.path().join(name);
        std::fs::write(&path, script).expect("write stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    TheStubClis { dir }
}

impl TheStubClis {
    fn binary(&self, name: &str) -> String {
        std::fs::canonicalize(self.dir.path().join(name))
            .expect("stub path")
            .to_string_lossy()
            .into_owned()
    }

    fn argv(&self, cli: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join(format!("{cli}.argv")))
            .unwrap_or_else(|e| panic!("the {cli} stub ran: {e}"))
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The variables of `names` that `cli` ran with, by name.
    fn env_seen(&self, cli: &str, names: &[&str]) -> BTreeMap<String, String> {
        std::fs::read_to_string(self.dir.path().join(format!("{cli}.env")))
            .unwrap_or_else(|e| panic!("the {cli} stub recorded its environment: {e}"))
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(name, _)| names.contains(name))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    fn mcp_seen(&self, cli: &str) -> serde_json::Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join(format!("{cli}.mcp")))
                .unwrap_or_else(|e| panic!("the {cli} stub saw a registration: {e}")),
        )
        .expect("the registration is JSON")
    }
}

/// A daemon config that names no CLIs: each agent comes from its tool definition alone.
fn a_daemon_config() -> DaemonConfig {
    serde_yaml::from_str("users:\n  - github_user: u\n    os_user: u\n")
        .expect("daemon config should parse")
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

/// A git checkout to run a conversation's CLI in, as a clone-placed conversation's worktree is.
fn a_git_worktree() -> tempfile::TempDir {
    let worktree = tempfile::tempdir().expect("worktree");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree.path())
        .args(["init", "-q", "-b", "main"])
        .status()
        .expect("git init");
    assert!(status.success(), "git init");
    worktree
}

fn a_headless_launch(harness: &str, conversation: &str, worktree: &Path) -> CodingAgentLaunch {
    CodingAgentLaunch {
        def: CodingAgentDef {
            name: format!("fix-the-parser-{harness}"),
            label: None,
            harness: harness.to_string(),
            model: "a-model".to_string(),
            provider_account: None,
        },
        session_id: SESSION_ID.to_string(),
        session_os_user: this_user(),
        worktree: worktree.to_path_buf(),
        managed: false,
        session_token: "caller-session-token".to_string(),
        conversation_id: Some(conversation.to_string()),
    }
}

/// A launcher wired for headless conversations under `<tddy_data_dir>/headless`, with the coding
/// agents of an empty `<tddyhome>/tools` resolved against a host that finds the stubs.
fn a_launcher_with_coding_agents(
    tddy_data_dir: &Path,
    stubs: &TheStubClis,
) -> DaemonCodingAgentLauncher {
    let registry =
        load_agent_registry(&tddy_data_dir.join("tools")).expect("the agent registry loads");
    let host = AFixtureHost {
        env: BTreeMap::from([
            ("TDDY_CURSOR_AGENT".to_string(), stubs.binary("agent")),
            ("TDDY_CODEX_CLI".to_string(), stubs.binary("codex")),
            ("TDDY_OZ_CLI".to_string(), stubs.binary("oz")),
            ("TDDY_TOOLS_BINARY".to_string(), stubs.binary("tddy-tools")),
        ]),
    };
    DaemonCodingAgentLauncher::with_identity(
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
    .with_headless_wiring(a_daemon_config(), tddy_data_dir.join("headless"))
    .with_coding_agents(Arc::new(HostCodingAgents::new(registry, Arc::new(host))))
}

/// One turn of conversation `conversation` of `harness`, planned by `launcher`.
async fn one_turn(
    launcher: &DaemonCodingAgentLauncher,
    harness: &str,
    conversation: &str,
    worktree: &Path,
) -> String {
    let command = launcher
        .headless_command_for(&a_headless_launch(harness, conversation, worktree))
        .await
        .unwrap_or_else(|e| panic!("the {harness} conversation is planned: {e}"));
    let driver = HeadlessDriver::new(command);
    tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt_turn("Fix it".to_string(), None),
    )
    .await
    .expect("the turn ended in time")
    .unwrap_or_else(|e| panic!("the {harness} turn succeeds: {e}"))
    .text
}

/// `tddy-tools --mcp` as the launch's tool server, its credential and conversation in its
/// environment.
fn the_tool_server_entry(conversation: &str) -> serde_json::Value {
    serde_json::json!({
        "command": TDDY_TOOLS_PATH,
        "args": ["--mcp"],
        "env": {
            "TDDY_REMOTE_SESSION_TOKEN": MINTED_TOKEN,
            "TDDY_CONVERSATION_ID": conversation,
        },
    })
}

/// The `-c` overrides in `argv`, in order.
fn config_overrides(argv: &[String]) -> Vec<String> {
    argv.windows(2)
        .filter(|pair| pair[0] == "-c")
        .map(|pair| pair[1].clone())
        .collect()
}

/// The value after `flag` in `argv`.
fn value_of(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

#[tokio::test]
async fn headless_cursor_codex_and_oz_subagents_launch_with_mcp_rendered_as_file_inline_and_argv() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
    let stubs = the_stub_clis();
    let launcher = a_launcher_with_coding_agents(data_dir.path(), &stubs);

    // When
    let replies = (
        one_turn(&launcher, "cursor", "conv-cursor", worktree.path()).await,
        one_turn(&launcher, "codex", "conv-codex", worktree.path()).await,
        one_turn(&launcher, "oz", "conv-oz", worktree.path()).await,
    );

    // Then — Cursor: a worktree `.cursor/mcp.json`; Codex: inline `-c`; Oz: an `--mcp` file
    let codex_argv = stubs.argv("codex");
    let codex_conversation_dir = data_dir.path().join("headless").join("conv-codex");
    let oz_mcp_file = data_dir
        .path()
        .join("headless")
        .join("conv-oz")
        .join("mcp.json");
    assert_eq!(
        (
            replies,
            stubs.mcp_seen("agent"),
            config_overrides(&codex_argv),
            codex_argv.iter().any(|arg| arg.contains(MINTED_TOKEN)),
            value_of(&stubs.argv("oz"), "--mcp"),
            stubs.mcp_seen("oz"),
        ),
        (
            (
                "Fixed it.".to_string(),
                "Fixed it.".to_string(),
                "Fixed it.".to_string()
            ),
            serde_json::json!({ "mcpServers": { "tddy-tools": the_tool_server_entry("conv-cursor") } }),
            vec![
                format!("mcp_servers.tddy-tools.command=\"{TDDY_TOOLS_PATH}\""),
                "mcp_servers.tddy-tools.args=[\"--mcp\"]".to_string(),
                format!(
                    "mcp_servers.tddy-tools.cwd=\"{}\"",
                    codex_conversation_dir.display()
                ),
            ],
            false,
            Some(oz_mcp_file.to_string_lossy().into_owned()),
            serde_json::json!({ "tddy-tools": the_tool_server_entry("conv-oz") }),
        )
    );
}

#[tokio::test]
async fn a_closed_cursor_conversation_leaves_no_cursor_mcp_json_and_a_retry_under_the_same_id_is_admitted(
) {
    // Given — a Cursor conversation that ran a turn, so its worktree holds its registration
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
    let stubs = the_stub_clis();
    let launcher = a_launcher_with_coding_agents(data_dir.path(), &stubs);
    let registration = worktree.path().join(".cursor").join("mcp.json");
    one_turn(&launcher, "cursor", "conv-1", worktree.path()).await;
    assert!(
        registration.exists(),
        "the positive control: the turn wrote it"
    );

    // When
    let removed = close_headless_conversation(data_dir.path(), "conv-1").expect("the close");
    let retried = launcher
        .headless_command_for(&a_headless_launch("cursor", "conv-1", worktree.path()))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string());

    // Then
    assert_eq!(
        (removed, registration.exists(), retried),
        (vec![registration.clone()], false, Ok(()))
    );
}

#[tokio::test]
async fn the_startup_sweep_removes_a_recorded_file_of_a_conversation_the_daemon_lost() {
    // Given — a Cursor conversation that ran a turn and was never closed
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
    let stubs = the_stub_clis();
    let launcher = a_launcher_with_coding_agents(data_dir.path(), &stubs);
    let registration = worktree.path().join(".cursor").join("mcp.json");
    one_turn(&launcher, "cursor", "conv-1", worktree.path()).await;
    assert!(
        registration.exists(),
        "the positive control: the turn wrote it"
    );

    // When
    let swept = sweep_headless_files_left_by_this_daemon(&a_daemon_config(), data_dir.path())
        .expect("the sweep");

    // Then
    assert_eq!(
        (swept, registration.exists()),
        (vec!["conv-1".to_string()], false)
    );
}

/// The variables the launch's tool server is given ([`the_tool_server_entry`]).
const TOOL_SERVER_VARIABLES: [&str; 2] = ["TDDY_REMOTE_SESSION_TOKEN", "TDDY_CONVERSATION_ID"];

#[tokio::test]
async fn a_headless_codex_conversation_carries_no_tddy_env_and_resolves_through_its_pointer() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = a_git_worktree();
    let stubs = the_stub_clis();
    let launcher = a_launcher_with_coding_agents(data_dir.path(), &stubs);
    let conversation_dir = data_dir.path().join("headless").join("conv-codex");

    // When
    one_turn(&launcher, "codex", "conv-codex", worktree.path()).await;

    // Then — Codex itself is given none of the tool server's variables, its inline registration
    // forwards none and starts the server in the conversation directory, where the conversation's
    // pointer carries them for it instead
    let found = tddy_core::session_pointer::find_session_pointer(&conversation_dir)
        .expect("the conversation's pointer is readable")
        .map(|(path, pointer)| (path, pointer.remote));
    assert_eq!(
        (
            stubs.env_seen("codex", &TOOL_SERVER_VARIABLES),
            config_overrides(&stubs.argv("codex")),
            found,
        ),
        (
            BTreeMap::new(),
            vec![
                format!("mcp_servers.tddy-tools.command=\"{TDDY_TOOLS_PATH}\""),
                "mcp_servers.tddy-tools.args=[\"--mcp\"]".to_string(),
                format!(
                    "mcp_servers.tddy-tools.cwd=\"{}\"",
                    conversation_dir.display()
                ),
            ],
            Some((
                conversation_dir.join(".tddy").join("session.json"),
                BTreeMap::from([
                    ("TDDY_CONVERSATION_ID".to_string(), "conv-codex".to_string()),
                    (
                        "TDDY_REMOTE_SESSION_TOKEN".to_string(),
                        MINTED_TOKEN.to_string()
                    ),
                ])
            )),
        )
    );
}
