//! `tddy-tools session-hook --await-answer`: an orchestrated claude-cli session's `PermissionRequest`
//! hook reports its question, waits for the orchestrator's answer, and prints Claude's decision
//! (`#home-inbox` 6/8).
//!
//! Process level, because the hook's stdout and exit code are Claude Code's contract: the decision
//! JSON is the only thing it may ever print, and it always exits 0. The daemon is a fake serving the
//! real `/rpc/{service}/{method}` router with `activity.ActivityService` mounted at its own name, so a
//! hook addressing any other coordinate is refused exactly as the daemon would refuse it.
//!
//! Lives in `tddy-tools-cli`, not `tddy-tools`: that package builds the `tddy-tools` binary.

use std::sync::{Arc, Mutex};

use assert_cmd::cargo::cargo_bin_cmd;
use prost::Message as _;
use rstest::rstest;
use tddy_rpc::{
    MultiRpcService, RpcBridge, RpcMessage, RpcResult, RpcService, ServiceEntry, Status,
};
use tddy_service::proto::activity::{
    AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse, AwaitQuestionOutcome,
    ReportSessionStatusResponse,
};

const SESSION: &str = "awaiting-session-1";
const HOOK_TOKEN: &str = "tok-await";
const REQUEST_ID: &str = "req-cargo-publish";

/// Claude's `PermissionRequest` hook input for `cargo publish`, with the rule it suggests.
const A_PERMISSION_REQUEST: &str = r#"{
    "session_id": "claude-session-1",
    "hook_event_name": "PermissionRequest",
    "tool_name": "Bash",
    "tool_input": {"command": "cargo publish"},
    "permission_suggestions": [
        {"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "cargo publish"}],
         "behavior": "allow", "destination": "localSettings"}
    ]
}"#;

// ---------------------------------------------------------------------------
// The daemon
// ---------------------------------------------------------------------------

/// Every call the fake daemon answered: the method and the body it carried.
type RecordedCalls = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// What the fake daemon answers `AwaitQuestionAnswer` with.
#[derive(Clone)]
enum TheDaemonAnswers {
    With(AwaitQuestionAnswerResponse),
    Refusing(Status),
}

/// An `activity.ActivityService` that records every call. `ReportSessionStatus` records the question
/// under [`REQUEST_ID`]; `AwaitQuestionAnswer` answers as told.
struct FakeActivityService {
    calls: RecordedCalls,
    await_answer: TheDaemonAnswers,
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
            "AwaitQuestionAnswer" => match &self.await_answer {
                TheDaemonAnswers::With(response) => RpcResult::Unary(Ok(response.encode_to_vec())),
                TheDaemonAnswers::Refusing(status) => RpcResult::Unary(Err(status.clone())),
            },
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
    async fn answering(await_answer: TheDaemonAnswers) -> Self {
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

/// The orchestrator answered with `option_id`.
fn answered(option_id: &str) -> TheDaemonAnswers {
    TheDaemonAnswers::With(AwaitQuestionAnswerResponse {
        outcome: AwaitQuestionOutcome::Answered as i32,
        option_id: option_id.to_string(),
        cancelled: false,
        message: String::new(),
        answered_by: "home-session-1".to_string(),
    })
}

fn the_wait_ended(outcome: AwaitQuestionOutcome) -> TheDaemonAnswers {
    TheDaemonAnswers::With(AwaitQuestionAnswerResponse {
        outcome: outcome as i32,
        ..Default::default()
    })
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
            .args(["--event", "PermissionRequest", "--await-answer"])
            .write_stdin(A_PERMISSION_REQUEST);
        let output = hook.assert().success().get_output().stdout.clone();
        String::from_utf8(output).expect("the hook's stdout is UTF-8")
    })
    .await
    .expect("the session-hook subprocess ran to completion")
}

trait AwaitRequestAssertions {
    fn assert_waits_on_the_reported_question(&self) -> &Self;
}

impl AwaitRequestAssertions for AwaitQuestionAnswerRequest {
    /// The wait names the session, its credential and the id the report was answered with.
    fn assert_waits_on_the_reported_question(&self) -> &Self {
        assert_eq!(
            (
                self.session_id.as_str(),
                self.hook_token.as_str(),
                self.os_user.as_str(),
                self.request_id.as_str(),
            ),
            (SESSION, HOOK_TOKEN, "testuser", REQUEST_ID),
            "what the hook waited on"
        );
        self
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_answered_permission_prints_claudes_allow_decision_json_and_exits_zero() {
    // Given a daemon whose orchestrator will allow the request once
    let daemon = ADaemon::answering(answered("allow_once")).await;

    // When Claude's PermissionRequest hook runs, waiting for an answer
    let stdout = stdout_of_the_awaiting_hook_against(&daemon).await;

    // Then it printed Claude's allow decision, and nothing else (exit 0 asserted by the runner)
    let decision: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout {stdout:?} is not JSON: {e}"));
    assert_eq!(
        decision,
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": { "behavior": "allow" }
            }
        })
    );
    // And it waited on the question its own report recorded, handing over Claude's suggestions
    let waited = daemon.the_await_it_was_asked();
    waited.assert_waits_on_the_reported_question();
    let suggestions: serde_json::Value = serde_json::from_str(&waited.permission_suggestions_json)
        .expect("the suggestions are JSON");
    assert_eq!(
        suggestions[0]["rules"][0]["ruleContent"],
        serde_json::json!("cargo publish")
    );
}

#[rstest]
#[case::the_wait_timed_out(the_wait_ended(AwaitQuestionOutcome::TimedOut))]
#[case::there_is_no_orchestrator(the_wait_ended(AwaitQuestionOutcome::NoOrchestrator))]
#[case::the_daemon_refused(TheDaemonAnswers::Refusing(Status::internal("the registry is gone")))]
#[tokio::test]
async fn a_timeout_or_daemon_error_prints_nothing_and_exits_zero(
    #[case] the_daemon_answers: TheDaemonAnswers,
) {
    // Given a daemon that will not deliver an answer
    let daemon = ADaemon::answering(the_daemon_answers).await;

    // When Claude's PermissionRequest hook runs, waiting for an answer
    let stdout = stdout_of_the_awaiting_hook_against(&daemon).await;

    // Then it asked, and printed nothing, so Claude shows its own prompt (exit 0 asserted by the
    // runner)
    daemon
        .the_await_it_was_asked()
        .assert_waits_on_the_reported_question();
    assert_eq!(stdout, "");
}
