use super::*;
use futures_util::StreamExt;
use std::time::Duration;
use tddy_core::agent_activity::{
    append_agent_activity, read_agent_activity, AgentActivityRecord, STATUS_COMPLETED,
    STATUS_RUNNING,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};

const TEST_HOOK_TOKEN: &str = "tok-activity-hook-xyz789";
const TEST_OS_USER: &str = "u";

fn make_unit_config() -> crate::config::DaemonConfig {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
    let config = make_unit_config();
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver =
        Arc::new(|token| (token == "valid").then(|| "u".to_string()));
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        sessions_base,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

fn write_claude_cli_session(session_dir: &std::path::Path, hook_token: &str) {
    let session_id = session_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let metadata = SessionMetadata {
        session_id,
        project_id: "proj-activity-unit".to_string(),
        created_at: "2026-06-13T10:00:00Z".to_string(),
        updated_at: "2026-06-13T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/activity-test".to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-sonnet-4-6".to_string()),
        cursor_chat_id: None,
        activity_status: None,
        hook_token: Some(hook_token.to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

fn a_pre_tool_use(
    session_id: &str,
    tool_name: &str,
    input_json: &str,
) -> ReportAgentActivityRequest {
    ReportAgentActivityRequest {
        session_id: session_id.to_string(),
        hook_token: TEST_HOOK_TOKEN.to_string(),
        os_user: TEST_OS_USER.to_string(),
        event: "PreToolUse".to_string(),
        tool_name: tool_name.to_string(),
        input_json: input_json.to_string(),
        result_json: String::new(),
        is_error: false,
        error_message: String::new(),
    }
}

fn a_post_tool_use(
    session_id: &str,
    tool_name: &str,
    result_json: &str,
) -> ReportAgentActivityRequest {
    ReportAgentActivityRequest {
        session_id: session_id.to_string(),
        hook_token: TEST_HOOK_TOKEN.to_string(),
        os_user: TEST_OS_USER.to_string(),
        event: "PostToolUse".to_string(),
        tool_name: tool_name.to_string(),
        input_json: String::new(),
        result_json: result_json.to_string(),
        is_error: false,
        error_message: String::new(),
    }
}

fn a_seeded_record(call_id: &str, tool_name: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: tool_name.to_string(),
        input: serde_json::json!({ "path": "src/main.rs" }),
        status: STATUS_COMPLETED.to_string(),
        result: serde_json::json!({ "content": "fn main() {}" }),
        error_message: String::new(),
        started_unix_ms: 1_700_000_000_000,
        completed_unix_ms: 1_700_000_000_500,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}

/// A PreToolUse then PostToolUse pair for one call appends a `running` then a terminal row that
/// coalesce (by shared call_id) into a single completed record in `agent-activity.jsonl`.
#[tokio::test]
async fn report_pre_then_post_tool_use_coalesces_into_one_completed_call() {
    // Given a registered claude-cli session with a valid hook_token.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-pair-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
    let service = make_unit_service(sessions_base);

    // When the hook reports PreToolUse (Bash starts) then PostToolUse (Bash finished).
    service
        .report_agent_activity(Request::new(a_pre_tool_use(
            session_id,
            "Bash",
            r#"{"command":"cargo test"}"#,
        )))
        .await
        .unwrap();
    service
        .report_agent_activity(Request::new(a_post_tool_use(
            session_id,
            "Bash",
            r#"{"stdout":"ok","exit_code":0}"#,
        )))
        .await
        .unwrap();

    // Then the two rows coalesce into one completed call carrying the terminal state.
    let records = read_agent_activity(&session_dir).unwrap();
    assert_eq!(records.len(), 1, "the pair must coalesce into one call");
    assert_eq!(records[0].tool_name, "Bash");
    assert_eq!(records[0].status, STATUS_COMPLETED);
    assert_eq!(
        records[0].result,
        serde_json::json!({ "stdout": "ok", "exit_code": 0 })
    );
    assert_eq!(records[0].source, "claude-cli");
}

/// A PreToolUse alone appends a single `running` row (no terminal row yet).
#[tokio::test]
async fn report_pre_tool_use_alone_appends_a_running_row() {
    // Given a registered claude-cli session.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-running-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
    let service = make_unit_service(sessions_base);

    // When only a PreToolUse is reported.
    service
        .report_agent_activity(Request::new(a_pre_tool_use(
            session_id,
            "Read",
            r#"{"path":"README.md"}"#,
        )))
        .await
        .unwrap();

    // Then the single recorded call is still running.
    let records = read_agent_activity(&session_dir).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].tool_name, "Read");
    assert_eq!(records[0].status, STATUS_RUNNING);
    assert_eq!(records[0].completed_unix_ms, 0);
}

/// A wrong hook_token is rejected before any activity is written.
#[tokio::test]
async fn report_agent_activity_rejects_bad_hook_token() {
    // Given a registered claude-cli session.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-bad-token-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
    let service = make_unit_service(sessions_base);

    // When the hook presents the wrong token.
    let mut req = a_pre_tool_use(session_id, "Bash", "{}");
    req.hook_token = "wrong-token".to_string();
    let err = service
        .report_agent_activity(Request::new(req))
        .await
        .unwrap_err();

    // Then it is denied and nothing is written.
    assert_eq!(err.code, tddy_rpc::Code::PermissionDenied);
    assert!(read_agent_activity(&session_dir).unwrap().is_empty());
}

/// StreamSessionActivity replays the persisted snapshot rows, in first-seen order, on subscribe.
#[tokio::test]
async fn stream_session_activity_replays_the_persisted_snapshot() {
    // Given a session with two pre-seeded agent-activity rows.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-snapshot-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_core::agent_activity::append_agent_activity(
        &session_dir,
        &a_seeded_record("call-a", "Read"),
    )
    .unwrap();
    tddy_core::agent_activity::append_agent_activity(
        &session_dir,
        &a_seeded_record("call-b", "Bash"),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);

    // When a client subscribes to the activity stream.
    let mut stream = service
        .stream_session_activity(Request::new(StreamSessionActivityRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then the two snapshot records arrive in first-seen order.
    let first = next_record(&mut stream).await;
    let second = next_record(&mut stream).await;
    assert_eq!(first.call_id, "call-a");
    assert_eq!(first.tool_name, "Read");
    assert_eq!(second.call_id, "call-b");
    assert_eq!(second.tool_name, "Bash");
}

/// After the snapshot, a record published to the hub for the session is delivered live.
#[tokio::test]
async fn stream_session_activity_delivers_a_live_record_after_the_snapshot() {
    // Given a session with one pre-seeded row and a subscribed stream.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-live-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_core::agent_activity::append_agent_activity(
        &session_dir,
        &a_seeded_record("call-snapshot", "Read"),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();

    let mut stream = service
        .stream_session_activity(Request::new(StreamSessionActivityRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    // Drain the snapshot record so the next awaited item is the live one.
    let snapshot = next_record(&mut stream).await;
    assert_eq!(snapshot.call_id, "call-snapshot");

    // When a fresh record is published live for this session.
    hub.publish(session_id, a_seeded_record("call-live", "Grep"));

    // Then the subscriber receives it.
    let live = next_record(&mut stream).await;
    assert_eq!(live.call_id, "call-live");
    assert_eq!(live.tool_name, "Grep");
}

/// In LIVE_ONLY mode the persisted snapshot is not replayed: the first record delivered is one
/// published *after* the subscription, never a pre-seeded snapshot row.
#[tokio::test]
async fn stream_session_activity_in_live_only_mode_skips_the_snapshot_and_delivers_only_live_records(
) {
    // Given a session with a pre-seeded snapshot row.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-live-only-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_core::agent_activity::append_agent_activity(
        &session_dir,
        &a_seeded_record("call-snapshot", "Read"),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();

    // When a client subscribes in LIVE_ONLY mode.
    let mut stream = service
        .stream_session_activity(Request::new(StreamSessionActivityRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: tddy_service::proto::connection::StreamMode::LiveOnly as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    // and a fresh record is published live for the session.
    hub.publish(session_id, a_seeded_record("call-live", "Grep"));

    // Then the first record delivered is the live one — the snapshot was skipped entirely.
    let first = next_record(&mut stream).await;
    assert_eq!(
        first.call_id, "call-live",
        "live-only must not replay the persisted snapshot ('call-snapshot')"
    );
}

/// The hook sends `input_json` as a string; the persisted record carries it as structured JSON.
#[tokio::test]
async fn report_agent_activity_parses_the_hooks_json_input_into_a_structured_record() {
    // Given a registered claude-cli session.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-parse-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
    let service = make_unit_service(sessions_base);

    // When the hook reports a PreToolUse whose input is a JSON object string.
    service
        .report_agent_activity(Request::new(a_pre_tool_use(
            session_id,
            "Bash",
            r#"{"command":"cargo test"}"#,
        )))
        .await
        .unwrap();

    // Then the persisted record carries the parsed structured input.
    let records = read_agent_activity(&session_dir).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].input,
        serde_json::json!({ "command": "cargo test" })
    );
}

/// A non-JSON input string is preserved as a JSON string scalar (no data loss, no fabrication).
#[tokio::test]
async fn report_agent_activity_stores_a_non_json_input_string_as_a_string_scalar() {
    // Given a registered claude-cli session.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "activity-nonjson-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN);
    let service = make_unit_service(sessions_base);

    // When the hook reports input that is not valid JSON.
    service
        .report_agent_activity(Request::new(a_pre_tool_use(
            session_id,
            "Bash",
            "not valid json",
        )))
        .await
        .unwrap();

    // Then it is stored as a JSON string scalar rather than dropped.
    let records = read_agent_activity(&session_dir).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].input,
        serde_json::Value::String("not valid json".to_string())
    );
}

/// Await the next stream item with a bounded timeout so a missing record fails loudly instead
/// of hanging the test.
async fn next_record(stream: &mut super::MpscAgentActivityStream) -> ProtoAgentActivityRecord {
    tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("no agent-activity record arrived within the timeout")
        .expect("activity stream closed unexpectedly")
        .expect("activity stream yielded an error")
}

/// Await the next replay frame with a bounded timeout, decoding its inner ACP `AcpAgentMessage`.
async fn next_replay_frame(
    stream: &mut super::MpscAcpReplayStream,
) -> tddy_service::proto::acp::AcpAgentMessage {
    let envelope = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("no replay frame arrived within the timeout")
        .expect("replay stream closed unexpectedly")
        .expect("replay stream yielded an error");
    prost::Message::decode(&envelope.acp_agent_message[..]).expect("decode inner AcpAgentMessage")
}

/// The text of an `agent_message_chunk` ACP frame (panics on any other shape).
fn acp_agent_text(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
    use tddy_service::proto::acp::{acp_agent_message, content_block, session_update};
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => {
            match n.update.as_ref().and_then(|u| u.update.as_ref()) {
                Some(session_update::Update::AgentMessageChunk(c)) => {
                    match c.content.as_ref().and_then(|b| b.block.as_ref()) {
                        Some(content_block::Block::Text(t)) => t.text.clone(),
                        other => panic!("expected text content, got {other:?}"),
                    }
                }
                other => panic!("expected AgentMessageChunk, got {other:?}"),
            }
        }
        other => panic!("expected a SessionUpdate frame, got {other:?}"),
    }
}

/// The tool_call_id of a `tool_call` ACP frame (panics on any other shape).
fn acp_tool_call_id(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
    use tddy_service::proto::acp::{acp_agent_message, session_update};
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => {
            match n.update.as_ref().and_then(|u| u.update.clone()) {
                Some(session_update::Update::ToolCall(tc)) => {
                    tc.tool_call_id.map(|id| id.value).unwrap_or_default()
                }
                other => panic!("expected ToolCall, got {other:?}"),
            }
        }
        other => panic!("expected a SessionUpdate frame, got {other:?}"),
    }
}

/// StreamAcpReplay replays the persisted transcript snapshot, in write order, on subscribe.
#[tokio::test]
async fn stream_acp_replay_replays_the_persisted_snapshot() {
    // Given a session with a pre-seeded ACP transcript (an agent-text then a tool_call frame).
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-snapshot-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::agent_text_frame("Analyzing the parser.", 1_000),
    )
    .unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);

    // When a client subscribes to the ACP replay stream.
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then the two snapshot frames arrive in write order.
    let first = next_replay_frame(&mut stream).await;
    let second = next_replay_frame(&mut stream).await;
    assert_eq!(acp_agent_text(&first), "Analyzing the parser.");
    assert_eq!(acp_tool_call_id(&second), "call-a");
}

