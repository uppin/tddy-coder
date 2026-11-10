//! Acceptance: every agent child this crate spawns starts from the allow-listed environment
//! (`#agent-tools 28/28`) — a headless turn, an ACP agent, a model listing and a conversation-id
//! mint. Each sees exactly the environment the daemon computed for it, nothing it inherited.
//!
//! The daemon's environment is this test process's: every test plants the same session marker
//! (`CLAUDE_CODE_CHILD_SESSION`, the variable that turned off transcript saving in a desktop-dev
//! run) and an inherited `TDDY_*` variable. Each agent is a shell script that writes the
//! environment it was started with to a file.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-06-agent-env-allow-list.md`.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use tddy_agent_backend::adapters::{
    AdapterRegistry, ConversationIdAdapter, ConversationMint, CursorCreateChat, HeadlessInvocation,
    HeadlessMcpServer, HeadlessModelListing, HeadlessProtocol, HeadlessRenderer, HeadlessTurn,
    LaunchAdapters, CLAUDE_STREAM_JSON, CURSOR_MODELS,
};
use tddy_agent_backend::provider_route::{
    HarnessRoute, ProviderKind, RouteShape, RouteSupport, RoutedAgent,
};
use tddy_agent_backend::{
    AcpAgentCommand, AcpDriver, AllowOnce, CodingBackend, ConversationDriver, HeadlessCommand,
    HeadlessDriver, InvokeRequest, NoHooks, SpecHeadlessBackend,
};
use tddy_session_store::error::BackendError;

/// How long a spawned agent has to write its environment file.
const ENV_FILE_DEADLINE: Duration = Duration::from_secs(10);

/// The daemon variable no agent may see: not in the base set, declared by no spec.
const SESSION_MARKER: &str = "CLAUDE_CODE_CHILD_SESSION";
/// An inherited `TDDY_*` configuration variable.
const INHERITED_TDDY: &str = "TDDY_ALLOW_LIST_PROBE";
/// What a launch sets on purpose (a route key, or a value the spec's `pass_env` named).
const LAUNCH_SET: &str = "ROUTE_BASE_URL";

/// The daemon's environment: the variables above set in this process, once.
fn a_daemon_with_a_session_marker_and_tddy_config() {
    static PLANTED: Once = Once::new();
    PLANTED.call_once(|| {
        std::env::set_var(SESSION_MARKER, "1");
        std::env::set_var(INHERITED_TDDY, "/etc/tddy/probe");
    });
}

/// The agent's whole environment, as the daemon computed it (`AgentEnvBase::agent_env`): the base
/// variables it holds and what the launch sets on purpose.
fn the_daemon_computed_agent_env() -> Vec<(String, String)> {
    vec![
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("HOME".to_string(), "/home/agent".to_string()),
        (LAUNCH_SET.to_string(), "https://route.example".to_string()),
    ]
}

/// An agent in its own directory: a script that writes its environment to `env.txt`, then runs
/// `then` (shell).
struct AnEnvRecordingAgent {
    dir: tempfile::TempDir,
}

impl AnEnvRecordingAgent {
    fn then(then: &str) -> Self {
        let dir = tempfile::tempdir().expect("agent dir");
        let program = dir.path().join("agent");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\n/usr/bin/env > '{}.tmp' && mv '{0}.tmp' '{0}'\n{then}\n",
                dir.path().join("env.txt").display()
            ),
        )
        .expect("write agent");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { dir }
    }

    fn program(&self) -> PathBuf {
        self.dir.path().join("agent")
    }

    fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// The environment the agent was started with, waiting up to [`ENV_FILE_DEADLINE`] for it.
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

/// What an agent's environment must and must not hold.
trait AnAgentEnvironment {
    fn holds_no_unlisted_daemon_variable(&self);
    fn holds_the_env_the_daemon_computed(&self);
}

impl AnAgentEnvironment for BTreeMap<String, String> {
    fn holds_no_unlisted_daemon_variable(&self) {
        assert!(
            !self.contains_key(SESSION_MARKER),
            "{SESSION_MARKER} reached the agent: {:?}",
            self.keys().collect::<Vec<_>>()
        );
        assert!(
            !self.contains_key(INHERITED_TDDY),
            "{INHERITED_TDDY} reached the agent"
        );
    }

    fn holds_the_env_the_daemon_computed(&self) {
        assert_eq!(
            self.get(LAUNCH_SET).map(String::as_str),
            Some("https://route.example")
        );
        assert_eq!(self.get("HOME").map(String::as_str), Some("/home/agent"));
        assert!(self.contains_key("PATH"), "PATH was computed");
    }
}

/// A renderer whose every turn runs `program` with the launch's own variable, read as Claude's
/// stream-json.
#[derive(Debug)]
struct ARenderer {
    program: PathBuf,
    /// The invocation's whole environment, as the renderer computed it.
    env: Vec<(String, String)>,
}

impl HeadlessRenderer for ARenderer {
    fn agent_id(&self) -> &str {
        "lark"
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        None
    }

    fn render(&self, _turn: &HeadlessTurn) -> Result<HeadlessInvocation, BackendError> {
        Ok(HeadlessInvocation {
            program: self.program.clone(),
            argv: Vec::new(),
            env: self.env.clone(),
            files: Vec::new(),
            adapter: AdapterRegistry::builtin()
                .get(CLAUDE_STREAM_JSON)
                .expect("claude-stream-json is registered"),
            exit_ok_if_structured: false,
        })
    }
}

const A_CLAUDE_RESULT: &str =
    r#"printf '%s\n' '{"type":"result","subtype":"success","result":"ok","session_id":"sess-1"}'"#;

