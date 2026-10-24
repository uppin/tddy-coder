//! Acceptance tests for `AcpDriver` — the one ACP client every ACP coding backend runs on.
//!
//! Every test drives a real `tddy-acp-stub` child. The stub records what it was asked
//! (`TDDY_ACP_STUB_RECORD`), which is how client-side behaviour — the model it set, the MCP servers
//! it passed, the cancel it sent, the permission answer it gave — is observed without a mock.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-acp-driver.md

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::{
    AcpAgentCommand, AcpDriver, AcpStopReason, AcpUpdate, AcpUpdateSink, McpServerSpec, NoHooks,
    OpenSession, PermissionDecision, PermissionHandler, PermissionRequest,
};

const TURN_DEADLINE: Duration = Duration::from_secs(10);

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// A stub agent to spawn: its scripted responses, the models it advertises, and where it records.
struct StubAgent {
    dir: tempfile::TempDir,
    scenario: serde_json::Value,
}

fn a_stub_agent() -> StubAgent {
    StubAgent {
        dir: tempfile::tempdir().expect("tempdir"),
        scenario: serde_json::json!({ "responses": [] }),
    }
}

impl StubAgent {
    fn answering(mut self, response: serde_json::Value) -> Self {
        self.scenario["responses"]
            .as_array_mut()
            .expect("responses")
            .push(response);
        self
    }

    fn offering_models(mut self, models: &[&str]) -> Self {
        self.scenario["models"] = serde_json::json!(models);
        self
    }

    fn record_path(&self) -> PathBuf {
        self.dir.path().join("record.jsonl")
    }

    fn command(&self) -> AcpAgentCommand {
        let scenario_path = self.dir.path().join("scenario.json");
        std::fs::write(&scenario_path, self.scenario.to_string()).expect("write scenario");
        AcpAgentCommand {
            program: stub_agent_path(),
            args: Vec::new(),
            env: vec![
                (
                    "TDDY_ACP_SCENARIO".to_string(),
                    scenario_path.display().to_string(),
                ),
                (
                    "TDDY_ACP_STUB_RECORD".to_string(),
                    self.record_path().display().to_string(),
                ),
            ],
            cwd: None,
        }
    }

    fn spawn_driver(&self) -> AcpDriver {
        self.spawn_driver_with(Arc::new(tddy_agent_backend::AllowOnce))
    }

    fn spawn_driver_with(&self, permissions: Arc<dyn PermissionHandler>) -> AcpDriver {
        AcpDriver::spawn(self.command(), permissions, Arc::new(NoHooks)).expect("spawn driver")
    }

    /// Every recorded line whose `event` is `event`.
    fn recorded(&self, event: &str) -> Vec<serde_json::Value> {
        recorded_events(&self.record_path(), event)
    }
}

fn recorded_events(path: &Path, event: &str) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("record line is JSON"))
        .filter(|line| line["event"] == event)
        .collect()
}

fn a_response() -> serde_json::Value {
    serde_json::json!({ "chunks": [], "tool_calls": [], "permission_requests": [] })
}

fn saying(chunks: &[&str]) -> serde_json::Value {
    let mut response = a_response();
    response["chunks"] = serde_json::json!(chunks);
    response
}

fn a_fresh_session_in(dir: &Path) -> OpenSession {
    OpenSession {
        cwd: dir.to_path_buf(),
        ..OpenSession::default()
    }
}

/// A permission handler that always picks `option_id`, and remembers what it was asked.
struct Choosing {
    option_id: &'static str,
    asked: Mutex<Vec<PermissionRequest>>,
}

#[async_trait::async_trait]
impl PermissionHandler for Choosing {
    async fn decide(&self, request: PermissionRequest) -> PermissionDecision {
        self.asked.lock().expect("asked").push(request);
        PermissionDecision::Selected {
            option_id: self.option_id.to_string(),
        }
    }
}

fn collecting_updates() -> (AcpUpdateSink, Arc<Mutex<Vec<AcpUpdate>>>) {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let sink_updates = Arc::clone(&updates);
    let sink: AcpUpdateSink = Arc::new(move |update| {
        sink_updates.lock().expect("updates").push(update);
    });
    (sink, updates)
}

