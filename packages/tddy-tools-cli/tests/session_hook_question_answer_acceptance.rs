//! `tddy-tools session-hook --await-answer` on an `AskUserQuestion`: an orchestrated claude-cli
//! session's `PreToolUse` hook reports the question, waits for the orchestrator's answers, and
//! prints the decision that hands them to Claude (`#home-inbox` 8/9).
//!
//! Process level, because the hook's stdout and exit code are Claude Code's contract. The decision
//! format is pinned whole: it is the mechanism verified against Claude Code 2.1.312 on 2026-10-28 —
//! `permissionDecision: allow` with `updatedInput` carrying the call's own `questions` plus an
//! `answers` map (question text to answer string), which Claude takes for the person's answer
//! without opening its dialog. See `question_decision.rs` in `tddy-tools`.
//!
//! The daemon is a fake serving the real `/rpc/{service}/{method}` router with
//! `activity.ActivityService` mounted at its own name, as `session_hook_await_answer_acceptance.rs`
//! (`6/8`) does.
//!
//! Depends on `permission-answer` (`6/8`) for the hook's await mode and on `elicit-content` (`3/8`)
//! for the `PreToolUse(AskUserQuestion)` status mapping that makes the hook report at all.
//!
//! Lives in `tddy-tools-cli`, not `tddy-tools` as the changeset says: that package builds the
//! `tddy-tools` binary (`6/8` made the same move).

use std::sync::{Arc, Mutex};

use assert_cmd::cargo::cargo_bin_cmd;
use prost::Message as _;
use tddy_rpc::{MultiRpcService, RpcBridge, RpcMessage, RpcResult, RpcService, ServiceEntry};
use tddy_service::proto::activity::{
    AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse, AwaitQuestionOutcome, QuestionAnswer,
    ReportSessionStatusResponse,
};

const SESSION: &str = "awaiting-session-2";
const HOOK_TOKEN: &str = "tok-await-question";
const REQUEST_ID: &str = "req-which-language";

/// The `questions` of Claude's `AskUserQuestion` call: one single-choice, one multi-select.
fn the_questions() -> serde_json::Value {
    serde_json::json!([
        {
            "question": "Which language?", "header": "Language", "multiSelect": false,
            "options": [
                {"label": "Rust", "description": "The default"},
                {"label": "Go", "description": "The alternative"}
            ]
        },
        {
            "question": "Which checks should run?", "header": "Checks", "multiSelect": true,
            "options": [
                {"label": "fmt", "description": "rustfmt"},
                {"label": "clippy", "description": "lints"},
                {"label": "tests", "description": "the suite"}
            ]
        }
    ])
}

/// Claude's `PreToolUse` hook input for that call.
fn an_ask_user_question() -> String {
    serde_json::json!({
        "session_id": "claude-session-2",
        "hook_event_name": "PreToolUse",
        "tool_name": "AskUserQuestion",
        "tool_input": { "questions": the_questions() }
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// The daemon
// ---------------------------------------------------------------------------

/// Every call the fake daemon answered: the method and the body it carried.
type RecordedCalls = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// An `activity.ActivityService` that records every call. `ReportSessionStatus` records the question
/// under [`REQUEST_ID`]; `AwaitQuestionAnswer` answers as told.
struct FakeActivityService {
    calls: RecordedCalls,
    await_answer: AwaitQuestionAnswerResponse,
}

#[async_trait::async_trait]
impl RpcService for FakeActivityService {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        self.calls
            .lock()
            .expect("recorded calls")
            .push((method.to_string(), message.payload.clone()));
        match method {
            "ReportSessionStatus" => RpcResult::Unary(Ok(ReportSessionStatusResponse {
                ok: true,
                request_id: REQUEST_ID.to_string(),
            }
            .encode_to_vec())),
            "AwaitQuestionAnswer" => RpcResult::Unary(Ok(self.await_answer.encode_to_vec())),
            _ => RpcResult::Unary(Ok(Vec::new())),
        }
    }
}

/// The fake daemon on an ephemeral loopback port. Dropping it stops serving.
struct ADaemon {
    base_url: String,
    calls: RecordedCalls,
    serving: tokio::task::JoinHandle<()>,
}

impl ADaemon {
    async fn answering(await_answer: AwaitQuestionAnswerResponse) -> Self {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let router = tddy_connectrpc::connect_router(RpcBridge::new(MultiRpcService::new(vec![
            ServiceEntry {
                name: tddy_service::session_activity::ACTIVITY_SERVICE,
                service: Arc::new(FakeActivityService {
                    calls: Arc::clone(&calls),
                    await_answer,
                }) as Arc<dyn RpcService>,
            },
        ])));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the fake daemon");
        let base_url = format!("http://{}", listener.local_addr().expect("its address"));
        let serving = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("the fake daemon served");
        });
        Self {
            base_url,
            calls,
            serving,
        }
    }

    /// The `AwaitQuestionAnswer` request the hook sent, or a failure naming what it sent instead.
    fn the_await_it_was_asked(&self) -> AwaitQuestionAnswerRequest {
        let calls = self.calls.lock().expect("recorded calls");
        let body = calls
            .iter()
            .find(|(method, _)| method == "AwaitQuestionAnswer")
            .map(|(_, body)| body.clone())
            .unwrap_or_else(|| {
                panic!(
                    "the hook never asked AwaitQuestionAnswer; it called {:?}",
                    calls.iter().map(|(m, _)| m.as_str()).collect::<Vec<_>>()
                )
            });
        AwaitQuestionAnswerRequest::decode(&body[..])
            .expect("decode the AwaitQuestionAnswerRequest")
    }
}