/// After the snapshot, a record published to the hub is delivered live as an ACP tool_call frame.
#[tokio::test]
async fn stream_acp_replay_delivers_a_live_frame_after_the_snapshot() {
    // Given a session with one pre-seeded transcript frame and a subscribed stream.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-live-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::agent_text_frame("Starting.", 1_000),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();

    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // Drain the snapshot frame so the next awaited item is the live one.
    let snapshot = next_replay_frame(&mut stream).await;
    assert_eq!(acp_agent_text(&snapshot), "Starting.");

    // When a fresh record is published live for this session.
    hub.publish(session_id, a_seeded_record("call-live", "Grep"));

    // Then the subscriber receives it as an ACP tool_call frame.
    let live = next_replay_frame(&mut stream).await;
    assert_eq!(acp_tool_call_id(&live), "call-live");
}

/// Pull one raw `AcpReplayFrame` envelope (the count-carrying wrapper), with a timeout so a
/// count-mode subscription that never broadcasts a count fails fast instead of hanging.
async fn next_replay_envelope(
    stream: &mut super::MpscAcpReplayStream,
) -> tddy_service::proto::connection::AcpReplayFrame {
    tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("no replay frame arrived within the timeout")
        .expect("replay stream closed unexpectedly")
        .expect("replay stream yielded an error")
}

