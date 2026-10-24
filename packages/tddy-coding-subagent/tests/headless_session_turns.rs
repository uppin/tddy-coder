//! A headless coding session answers a turn with its CLI's text, and its next turn resumes the CLI's
//! own session.
//!
//! The fake claude is a shell script that appends its argv to a record and prints a stream-json
//! transcript, so what each turn ran the CLI with is observed, not inferred. Real CLIs are not run.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use tddy_agent_backend::{HeadlessCommand, HeadlessHarness};
use tddy_coding_subagent::CodingAgentSession;
use tddy_discovery::subagent::{PromptOutcome, StopReason, SubagentError, SubagentSession};

const TURN_DEADLINE: Duration = Duration::from_secs(10);

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
async fn a_headless_session_answers_a_turn_with_its_cli_text_and_resumes_the_same_cli_session() {
    // Given
    let claude = a_fake_claude(&claude_stream("cl-1", "Fixed it."));
    let mut session = CodingAgentSession::start_headless(
        HeadlessHarness::Claude,
        claude.command(),
        "sonnet".to_string(),
    )
    .await
    .expect("the headless session starts");

    // When
    let first = within_deadline(session.prompt("Fix it")).await;
    within_deadline(session.prompt("And test it")).await;

    // Then
    assert_eq!(first.stop_reason, StopReason::EndTurn);
    assert_eq!(text_of(&first), "Fixed it.");
    let invocations = claude.invocations();
    assert_eq!(invocations.len(), 2, "{invocations:?}");
    assert!(
        invocations[1].contains("--resume cl-1"),
        "{}",
        invocations[1]
    );
}
