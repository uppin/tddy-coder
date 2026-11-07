//! Acceptance: a host PTY agent starts from the allow-listed environment (`#agent-tools 28/28`):
//! the base set, the daemon variables its spec's `pass_env` names, and what the launch sets — never
//! anything else the daemon holds.
//!
//! The daemon's environment is this test process's. The agent is a shell script that writes the
//! environment it was started with to a file. (The tmux pane's half of the rule is a unit test
//! beside the pane command, `spec_launch/pane_with_only_the_agent_env_tests.rs`.)
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-06-agent-env-allow-list.md`.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use tddy_actions::tool::{ResolvedTool, TemplateParams, ToolDef};
use tddy_agent_launch::spec_launch::{launch_host_pty, HostAgent, HostPtySession};
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;

/// How long a launched agent has to write its environment file.
const ENV_FILE_DEADLINE: Duration = Duration::from_secs(15);

const SESSION_MARKER: &str = "CLAUDE_CODE_CHILD_SESSION";
const INHERITED_TDDY: &str = "TDDY_ALLOW_LIST_PROBE";
/// The provider key the agent's spec declares in `pass_env`.
const DECLARED_KEY: &str = "LARK_API_KEY";
/// A provider key the daemon holds and the spec does not declare.
const UNDECLARED_KEY: &str = "OTHER_VENDOR_API_KEY";
/// What the launch sets on purpose.
const LAUNCH_SET: &str = "ROUTE_BASE_URL";

fn a_daemon_holding_a_marker_tddy_config_and_two_provider_keys() {
    static PLANTED: Once = Once::new();
    PLANTED.call_once(|| {
        std::env::set_var(SESSION_MARKER, "1");
        std::env::set_var(INHERITED_TDDY, "/etc/tddy/probe");
        std::env::set_var(DECLARED_KEY, "sk-lark");
        std::env::set_var(UNDECLARED_KEY, "sk-other");
    });
}

/// A PTY agent `lark` whose binary is `program` and whose spec passes [`DECLARED_KEY`].
fn a_lark_agent(program: &Path) -> HostAgent {
    let def: ToolDef = serde_yaml::from_str(&format!(
        r#"schema_version: 1
id: lark
summary: a coding agent
binary:
  command: lark
deps: []
run:
  transport: pty
  argv: ["{{binary}}"]
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: opus, curated: [opus] }}
    pass_env: [{DECLARED_KEY}]
"#
    ))
    .expect("the tool file deserializes");
    HostAgent {
        spec: CodingAgentSpec::of(&def).expect("a valid coding agent"),
        tool: ResolvedTool {
            id: "lark".to_string(),
            command: "lark".to_string(),
            binary: program.to_path_buf(),
            reads: Vec::new(),
            network: Default::default(),
        },
        deps: Vec::new(),
        env_base: tddy_coding_agent_specs::AgentEnvBase::builtin(),
    }
}

/// A stub agent that writes its environment to `env.txt` and then waits on its terminal.
struct AnEnvRecordingAgent {
    dir: tempfile::TempDir,
}

fn an_env_recording_agent() -> AnEnvRecordingAgent {
    let dir = tempfile::tempdir().expect("agent dir");
    let program = dir.path().join("lark");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\n/usr/bin/env > '{}.tmp' && mv '{0}.tmp' '{0}'\nexec cat\n",
            dir.path().join("env.txt").display()
        ),
    )
    .expect("write agent");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    AnEnvRecordingAgent { dir }
}

impl AnEnvRecordingAgent {
    fn program(&self) -> PathBuf {
        self.dir.path().join("lark")
    }

    async fn environment(&self) -> BTreeMap<String, String> {
        let file = self.dir.path().join("env.txt");
        let started = Instant::now();
        while !file.exists() {
            assert!(
                started.elapsed() < ENV_FILE_DEADLINE,
                "the agent wrote no environment within {ENV_FILE_DEADLINE:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        std::fs::read_to_string(&file)
            .expect("read env")
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }
}

#[tokio::test]
async fn a_pty_agent_sees_no_unlisted_daemon_variable() {
    // Given
    a_daemon_holding_a_marker_tddy_config_and_two_provider_keys();
    let stub = an_env_recording_agent();
    let worktree = tempfile::tempdir().expect("worktree");
    let manager = Arc::new(CliSessionManager::new());

    // When
    let _handle = launch_host_pty(
        &a_lark_agent(&stub.program()),
        &TemplateParams::new(),
        &RenderedSlots::default(),
        worktree.path(),
        HostPtySession {
            manager: &manager,
            session_id: "019d105b-ac0f-78d3-9a89-409731145a50",
            session_type: "lark-cli",
            model: "opus",
            env: vec![(LAUNCH_SET.to_string(), "https://route.example".to_string())],
            os_user: None,
        },
        None,
    )
    .await
    .expect("the agent is launched");
    let env = stub.environment().await;

    // Then — nothing the daemon holds outside the allow-list
    for dropped in [SESSION_MARKER, INHERITED_TDDY, UNDECLARED_KEY] {
        assert!(
            !env.contains_key(dropped),
            "{dropped} reached the agent: {:?}",
            env.keys().collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
async fn a_pty_agent_sees_its_pass_env_its_launch_env_and_the_base_set() {
    // Given
    a_daemon_holding_a_marker_tddy_config_and_two_provider_keys();
    let stub = an_env_recording_agent();
    let worktree = tempfile::tempdir().expect("worktree");
    let manager = Arc::new(CliSessionManager::new());

    // When
    let _handle = launch_host_pty(
        &a_lark_agent(&stub.program()),
        &TemplateParams::new(),
        &RenderedSlots::default(),
        worktree.path(),
        HostPtySession {
            manager: &manager,
            session_id: "019d105b-ac0f-78d3-9a89-409731145a51",
            session_type: "lark-cli",
            model: "opus",
            env: vec![(LAUNCH_SET.to_string(), "https://route.example".to_string())],
            os_user: None,
        },
        None,
    )
    .await
    .expect("the agent is launched");
    let env = stub.environment().await;

    // Then — guard: what the allow-list keeps still arrives
    assert_eq!(env.get(DECLARED_KEY).map(String::as_str), Some("sk-lark"));
    assert_eq!(
        env.get(LAUNCH_SET).map(String::as_str),
        Some("https://route.example")
    );
    assert!(env.contains_key("PATH"), "PATH is a base variable");
}