/// COUNT_THEN_LIVE emits the current persisted-frame count first (no transcript payload), then a
/// fresh count each time a new activity is published — the cheap feed that drives the overlay's
/// icon/badge before the pane is opened.
#[tokio::test]
async fn stream_acp_replay_count_then_live_broadcasts_the_activity_count() {
    // Given a session whose persisted transcript already holds three frames.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-count-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
    )
    .unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
    )
    .unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-b", "Grep")),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();

    // When a client subscribes in count-first mode.
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then the first frame carries the current count (3) and no transcript payload.
    let first = next_replay_envelope(&mut stream).await;
    assert_eq!(first.activity_count, 3);
    assert!(
        first.acp_agent_message.is_empty(),
        "a count frame must not carry a transcript payload"
    );

    // And a newly-published activity raises the broadcast count to 4.
    hub.publish(session_id, a_seeded_record("call-live", "Bash"));
    let next = next_replay_envelope(&mut stream).await;
    assert_eq!(next.activity_count, 4);
}

/// A single tool call publishes two records (running then terminal) under one call_id; the count
/// must rise by one, not two — matching the single coalesced row the pane renders.
#[tokio::test]
async fn stream_acp_replay_count_then_live_counts_a_tool_call_once_across_its_two_records() {
    // Given a session whose transcript holds one agent-text frame (count baseline 1).
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-count-dedupe-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();

    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 1);

    // When a tool call publishes its running then terminal record under the same call_id.
    hub.publish(session_id, a_seeded_record("call-x", "Bash"));
    // Then the first (running) record lifts the count to 2.
    assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 2);
    // The terminal record for call-x emits nothing; a distinct call is the next frame — and it
    // reads 3, proving call-x was not counted twice.
    hub.publish(session_id, a_seeded_record("call-x", "Bash"));
    hub.publish(session_id, a_seeded_record("call-y", "Read"));
    assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 3);
}

