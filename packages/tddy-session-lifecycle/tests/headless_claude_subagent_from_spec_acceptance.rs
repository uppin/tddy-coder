//! Acceptance: a headless `claude` coding subagent is launched from the `claude` tool definition's
//! `coding_agent.headless` section — its binary resolved by the agent registry, its argv rendered
//! from the template, and its MCP config and approval tool from the declared slots.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md` (`#agent-tools 6/12`).
//!
//! The daemon config names a `claude` that does not exist, so only the binary the spec resolves —
//! a stub on a fixture `ToolHost` — can run. The stub records its argv and the MCP config it was
//! handed, then prints a recorded stream-json turn.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::load_agent_registry;
use tddy_coding_subagent::{CodingAgentDef, CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::{ConversationDriver, HeadlessDriver, HeadlessHarness};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::coding_agent_as_user::DropContext;
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;

const TURN_DEADLINE: Duration = Duration::from_secs(10);
const MODEL: &str = "sonnet";
const CONVERSATION: &str = "conv-1";
const SESSION_ID: &str = "1780828020298-coding";
/// Where the launch's tool server resolves `tddy-tools` to.
const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";
const MINTED_TOKEN: &str = "minted-agent-token";
const APPROVAL_TOOL: &str = "mcp__tddy-tools__approval_prompt";

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

/// A stub `claude` in its own directory: records its argv (one entry per line) and the MCP config
/// it was handed, then prints one stream-json turn.
struct AStubClaude {
    dir: tempfile::TempDir,
}

fn a_stub_claude() -> AStubClaude {
    let dir = tempfile::tempdir().expect("stub dir");
    let script = dir.path().join("claude");
    std::fs::write(
        &script,
        format!(
            r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a"; done > '{argv}'
mcp=""; prev=""
for a in "$@"; do [ "$prev" = "--mcp-config" ] && mcp="$a"; prev="$a"; done
/bin/cat "$mcp" > '{mcp}' 2>/dev/null
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"cl-1"}}'
printf '%s\n' '{{"type":"result","subtype":"success","result":"Fixed it.","session_id":"cl-1"}}'
"#,
            argv = dir.path().join("argv.log").display(),
            mcp = dir.path().join("mcp.seen").display(),
        ),
    )
    .expect("write stub");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let tools = dir.path().join("tddy-tools");
    std::fs::write(&tools, "#!/bin/sh\n").expect("write tddy-tools stub");
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    AStubClaude { dir }
}

impl AStubClaude {
    fn binary(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path().join("claude")).expect("stub path")
    }

    fn tddy_tools(&self) -> PathBuf {
        self.dir.path().join("tddy-tools")
    }

    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.log"))
            .expect("the stub ran")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn mcp_config_seen(&self) -> serde_json::Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("mcp.seen")).expect("the stub ran"),
        )
        .expect("the MCP config is JSON")
    }
}

/// A daemon config whose own `claude` does not exist.
fn a_daemon_config() -> DaemonConfig {
    serde_yaml::from_str(
        "users:\n  - github_user: u\n    os_user: u\nclaude_cli:\n  binary_path: /nonexistent/claude-is-never-run\n",
    )
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

fn a_headless_claude_launch(worktree: &Path) -> CodingAgentLaunch {
    CodingAgentLaunch {
        def: CodingAgentDef {
            name: "fix-the-parser".to_string(),
            label: None,
            harness: "claude".to_string(),
            model: MODEL.to_string(),
            provider_account: None,
        },
        session_id: SESSION_ID.to_string(),
        session_os_user: this_user(),
        worktree: worktree.to_path_buf(),
        managed: false,
        session_token: "caller-session-token".to_string(),
        conversation_id: Some(CONVERSATION.to_string()),
    }
}

/// A launcher wired for headless conversations under `data_dir`, with the coding agents of an empty
/// `<tddyhome>/tools` resolved against a host that finds `stub`.
fn a_launcher_with_coding_agents(data_dir: &Path, stub: &AStubClaude) -> DaemonCodingAgentLauncher {
    let tools = data_dir.join("tools");
    let registry = load_agent_registry(&tools).expect("the agent registry loads");
    let host = AFixtureHost {
        env: BTreeMap::from([
            (
                "TDDY_CLAUDE_BINARY".to_string(),
                stub.binary().to_string_lossy().into_owned(),
            ),
            (
                "TDDY_TOOLS_BINARY".to_string(),
                stub.tddy_tools().to_string_lossy().into_owned(),
            ),
        ]),
    };
    DaemonCodingAgentLauncher::with_identity(
        the_daemons_own_identity(),
        Arc::new(|_| Ok(None)),
        Arc::new(|launch| {
            Ok((
                PathBuf::from(TDDY_TOOLS_PATH),
                vec![
                    (
                        "TDDY_REMOTE_SESSION_TOKEN".to_string(),
                        MINTED_TOKEN.to_string(),
                    ),
                    (
                        "TDDY_REMOTE_SESSION_ID".to_string(),
                        launch.session_id.clone(),
                    ),
                ],
            ))
        }),
    )
    .with_headless_wiring(a_daemon_config(), data_dir.join("headless"))
    .with_coding_agents(Arc::new(HostCodingAgents::new(registry, Arc::new(host))))
}

#[tokio::test]
async fn a_headless_claude_coding_subagent_launches_with_mcp_config_and_approval_tool_rendered_from_the_spec(
) {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let stub = a_stub_claude();
    let launcher = a_launcher_with_coding_agents(data_dir.path(), &stub);
    let command = launcher
        .headless_command_for(&a_headless_claude_launch(worktree.path()))
        .await
        .expect("the claude conversation is planned from its spec");
    let driver = HeadlessDriver::new(HeadlessHarness::Claude, command);

    // When
    let turn = tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt_turn("Fix the parser".to_string(), None),
    )
    .await
    .expect("the turn ended in time")
    .expect("the turn succeeds");

    // Then — the argv the hand-built headless driver ran, rendered from the spec
    let mcp_config = data_dir
        .path()
        .join("headless")
        .join(CONVERSATION)
        .join("claude-mcp-config.json");
    assert_eq!(turn.text, "Fixed it.");
    assert_eq!(
        stub.argv(),
        vec![
            "-p".to_string(),
            "Fix the parser".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--model".to_string(),
            MODEL.to_string(),
            "--permission-prompt-tool".to_string(),
            APPROVAL_TOOL.to_string(),
            "--mcp-config".to_string(),
            mcp_config.to_string_lossy().into_owned(),
        ]
    );
    assert_eq!(
        stub.mcp_config_seen(),
        serde_json::json!({
            "mcpServers": {
                "tddy-tools": {
                    "command": TDDY_TOOLS_PATH,
                    "args": ["--mcp"],
                    "env": {
                        "TDDY_REMOTE_SESSION_TOKEN": MINTED_TOKEN,
                        "TDDY_REMOTE_SESSION_ID": SESSION_ID,
                        "TDDY_CONVERSATION_ID": CONVERSATION,
                    },
                }
            }
        })
    );
    let mode = std::fs::metadata(&mcp_config)
        .expect("the MCP config stays for the conversation")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "the registration carries a credential: owner only"
    );
}
