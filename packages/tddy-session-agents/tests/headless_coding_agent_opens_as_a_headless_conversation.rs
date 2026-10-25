//! A headless coding agent opens as a headless conversation: its launcher names the CLI command, and
//! the conversation's turns run that CLI rather than an ACP agent.
//!
//! The fake claude is a shell script that records its argv and prints a stream-json transcript, so
//! what the turn ran the CLI with is observed, not inferred. Real CLIs are not run.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
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

impl FakeClaude {
    fn command(&self) -> HeadlessCommand {
        HeadlessCommand {
            program: self.dir.path().join("claude"),
            cwd: Some(self.dir.path().to_path_buf()),
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
    let mut session = start_coding_agent_session(&launcher, &launch)
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