// -----------------------------------------------------------------------
// Persisted-activity replay (bug fc990524: badge counts, pane opens empty)
//
// `acp-transcript.jsonl` is written by the tddy-coder presenter seam only. Every daemon-hosted
// (claude-cli / sandbox) session on disk therefore has a large `agent-activity.jsonl` and NO
// `acp-transcript.jsonl` — so an ACP replay that reads the transcript file alone serves an empty
// snapshot while its count feed keeps counting live records: the operator sees a badge, opens
// the pane, and finds nothing (and nothing at all after a page reload).
// -----------------------------------------------------------------------

/// The snapshot must project the session's durable `agent-activity.jsonl` rows, which are the
/// only persisted record of a daemon-hosted session's tool calls.
#[tokio::test]
async fn stream_acp_replay_replays_persisted_agent_activity_when_no_acp_transcript_exists() {
    // Given a session whose durable activity log holds two completed calls and which has no
    // `acp-transcript.jsonl` at all (the on-disk shape of every claude-cli session).
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-legacy-activity-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    append_agent_activity(&session_dir, &a_seeded_record("call-a", "Read")).unwrap();
    append_agent_activity(&session_dir, &a_seeded_record("call-b", "Grep")).unwrap();
    let service = make_unit_service(sessions_base);

    // When a client subscribes to the ACP replay snapshot.
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then both persisted calls are replayed as tool_call frames, in recorded order.
    assert_eq!(
        acp_tool_call_id(&next_replay_frame(&mut stream).await),
        "call-a"
    );
    assert_eq!(
        acp_tool_call_id(&next_replay_frame(&mut stream).await),
        "call-b"
    );
}

