//! A headless coding agent opens as a headless conversation: its launcher names the CLI command, and
//! the conversation's turns run that CLI rather than an ACP agent.
//!
//! The fake claude is a shell script that records its argv and prints a stream-json transcript, so
//! what the turn ran the CLI with is observed, not inferred. Real CLIs are not run.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_agent_backend::adapters::{
    AdapterError, AdapterRegistry, HeadlessInvocation, HeadlessMcpServer, HeadlessRenderer,
    HeadlessTurn, CLAUDE_STREAM_JSON,
};

use tddy_agent_backend::{AcpAgentCommand, HeadlessCommand, McpServerSpec};
use tddy_coding_subagent::{CodingAgentDef, CodingAgentLaunch, CodingAgentLauncher};
use tddy_discovery::subagent::{PromptOutcome, SubagentError, SubagentSession};
use tddy_session_agents::coding_agent::start_coding_agent_session;

const TURN_DEADLINE: Duration = Duration::from_secs(10);

/// A launcher that starts a headless coding agent and nothing else. Its ACP methods refuse, naming
/// this double, because this test only opens the headless branch.
struct AHeadlessCliLauncher {
    cli: HeadlessCommand,
}

#[async_trait]
impl CodingAgentLauncher for AHeadlessCliLauncher {
    async fn session_meta_for(
        &self,
        _launch: &tddy_coding_subagent::CodingAgentLaunch,
    ) -> Result<
        Option<serde_json::Map<String, serde_json::Value>>,
        tddy_discovery::subagent::SubagentError,
    > {
        Ok(None)
    }

    async fn command_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        Err(SubagentError::from(
            "the headless test launcher starts no ACP agent",
        ))
    }

    async fn mcp_servers_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        Err(SubagentError::from(
            "the headless test launcher hands out no ACP MCP servers",
        ))
    }

    async fn headless_command_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<HeadlessCommand, SubagentError> {
        Ok(self.cli.clone())
    }
}

/// A fake claude in its own directory: prints `stream` on every invocation, and records each
/// invocation's arguments, one line per call.
struct FakeClaude {
    dir: tempfile::TempDir,
}

fn a_fake_claude(stream: &[serde_json::Value]) -> FakeClaude {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("stream.jsonl");
    std::fs::write(
        &output,
        stream
            .iter()
            .map(|event| event.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .expect("write stream");
    let script = dir.path().join("claude");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{record}'\ncat '{output}'\n",
            record = dir.path().join("argv.log").display(),
            output = output.display(),
        ),
    )
    .expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    FakeClaude { dir }
}

/// A renderer standing in for claude's spec: `-p <prompt>`, `--resume <session>` on a resumed turn,
/// read through `claude-stream-json`. The argv a real spec renders is pinned by its render goldens.
#[derive(Debug)]
struct ARenderer {
    program: PathBuf,
}

impl HeadlessRenderer for ARenderer {
    fn agent_id(&self) -> &str {
        "claude"
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        None
    }

    fn render(&self, turn: &HeadlessTurn) -> Result<HeadlessInvocation, AdapterError> {
        let mut argv = vec!["-p".to_string(), turn.prompt.clone()];
        if let Some(model) = &turn.model {
            argv.extend(["--model".to_string(), model.clone()]);
        }
        if let Some(session) = turn.session.as_ref().filter(|session| session.is_resume()) {
            argv.extend(["--resume".to_string(), session.session_id().to_string()]);
        }
        Ok(HeadlessInvocation {
            program: self.program.clone(),
            argv,
            env: Vec::new(),
            files: Vec::new(),
            adapter: AdapterRegistry::builtin()
                .get(CLAUDE_STREAM_JSON)
                .expect("claude-stream-json is registered"),
            exit_ok_if_structured: false,
        })
    }
}

impl FakeClaude {
    fn command(&self) -> HeadlessCommand {
        HeadlessCommand {
            cwd: Some(self.dir.path().to_path_buf()),
            scratch_dir: Some(self.dir.path().join("conversation")),
            renderer: Some(Arc::new(ARenderer {
                program: self.dir.path().join("claude"),
            })),
            ..HeadlessCommand::default()
        }
    }

    /// The arguments of each invocation, in order.
    fn invocations(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

fn claude_stream(session: &str, text: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "type": "system", "subtype": "init", "session_id": session }),
        serde_json::json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] }, "session_id": session }),
        serde_json::json!({ "type": "result", "subtype": "success", "result": text, "session_id": session }),
    ]
}