#[tokio::test]
async fn a_headless_turn_sees_no_unlisted_daemon_variable() {
    // Given
    a_daemon_with_a_session_marker_and_tddy_config();
    let agent = AnEnvRecordingAgent::then(A_CLAUDE_RESULT);
    let driver = HeadlessDriver::new(HeadlessCommand {
        env: the_daemon_computed_agent_env(),
        cwd: Some(agent.dir().to_path_buf()),
        scratch_dir: Some(agent.dir().join("conversation")),
        renderer: Some(Arc::new(ARenderer {
            program: agent.program(),
            env: Vec::new(),
        })),
        ..HeadlessCommand::default()
    });

    // When
    tokio::time::timeout(
        ENV_FILE_DEADLINE,
        driver.prompt_turn("Fix it".to_string(), None),
    )
    .await
    .expect("the turn ended in time")
    .expect("the turn succeeds");

    // Then
    let env = agent.environment().await;
    env.holds_no_unlisted_daemon_variable();
    env.holds_the_env_the_daemon_computed();
}

#[tokio::test]
async fn an_acp_agent_sees_no_unlisted_daemon_variable() {
    // Given — an agent that records its environment and then waits to be ended
    a_daemon_with_a_session_marker_and_tddy_config();
    let agent = AnEnvRecordingAgent::then("exec sleep 30");

    // When
    let driver = AcpDriver::spawn(
        AcpAgentCommand {
            program: agent.program(),
            args: Vec::new(),
            env: the_daemon_computed_agent_env(),
            cwd: Some(agent.dir().to_path_buf()),
        },
        Arc::new(AllowOnce),
        Arc::new(NoHooks),
    )
    .expect("the agent is spawned");
    let env = agent.environment().await;
    drop(driver);

    // Then
    env.holds_no_unlisted_daemon_variable();
    env.holds_the_env_the_daemon_computed();
}

#[tokio::test]
async fn model_listing_sees_no_unlisted_daemon_variable() {
    // Given — a spec's model listing, with the values its `pass_env` named
    a_daemon_with_a_session_marker_and_tddy_config();
    let agent = AnEnvRecordingAgent::then("exit 0");
    let backend = SpecHeadlessBackend::new(
        Arc::new(ARenderer {
            program: agent.program(),
            env: Vec::new(),
        }),
        HeadlessProtocol::default(),
        RoutedAgent {
            id: "lark".to_string(),
            product: "Lark Code".to_string(),
            support: RouteSupport::default(),
            key_env: None,
        },
    )
    .with_model_listing(HeadlessModelListing {
        program: agent.program(),
        argv: Vec::new(),
        adapter: LaunchAdapters::builtin()
            .models(CURSOR_MODELS)
            .expect("cursor-models is registered"),
        env: the_daemon_computed_agent_env(),
    });

    // When — whatever the listing parses to, the process has run
    let _ = tokio::time::timeout(ENV_FILE_DEADLINE, backend.list_models())
        .await
        .expect("the listing ended in time");

    // Then
    let env = agent.environment().await;
    env.holds_no_unlisted_daemon_variable();
    env.holds_the_env_the_daemon_computed();
}

#[tokio::test]
async fn a_conversation_id_mint_sees_no_unlisted_daemon_variable() {
    // Given — Cursor's `create-chat`, answering a chat id
    a_daemon_with_a_session_marker_and_tddy_config();
    let agent = AnEnvRecordingAgent::then("echo 0f3c2a4e-chat");
    let env = the_daemon_computed_agent_env();

    // When
    CursorCreateChat
        .mint(ConversationMint {
            binary: &agent.program(),
            workdir: agent.dir(),
            env: &env,
            deadline: ENV_FILE_DEADLINE,
        })
        .await
        .expect("a conversation id is minted");

    // Then
    let seen = agent.environment().await;
    seen.holds_no_unlisted_daemon_variable();
    seen.holds_the_env_the_daemon_computed();
}

#[tokio::test]
async fn a_routed_turns_route_variable_stands_over_a_daemon_variable_of_the_same_name() {
    // Given — a turn whose computed env holds the daemon's value of a variable its route also sets
    a_daemon_with_a_session_marker_and_tddy_config();
    let agent = AnEnvRecordingAgent::then(A_CLAUDE_RESULT);
    let backend = SpecHeadlessBackend::new(
        Arc::new(ARenderer {
            program: agent.program(),
            env: vec![
                ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                (LAUNCH_SET.to_string(), "https://daemon.example".to_string()),
            ],
        }),
        HeadlessProtocol::default(),
        RoutedAgent {
            id: "lark".to_string(),
            product: "Lark Code".to_string(),
            support: RouteSupport {
                providers: vec![ProviderKind::Anthropic],
                shape: RouteShape::Env,
            },
            key_env: None,
        },
    )
    .with_provider_route(HarnessRoute {
        env: vec![(LAUNCH_SET.to_string(), "https://route.example".to_string())],
        ..HarnessRoute::default()
    })
    .expect("lark is routable");

    // When
    tokio::time::timeout(
        ENV_FILE_DEADLINE,
        backend.invoke(InvokeRequest {
            prompt: "Fix it".to_string(),
            working_dir: Some(agent.dir().to_path_buf()),
            session_dir: Some(agent.dir().join("session")),
            ..InvokeRequest::default()
        }),
    )
    .await
    .expect("the turn ended in time")
    .expect("the turn succeeds");

    // Then
    let env = agent.environment().await;
    env.holds_no_unlisted_daemon_variable();
    assert_eq!(
        env.get(LAUNCH_SET).map(String::as_str),
        Some("https://route.example")
    );
}