/// The badge must never promise entries the pane cannot deliver: the count baseline is taken from
/// the same resolved transcript the snapshot replays, so persisted activity counts even when no
/// `acp-transcript.jsonl` was ever written.
#[tokio::test]
async fn stream_acp_replay_count_then_live_counts_persisted_agent_activity_rows() {
    // Given a session whose durable activity log holds two completed calls and no ACP transcript.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-legacy-count-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    append_agent_activity(&session_dir, &a_seeded_record("call-a", "Read")).unwrap();
    append_agent_activity(&session_dir, &a_seeded_record("call-b", "Grep")).unwrap();
    let service = make_unit_service(sessions_base);

    // When a client subscribes in count-first mode.
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then the first count frame reports the two persisted calls.
    assert_eq!(next_replay_envelope(&mut stream).await.activity_count, 2);
}

/// The full `ToolCall` payload of a `tool_call` ACP frame (panics on any other shape).
fn acp_tool_call(
    frame: &tddy_service::proto::acp::AcpAgentMessage,
) -> tddy_service::proto::acp::ToolCall {
    use tddy_service::proto::acp::{acp_agent_message, session_update};
    match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => {
            match n.update.as_ref().and_then(|u| u.update.clone()) {
                Some(session_update::Update::ToolCall(tc)) => tc,
                other => panic!("expected ToolCall, got {other:?}"),
            }
        }
        other => panic!("expected a SessionUpdate frame, got {other:?}"),
    }
}

/// A `SNAPSHOT_THEN_LIVE` tool-call frame carries the call's metadata but not its bodies: the
/// heavy `raw_input`/`raw_output` are stripped so the stream's size tracks the number of tool
/// calls, not the volume of their I/O.
#[tokio::test]
async fn stream_acp_replay_snapshot_frames_omit_tool_bodies() {
    // Given a session whose persisted transcript holds a completed Read call with a full
    // raw_input and raw_output baked into the frame.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-strip-snapshot-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);

    // When a client subscribes to the snapshot replay.
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then the tool call arrives with its id and title intact but its bodies stripped.
    let tc = acp_tool_call(&next_replay_frame(&mut stream).await);
    assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-a");
    assert_eq!(tc.title, "Read");
    assert_eq!(tc.raw_input, None);
    assert_eq!(tc.raw_output, None);
}