impl Drop for ADaemon {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

/// The orchestrator answered both questions: Go to the first as free text, and to the second two
/// labels and some free text.
fn answered_with_labels_and_free_text() -> AwaitQuestionAnswerResponse {
    AwaitQuestionAnswerResponse {
        outcome: AwaitQuestionOutcome::Answered as i32,
        answered_by: "home-session-1".to_string(),
        answers: vec![
            QuestionAnswer {
                question_index: 0,
                selected: Vec::new(),
                other: Some("Zig".to_string()),
            },
            QuestionAnswer {
                question_index: 1,
                selected: vec!["fmt".to_string(), "clippy".to_string()],
                other: Some("bench".to_string()),
            },
        ],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// The hook
// ---------------------------------------------------------------------------

/// What the hook printed on stdout. Exit 0 is asserted: a non-zero exit is a hook failure Claude
/// reports, never a decision.
async fn stdout_of_the_awaiting_hook_against(daemon: &ADaemon) -> String {
    let daemon = daemon.base_url.clone();
    tokio::task::spawn_blocking(move || {
        let mut hook = cargo_bin_cmd!("tddy-tools");
        hook.env_remove("TDDY_SOCKET")
            .args(["session-hook", "--session", SESSION, "--daemon", &daemon])
            .args(["--os-user", "testuser", "--hook-token", HOOK_TOKEN])
            .args(["--event", "PreToolUse", "--await-answer"])
            .write_stdin(an_ask_user_question());
        let output = hook.assert().success().get_output().stdout.clone();
        String::from_utf8(output).expect("the hook's stdout is UTF-8")
    })
    .await
    .expect("the session-hook subprocess ran to completion")
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_answered_question_prints_the_decision_that_hands_claude_those_answers() {
    // Given a daemon whose orchestrator answered both questions
    let daemon = ADaemon::answering(answered_with_labels_and_free_text()).await;

    // When Claude's PreToolUse hook for AskUserQuestion runs, waiting for an answer
    let stdout = stdout_of_the_awaiting_hook_against(&daemon).await;

    // Then it waited on the question its own report recorded
    let waited = daemon.the_await_it_was_asked();
    assert_eq!(
        (
            waited.session_id.as_str(),
            waited.hook_token.as_str(),
            waited.request_id.as_str()
        ),
        (SESSION, HOOK_TOKEN, REQUEST_ID),
        "what the hook waited on"
    );
    // And it printed the allow decision whose updated input is the call's own questions plus the
    // answers keyed by question text — labels joined with ", ", free text after them — and nothing
    // else (exit 0 asserted by the runner)
    let decision: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout {stdout:?} is not JSON: {e}"));
    assert_eq!(
        decision,
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": {
                    "questions": the_questions(),
                    "answers": {
                        "Which language?": "Zig",
                        "Which checks should run?": "fmt, clippy, bench"
                    }
                }
            }
        })
    );
}

#[tokio::test]
async fn with_no_answer_before_the_deadline_the_question_hook_prints_nothing() {
    // Given a daemon whose wait ran out with nobody answering
    let daemon = ADaemon::answering(AwaitQuestionAnswerResponse {
        outcome: AwaitQuestionOutcome::TimedOut as i32,
        ..Default::default()
    })
    .await;

    // When Claude's PreToolUse hook for AskUserQuestion runs, waiting for an answer
    let stdout = stdout_of_the_awaiting_hook_against(&daemon).await;

    // Then it asked, and printed nothing, so Claude shows its own dialog (exit 0 asserted by the
    // runner)
    daemon.the_await_it_was_asked();
    assert_eq!(stdout, "");
}