/// A claude coding agent on the `sonnet` model, opened for a conversation of its own.
fn a_claude_launch(worktree: PathBuf) -> CodingAgentLaunch {
    CodingAgentLaunch {
        session_dir: None,
        def: CodingAgentDef {
            name: "plain-claude".to_string(),
            label: None,
            harness: "claude".to_string(),
            model: "sonnet".to_string(),
            provider_account: None,
        },
        session_id: "session-1".to_string(),
        session_os_user: "tester".to_string(),
        worktree,
        managed: false,
        session_token: "session-token".to_string(),
        conversation_id: Some("conv-1".to_string()),
    }
}

async fn within_deadline(
    turn: impl std::future::Future<Output = Result<PromptOutcome, SubagentError>>,
) -> PromptOutcome {
    tokio::time::timeout(TURN_DEADLINE, turn)
        .await
        .expect("the turn ended in time")
        .expect("the turn succeeds")
}

fn text_of(outcome: &PromptOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|block| block.text.as_str())
        .collect()
}

#[tokio::test]
async fn a_headless_coding_agent_opens_as_a_headless_conversation() {
    // Given
    let claude = a_fake_claude(&claude_stream("cl-1", "Ran it."));
    let launcher = AHeadlessCliLauncher {
        cli: claude.command(),
    };
    let launch = a_claude_launch(claude.dir.path().to_path_buf());

    // When
    let mut session = start_coding_agent_session(
        &launcher,
        &launch,
        tddy_coding_agent_specs::CodingAgentRegistry::builtin_cached(),
    )
    .await
    .expect("the headless conversation opens");
    let outcome = within_deadline(session.prompt("Run it.")).await;

    // Then
    assert_eq!(session.model(), "sonnet");
    assert_eq!(text_of(&outcome), "Ran it.");
    let invocations = claude.invocations();
    assert!(
        invocations
            .iter()
            .any(|argv| argv.contains("--model sonnet")),
        "the CLI runs on the def's model: {invocations:?}"
    );
}

/// `#usage-binding 3/5`: a subagent's turns are tapped into the session's usage directory, and once
/// the conversation is registered under the session's main one, the session's usage stream carries
/// its row, nested, with the cost its `result` reported — read through claude's own binding.
#[tokio::test]
async fn a_coding_subagent_the_daemon_opens_reports_its_own_usage_nested_under_the_session() {
    // Given — a session whose main conversation is recorded, and a subagent whose CLI reports a cost
    let session_dir = tempfile::tempdir().expect("session dir");
    tddy_usage::write_usage_context(
        session_dir.path(),
        &tddy_usage::UsageContextFile {
            spec_id: "claude".to_string(),
            agent_home: None,
            conversation_id: Some("main-conv".to_string()),
            worktree: None,
            model: None,
        },
    )
    .expect("usage context");
    let mut stream = claude_stream("cl-1", "Ran it.");
    stream[2]["total_cost_usd"] = serde_json::json!(0.25);
    let claude = a_fake_claude(&stream);
    let registry = tddy_coding_agent_specs::CodingAgentRegistry::builtin_cached();
    let events = registry
        .get("claude")
        .and_then(|spec| spec.headless())
        .expect("claude is headless")
        .usage_events
        .clone();
    let launcher = AHeadlessCliLauncher {
        cli: HeadlessCommand {
            usage_tap: tddy_agent_backend::usage_tap::UsageTapSpec::for_conversation(
                session_dir.path(),
                "conv-1",
                events,
            ),
            ..claude.command()
        },
    };
    let launch = CodingAgentLaunch {
        session_dir: Some(session_dir.path().to_path_buf()),
        ..a_claude_launch(claude.dir.path().to_path_buf())
    };

    // When — one turn runs, and the conversation is registered under the main one
    let mut session = start_coding_agent_session(&launcher, &launch, registry)
        .await
        .expect("the headless conversation opens");
    within_deadline(session.prompt("Run it.")).await;
    tddy_usage::register_conversation(
        session_dir.path(),
        &tddy_usage::RegisteredConversation {
            spec_id: "claude".to_string(),
            conversation_id: "conv-1".to_string(),
            agent_home: None,
            worktree: None,
            parent_id: "main-conv".to_string(),
            agent_label: "plain-claude".to_string(),
        },
    )
    .expect("registered");
    let rows = tddy_usage::gather_session_usage_for(session_dir.path(), &|id| {
        registry.get(id).and_then(|spec| spec.agent.usage.clone())
    });

    // Then
    let row = rows
        .iter()
        .find(|row| row.id == "conv-1")
        .unwrap_or_else(|| panic!("the subagent has a row: {rows:?}"));
    assert_eq!(row.parent_id.as_deref(), Some("main-conv"));
    assert_eq!(row.agent, "plain-claude");
    assert_eq!(row.cost_usd, Some(0.25));
}