/// The live tail is stripped too: a record published after subscribe (LIVE_ONLY) arrives as a
/// body-less tool-call frame, same as the snapshot.
#[tokio::test]
async fn stream_acp_replay_live_frames_omit_tool_bodies() {
    // Given a live subscription in LIVE_ONLY mode (no snapshot).
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-strip-live-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::LiveOnly as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // When a completed call is published live for this session.
    hub.publish(session_id, a_seeded_record("call-live", "Grep"));

    // Then the live tool-call frame carries its id but neither body.
    let tc = acp_tool_call(&next_replay_frame(&mut stream).await);
    assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-live");
    assert_eq!(tc.raw_input, None);
    assert_eq!(tc.raw_output, None);
}

/// The bodies the stream strips are fetched on demand: GetAcpToolCallDetail returns the exact
/// raw_input/raw_output the transcript recorded for one call.
#[tokio::test]
async fn get_acp_tool_call_detail_returns_the_full_tool_bodies() {
    // Given a session whose transcript holds a completed Read call.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-detail-full-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);

    // When the detail for that call is requested.
    let detail = service
        .get_acp_tool_call_detail(Request::new(GetAcpToolCallDetailRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            tool_call_id: "call-a".to_string(),
        }))
        .await
        .expect("detail lookup should succeed")
        .into_inner();

    // Then it returns the exact bodies the stream used to inline.
    let raw_input: serde_json::Value =
        serde_json::from_str(&detail.raw_input.expect("raw_input")).expect("raw_input is JSON");
    let raw_output: serde_json::Value =
        serde_json::from_str(&detail.raw_output.expect("raw_output")).expect("raw_output is JSON");
    assert_eq!(raw_input, serde_json::json!({ "path": "src/main.rs" }));
    assert_eq!(raw_output, serde_json::json!({ "content": "fn main() {}" }));
}

/// A tool_call_id absent from the transcript is a NOT_FOUND error, not an empty success — so the
/// caller can tell "no such call" from "call exists but has no output".
#[tokio::test]
async fn get_acp_tool_call_detail_is_not_found_for_an_unknown_tool_call_id() {
    // Given a session whose transcript holds only call-a.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-detail-missing-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);

    // When the detail for a non-existent call is requested.
    let status = service
        .get_acp_tool_call_detail(Request::new(GetAcpToolCallDetailRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            tool_call_id: "does-not-exist".to_string(),
        }))
        .await
        .expect_err("an unknown tool_call_id should be an error");

    // Then the status is NOT_FOUND.
    assert_eq!(status.code(), tddy_rpc::Code::NotFound);
}

// -----------------------------------------------------------------------
// Tail-first replay and the reverse cursor
// -----------------------------------------------------------------------

use tddy_service::proto::connection::GetAcpReplayPageRequest;

/// Seed a session dir with `entry_count` agent-text frames labelled `Entry 1` … `Entry N`
/// (1-based, naming the entry's position in the whole transcript) and return its dir.
fn a_session_recording(sessions_base: &std::path::Path, session_id: &str, entry_count: usize) {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    for n in 1..=entry_count {
        tddy_service::acp_replay::append_acp_frame(
            &session_dir,
            &tddy_service::acp_replay::agent_text_frame(&format!("Entry {n}"), 1_000 * n as i64),
        )
        .unwrap();
    }
}

/// Tail mode replays the newest page only — a long transcript no longer costs its whole history
/// to render a screenful of its end.
#[tokio::test]
async fn stream_acp_replay_in_tail_mode_replays_only_the_newest_page() {
    // Given a session whose persisted transcript holds five entries.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-tail-1";
    a_session_recording(&sessions_base, session_id, 5);
    let service = make_unit_service(sessions_base);

    // When a client subscribes tail-first for a page of two.
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::TailThenLive as i32,
            page_size: 2,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then only the newest two arrive, oldest-first *within* the page — entries 1-3 are not
    // replayed at all, and are reached by paging backwards instead.
    let first = next_replay_frame(&mut stream).await;
    let second = next_replay_frame(&mut stream).await;
    assert_eq!(
        (acp_agent_text(&first), acp_agent_text(&second)),
        ("Entry 4".to_string(), "Entry 5".to_string())
    );
}