fn process_is_running(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[tokio::test]
#[serial]
async fn a_prompt_returns_the_agents_text_and_stop_reason() {
    // Given
    let stub = a_stub_agent().answering(saying(&["Hello, ", "caller."]));
    let driver = stub.spawn_driver();
    let session = driver
        .open_session(a_fresh_session_in(stub.dir.path()))
        .await
        .expect("open session");

    // When
    let turn = tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt(&session.session_id, "Say hello".to_string(), None, None),
    )
    .await
    .expect("turn finished in time")
    .expect("prompt succeeds");

    // Then
    assert_eq!(turn.stop_reason, AcpStopReason::EndTurn);
    assert_eq!(turn.text, "Hello, caller.");
}

#[tokio::test]
#[serial]
async fn a_requested_model_is_set_on_the_session() {
    // Given
    let stub = a_stub_agent().offering_models(&["model-a", "model-b"]);
    let driver = stub.spawn_driver();

    // When
    let session = driver
        .open_session(OpenSession {
            model: Some("model-b".to_string()),
            ..a_fresh_session_in(stub.dir.path())
        })
        .await
        .expect("open session");

    // Then
    let set_model = stub.recorded("set_session_model");
    assert_eq!(set_model.len(), 1, "one set_session_model: {set_model:?}");
    assert_eq!(set_model[0]["model_id"], "model-b");
    assert_eq!(set_model[0]["session_id"], session.session_id.as_str());
    assert_eq!(session.current_model.as_deref(), Some("model-b"));
}

#[tokio::test]
#[serial]
async fn a_model_the_agent_does_not_offer_is_refused_naming_the_offered_ones() {
    // Given
    let stub = a_stub_agent().offering_models(&["model-a", "model-b"]);
    let driver = stub.spawn_driver();

    // When
    let refusal = driver
        .open_session(OpenSession {
            model: Some("model-z".to_string()),
            ..a_fresh_session_in(stub.dir.path())
        })
        .await
        .expect_err("an unknown model is refused");

    // Then
    let message = refusal.to_string();
    assert!(message.contains("model-z"), "names the request: {message}");
    assert!(
        message.contains("model-a"),
        "names an offered model: {message}"
    );
    assert!(
        message.contains("model-b"),
        "names an offered model: {message}"
    );
    assert!(
        stub.recorded("set_session_model").is_empty(),
        "nothing was set"
    );
}

