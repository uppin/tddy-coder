//! A headless coding session's permission request yields its turn, parked on the session, and the
//! caller's answer continues that same CLI turn.
//!
//! The fake claude sleeps before it prints, so its turn is still open when the request parks. The
//! request is parked through the session's own `permissions()` handle, the way the daemon reaches it.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use tddy_agent_backend::{
    HeadlessCommand, HeadlessHarness, PermissionDecision, PermissionHandler, PermissionOptionView,
    PermissionRequest,
};
use tddy_coding_subagent::CodingAgentSession;
use tddy_discovery::subagent::{
    PermissionAnswer, PromptOutcome, StopReason, SubagentError, SubagentSession, TurnRequest,
};

const TURN_DEADLINE: Duration = Duration::from_secs(10);
/// How long the caller waits before it parks its request, so the turn is open by then.
const REQUEST_DELAY: Duration = Duration::from_millis(200);

/// A fake claude in its own directory: sleeps `sleep_secs`, then prints `stream` on every
/// invocation.
struct FakeClaude {
    dir: tempfile::TempDir,
}

fn a_fake_claude(stream: &[serde_json::Value], sleep_secs: u32) -> FakeClaude {
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
            "#!/bin/sh\nsleep {sleep_secs}\ncat '{output}'\n",
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
}

fn claude_stream(session: &str, text: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "type": "system", "subtype": "init", "session_id": session }),
        serde_json::json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] }, "session_id": session }),
        serde_json::json!({ "type": "result", "subtype": "success", "result": text, "session_id": session }),
    ]
}

/// A Bash permission request the CLI's session `cl-1` raises, offering allow and reject.
fn bash_request() -> PermissionRequest {
    PermissionRequest {
        session_id: "cl-1".to_string(),
        tool_call_id: "tu-1".to_string(),
        title: Some("Bash".to_string()),
        raw_input: None,
        options: vec![
            PermissionOptionView {
                option_id: "allow".to_string(),
                name: "Allow".to_string(),
                kind: "allow_once".to_string(),
            },
            PermissionOptionView {
                option_id: "reject".to_string(),
                name: "Reject".to_string(),
                kind: "reject_once".to_string(),
            },
        ],
    }
}

async fn within_deadline(
    turn: impl std::future::Future<Output = Result<PromptOutcome, SubagentError>>,
) -> PromptOutcome {
    tokio::time::timeout(TURN_DEADLINE, turn)
        .await
        .expect("the turn yielded in time")
        .expect("the turn succeeds")
}

fn answering(outcome: &PromptOutcome, option_id: &str) -> TurnRequest {
    let request = outcome
        .permission_request
        .as_ref()
        .expect("the turn yielded a permission request");
    TurnRequest::resuming().with_permission_answer(PermissionAnswer::Selected {
        request_id: request.request_id.clone(),
        option_id: option_id.to_string(),
    })
}

fn text_of(outcome: &PromptOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|block| block.text.as_str())
        .collect()
}

#[tokio::test]
async fn a_headless_turn_yields_a_permission_request_parked_on_its_session_and_resumes_when_answered(
) {
    // Given
    let claude = a_fake_claude(&claude_stream("cl-1", "Fixed it."), 2);
    let mut session = CodingAgentSession::start_headless(
        HeadlessHarness::Claude,
        claude.command(),
        "sonnet".to_string(),
    )
    .await
    .expect("the headless session starts");
    let permissions = session.permissions();
    let asking = tokio::spawn(async move {
        tokio::time::sleep(REQUEST_DELAY).await;
        permissions.decide(bash_request()).await
    });

    // When
    let yielded = within_deadline(session.prompt("Work")).await;
    let finished = within_deadline(session.take_turn(answering(&yielded, "allow"))).await;
    let decision = tokio::time::timeout(TURN_DEADLINE, asking)
        .await
        .expect("the parked request was answered in time")
        .expect("the parking task completes");

    // Then
    assert_eq!(yielded.stop_reason, StopReason::PermissionRequested);
    assert!(
        yielded.permission_request.is_some(),
        "the yielded turn names its request"
    );
    assert_eq!(
        decision,
        PermissionDecision::Selected {
            option_id: "allow".to_string()
        }
    );
    assert_eq!(finished.stop_reason, StopReason::EndTurn);
    assert_eq!(text_of(&finished), "Fixed it.");
}