/// Every tail frame carries its position in the **whole** transcript, not its index within the
/// page — the first frame's `seq` is what the client pages backwards from.
#[tokio::test]
async fn stream_acp_replay_tail_frames_carry_their_absolute_transcript_position() {
    // Given a session whose persisted transcript holds five entries.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-tail-seq-1";
    a_session_recording(&sessions_base, session_id, 5);
    let service = make_unit_service(sessions_base);

    // When the newest page of two is replayed.
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::TailThenLive as i32,
            page_size: 2,
        }))
        .await
        .unwrap()
        .into_inner();

    // Then the page's frames are stamped 3 and 4 — their 0-based positions in the transcript.
    let first = next_replay_envelope(&mut stream).await;
    let second = next_replay_envelope(&mut stream).await;
    assert_eq!((first.seq, second.seq), (3, 4));
}

/// The reverse cursor: one page of frames strictly older than `before_seq`, oldest-first.
#[tokio::test]
async fn get_acp_replay_page_serves_the_frames_before_the_cursor() {
    // Given a session whose persisted transcript holds five entries.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-page-1";
    a_session_recording(&sessions_base, session_id, 5);
    let service = make_unit_service(sessions_base);

    // When the page before seq 3 is requested, two frames wide.
    let page = service
        .get_acp_replay_page(Request::new(GetAcpReplayPageRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            before_seq: 3,
            page_size: 2,
        }))
        .await
        .expect("page lookup should succeed")
        .into_inner();

    // Then entries at seq 1 and 2 come back, oldest-first, with the head still further back.
    let texts: Vec<String> = page
        .frames
        .iter()
        .map(|bytes| {
            acp_agent_text(
                &prost::Message::decode(&bytes[..]).expect("decode paged AcpAgentMessage"),
            )
        })
        .collect();
    assert_eq!((page.first_seq, page.at_oldest), (1, false));
    assert_eq!(texts, vec!["Entry 2".to_string(), "Entry 3".to_string()]);
}

/// A paged frame is not a back door to the bodies: `GetAcpReplayPage` applies the same
/// `strip_tool_body` seam the replay stream does.
#[tokio::test]
async fn get_acp_replay_page_strips_tool_bodies_like_the_replay_stream_does() {
    // Given a session whose transcript holds a completed Read call with full bodies, followed
    // by one entry, so the call sits strictly before the cursor.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-page-strip-1";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::frame_for_agent_activity(&a_seeded_record("call-a", "Read")),
    )
    .unwrap();
    let service = make_unit_service(sessions_base);

    // When that call is paged back rather than streamed.
    let page = service
        .get_acp_replay_page(Request::new(GetAcpReplayPageRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            before_seq: 1,
            page_size: 10,
        }))
        .await
        .expect("page lookup should succeed")
        .into_inner();

    // Then it arrives with its id and title intact but its bodies stripped.
    let frame: tddy_service::proto::acp::AcpAgentMessage =
        prost::Message::decode(&page.frames[0][..]).expect("decode paged AcpAgentMessage");
    let tc = acp_tool_call(&frame);
    assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-a");
    assert_eq!(
        (tc.title, tc.raw_input, tc.raw_output),
        ("Read".to_string(), None, None)
    );
}

/// The `running` record a tool call publishes when it starts: the same call as
/// [`a_seeded_record`], before its result is known.
fn a_running_record(call_id: &str, tool_name: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        status: STATUS_RUNNING.to_string(),
        result: serde_json::Value::Null,
        completed_unix_ms: 0,
        ..a_seeded_record(call_id, tool_name)
    }
}

/// A tool call's terminal record refines the entry its `running` record created, so it carries
/// that entry's position — the two records coalesce into one transcript row, and a live reader
/// must be able to replace the row it already placed rather than append a second one.
#[tokio::test]
async fn stream_acp_replay_gives_a_tool_calls_terminal_record_the_position_of_its_running_record() {
    // Given a session whose transcript holds two entries, streamed tail-first.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-live-refine-1";
    a_session_recording(&sessions_base, session_id, 2);
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::TailThenLive as i32,
            page_size: 2,
        }))
        .await
        .unwrap()
        .into_inner();
    next_replay_envelope(&mut stream).await;
    next_replay_envelope(&mut stream).await;

    // When a tool call publishes its running record and then its terminal one.
    hub.publish(session_id, a_running_record("call-x", "Bash"));
    hub.publish(session_id, a_seeded_record("call-x", "Bash"));

    // Then both land on position 2 — the position a re-read of the transcript would give the
    // single row they coalesce into.
    let running = next_replay_envelope(&mut stream).await;
    let terminal = next_replay_envelope(&mut stream).await;
    assert_eq!((running.seq, terminal.seq), (2, 2));
}