#[tokio::test]
#[serial]
async fn cancel_ends_a_running_turn_as_cancelled() {
    // Given
    let mut held = saying(&["working…"]);
    held["hold_until_cancel"] = serde_json::json!(true);
    let stub = a_stub_agent().answering(held);
    let driver = Arc::new(stub.spawn_driver());
    let session = driver
        .open_session(a_fresh_session_in(stub.dir.path()))
        .await
        .expect("open session");
    let turn_driver = Arc::clone(&driver);
    let turn_session = session.session_id.clone();
    let turn = tokio::spawn(async move {
        turn_driver
            .prompt(&turn_session, "Work until stopped".to_string(), None, None)
            .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    // When
    driver
        .cancel(&session.session_id)
        .await
        .expect("cancel is sent");

    // Then
    let ended = tokio::time::timeout(TURN_DEADLINE, turn)
        .await
        .expect("the turn ended after cancel")
        .expect("turn task")
        .expect("a cancelled turn is not an error");
    assert_eq!(ended.stop_reason, AcpStopReason::Cancelled);
    assert_eq!(stub.recorded("cancel").len(), 1);
}

#[tokio::test]
#[serial]
async fn dropping_the_driver_terminates_the_agent_process() {
    // Given
    let stub = a_stub_agent();
    let driver = stub.spawn_driver();
    driver
        .open_session(a_fresh_session_in(stub.dir.path()))
        .await
        .expect("open session");
    let pid = stub.recorded("started")[0]["pid"]
        .as_u64()
        .expect("the stub recorded its pid") as u32;
    assert!(
        process_is_running(pid),
        "the agent runs while the driver lives"
    );

    // When
    drop(driver);

    // Then
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while process_is_running(pid) && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!process_is_running(pid), "the agent process {pid} is gone");
}

#[tokio::test]
#[serial]
async fn mcp_servers_reach_session_new() {
    // Given
    let stub = a_stub_agent();
    let driver = stub.spawn_driver();
    let tddy_tools = McpServerSpec {
        name: "tddy-tools".to_string(),
        command: PathBuf::from("/usr/local/bin/tddy-tools"),
        args: vec!["--mcp".to_string()],
        env: vec![("TDDY_SESSION_ID".to_string(), "s-1".to_string())],
    };

    // When
    driver
        .open_session(OpenSession {
            mcp_servers: vec![tddy_tools],
            ..a_fresh_session_in(stub.dir.path())
        })
        .await
        .expect("open session");

    // Then
    let new_session = stub.recorded("new_session");
    assert_eq!(new_session.len(), 1);
    assert_eq!(
        new_session[0]["mcp_servers"],
        serde_json::json!(["tddy-tools"])
    );
}

#[tokio::test]
#[serial]
async fn a_permission_request_is_answered_by_the_installed_handler() {
    // Given
    let mut asking = a_response();
    asking["permission_requests"] = serde_json::json!([{ "title": "Write src/lib.rs" }]);
    let stub = a_stub_agent().answering(asking);
    let rejecting = Arc::new(Choosing {
        option_id: "reject-once",
        asked: Mutex::new(Vec::new()),
    });
    let driver = stub.spawn_driver_with(rejecting.clone());
    let session = driver
        .open_session(a_fresh_session_in(stub.dir.path()))
        .await
        .expect("open session");

    // When
    tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt(&session.session_id, "Edit the file".to_string(), None, None),
    )
    .await
    .expect("turn finished in time")
    .expect("prompt succeeds");

    // Then
    let asked = rejecting.asked.lock().expect("asked");
    assert_eq!(asked.len(), 1, "the handler was asked once");
    assert_eq!(asked[0].title.as_deref(), Some("Write src/lib.rs"));
    let option_ids: Vec<&str> = asked[0]
        .options
        .iter()
        .map(|o| o.option_id.as_str())
        .collect();
    assert_eq!(option_ids, ["allow-once", "reject-once"]);
    let outcome = stub.recorded("permission_outcome");
    assert_eq!(outcome[0]["outcome"], "selected:reject-once");
}

#[tokio::test]
#[serial]
async fn a_session_id_resumes_by_session_load() {
    // Given
    let first = a_stub_agent().answering(saying(&["first"]));
    let session_id = {
        let driver = first.spawn_driver();
        driver
            .open_session(a_fresh_session_in(first.dir.path()))
            .await
            .expect("open session")
            .session_id
    };
    let second = a_stub_agent();
    let driver = second.spawn_driver();

    // When
    let resumed = driver
        .open_session(OpenSession {
            resume: Some(session_id.clone()),
            ..a_fresh_session_in(second.dir.path())
        })
        .await
        .expect("resume session");

    // Then
    assert_eq!(resumed.session_id, session_id);
    let loaded = second.recorded("load_session");
    assert_eq!(loaded.len(), 1, "resumed by session/load: {loaded:?}");
    assert_eq!(loaded[0]["session_id"], session_id.as_str());
    assert!(
        second.recorded("new_session").is_empty(),
        "no fresh session"
    );
}

#[tokio::test]
#[serial]
async fn updates_stream_as_tool_calls_and_text() {
    // Given
    let mut with_tool = saying(&["Reading."]);
    with_tool["tool_calls"] = serde_json::json!([{ "name": "Read", "input": { "path": "a.rs" } }]);
    let stub = a_stub_agent().answering(with_tool);
    let driver = stub.spawn_driver();
    let session = driver
        .open_session(a_fresh_session_in(stub.dir.path()))
        .await
        .expect("open session");
    let (sink, updates) = collecting_updates();

    // When
    tokio::time::timeout(
        TURN_DEADLINE,
        driver.prompt(
            &session.session_id,
            "Read a.rs".to_string(),
            Some(sink),
            None,
        ),
    )
    .await
    .expect("turn finished in time")
    .expect("prompt succeeds");

    // Then
    let updates = updates.lock().expect("updates");
    assert_eq!(
        *updates,
        vec![
            AcpUpdate::Text("Reading.".to_string()),
            AcpUpdate::ToolCall {
                id: "tool-Read".to_string(),
                title: "Read".to_string(),
                kind: "other".to_string(),
                raw_input: Some(serde_json::json!({ "path": "a.rs" })),
            },
        ]
    );
}