/// A refinement must not cost the transcript a position: the next distinct call takes the very
/// next one, so live numbering neither drifts ahead of nor lags behind a later re-read.
#[tokio::test]
async fn stream_acp_replay_gives_the_call_after_a_refinement_the_next_position() {
    // Given a session whose transcript holds two entries, streamed tail-first.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-live-refine-2";
    a_session_recording(&sessions_base, session_id, 2);
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::TailThenLive as i32,
            page_size: 2,
        }))
        .await
        .unwrap()
        .into_inner();
    next_replay_envelope(&mut stream).await;
    next_replay_envelope(&mut stream).await;

    // When one call runs to completion and a second, distinct call then starts.
    hub.publish(session_id, a_running_record("call-x", "Bash"));
    hub.publish(session_id, a_seeded_record("call-x", "Bash"));
    hub.publish(session_id, a_running_record("call-y", "Read"));

    // Then the new call is at 3: the refinement of call-x consumed no position of its own.
    next_replay_envelope(&mut stream).await;
    next_replay_envelope(&mut stream).await;
    let new_call = next_replay_envelope(&mut stream).await;
    assert_eq!(new_call.seq, 3);
}

/// `LIVE_ONLY` replays none of the transcript but still numbers against it: a live frame's
/// position is where that entry sits in the whole recording, not where it sits in the handful of
/// frames this subscription happened to see. Reading the transcript purely to establish that base
/// is the only reason the mode touches the disk at all, so this is what pays for it.
#[tokio::test]
async fn stream_acp_replay_live_only_frames_are_numbered_from_the_recorded_transcript() {
    // Given a session with three recorded entries, subscribed live-only (nothing replayed).
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-live-only-seq-1";
    a_session_recording(&sessions_base, session_id, 3);
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::LiveOnly as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();

    // When a new call is published live.
    hub.publish(session_id, a_seeded_record("call-live", "Grep"));

    // Then it is at position 3 — after the three recorded entries — rather than claiming
    // position 0 as it would if the mode numbered from its own first delivery.
    assert_eq!(next_replay_envelope(&mut stream).await.seq, 3);
}

/// A call that straddles the subscribe boundary — its `running` record already in the snapshot,
/// its terminal record arriving live — refines the snapshot's row, so it carries the position
/// the snapshot gave that row rather than a fresh one at the tail.
#[tokio::test]
async fn stream_acp_replay_gives_a_live_terminal_record_the_position_its_call_holds_in_the_snapshot(
) {
    // Given a session whose snapshot is one agent-text entry followed by a still-running call.
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "acp-replay-live-refine-3";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_service::acp_replay::append_acp_frame(
        &session_dir,
        &tddy_service::acp_replay::agent_text_frame("Reading the parser.", 1_000),
    )
    .unwrap();
    append_agent_activity(&session_dir, &a_running_record("call-a", "Read")).unwrap();
    let service = make_unit_service(sessions_base);
    let hub = service.agent_activity_hub();
    let mut stream = service
        .stream_acp_replay(Request::new(StreamAcpReplayRequest {
            session_token: "valid".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
            page_size: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    next_replay_envelope(&mut stream).await;
    let snapshot_call = next_replay_envelope(&mut stream).await;
    assert_eq!(snapshot_call.seq, 1);

    // When that same call reports its terminal record live.
    hub.publish(session_id, a_seeded_record("call-a", "Read"));

    // Then it arrives at position 1 — replacing the snapshot's row, not appended after it.
    let terminal = next_replay_envelope(&mut stream).await;
    assert_eq!(terminal.seq, 1);
}
