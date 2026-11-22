//! Acceptance: an orchestrator streams its session watches through `WaitSessionWatchEvents` — the
//! call `tddy-tools monitor` loops on. While a stream holds a watch its events go to the stream
//! only, never to tool answers or a wake; a batch is handed until it is acknowledged; a resumed
//! monitor takes the watch over from the one before it; and once the
//! stream stops calling, its lease lapses and what it did not acknowledge returns to both
//! (`#activity-wake` 4/5).
//!
//! The host keeps a project notification log, session watches over it and the wake of
//! `#activity-wake` 3/3, with its timings and the stream lease shortened through their own
//! constructors. The orchestrator's agent is stood in for by a recording shell on its main
//! terminal, as in the wake's suite, so a test reads exactly what a wake pasted. A child's events
//! are published on the host's bus as the daemon's publish sites publish them; the orchestrator's
//! calls are the real exec-tool calls its tddy-tools makes.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use support::{
    a_host_streaming_watches, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::session_notification::QuestionNotice;
use tddy_daemon_rpc::exec_tool::watch_wake::WakeTimings;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const WAIT_SESSION_WATCH_EVENTS: &str = "WaitSessionWatchEvents";
const CREATE_SESSION_WATCH: &str = "CreateSessionWatch";
const CANCEL_SESSION_WATCH: &str = "CancelSessionWatch";
/// Any tool an orchestrator calls: its answer is where unstreamed watch events ride.
const ANY_TOOL: &str = "ListProjectSessions";

const TIMINGS: WakeTimings = WakeTimings {
    settle: Duration::from_millis(50),
    min_gap: Duration::from_millis(200),
};
/// A lease no test outlives: the stream holds its watch for the whole test.
const A_HELD_LEASE: Duration = Duration::from_secs(15);
/// A lease a test waits out.
const A_SHORT_LEASE: Duration = Duration::from_millis(500);
/// Longer than [`A_SHORT_LEASE`]: a stream that stopped calling this long ago holds nothing.
const PAST_THE_SHORT_LEASE: Duration = Duration::from_millis(900);
/// Several settles long: a wake that was going to happen has happened by then.
const QUIET_PERIOD: Duration = Duration::from_millis(400);
/// How long a typed byte is given to reach the stand-in agent.
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const READY_MARKER: &str = "READY";
/// What every wake starts with: a bracketed paste of the `[tddy watch]` message.
const WAKE_OPENS: &[u8] = b"\x1b[200~[tddy watch]";

struct World {
    a: Host,
    bus: Arc<SessionNotificationBus>,
    /// The project's main session: a claude-cli orchestrator with a recording stand-in agent.
    orchestrator: String,
    child: String,
    agent: RecordingAgent,
    os_user: String,
}

/// One host whose orchestrator watches its child, with streams leased for `lease`.
async fn a_world_leasing_streams_for(lease: Duration) -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_streaming_watches(HOST_A, &peers, TIMINGS, lease);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    records_a_running_claude_session(&a, &orchestrator);
    let agent = RecordingAgent::on_the_main_terminal_of(&a, &orchestrator).await;
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    World {
        a,
        bus,
        orchestrator,
        child,
        agent,
        os_user,
    }
}

fn session_dir(host: &Host, session_id: &str) -> PathBuf {
    unified_session_dir_path(host.sessions.path(), session_id)
}

fn records_a_running_claude_session(host: &Host, session_id: &str) {
    let dir = session_dir(host, session_id);
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_type = Some("claude-cli".to_string());
    meta.pid = Some(std::process::id());
    meta.activity_status = Some("Running".to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
}

// ── what the sessions do ─────────────────────────────────────────────────────────────────────

impl World {
    fn a_notification_of(&self, session_id: &str) -> SessionNotification {
        SessionNotification {
            session_id: session_id.to_string(),
            os_user: self.os_user.clone(),
            label: "fix-login".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::ActivityStatus,
            text: String::new(),
            agent: None,
            question: None,
            ended: None,
            activity_status: None,
            at_unix_ms: 1_790_000_000_000,
            message: None,
        }
    }

    /// The child asks permission to run `cargo test`; answered by `request_id`.
    async fn the_child_asks_permission(&self, request_id: &str) {
        self.bus
            .publish(SessionNotification {
                source: SessionNotificationSource::PendingQuestion,
                text: "🔔 Session fix-login wants to run Bash: cargo test".to_string(),
                question: Some(QuestionNotice {
                    request_id: request_id.to_string(),
                    kind: "permission".to_string(),
                    tool_name: "Bash".to_string(),
                    summary: "cargo test".to_string(),
                    questions: Vec::new(),
                }),
                ..self.a_notification_of(&self.child)
            })
            .await;
    }

    /// The orchestrator's hooks report `status`: recorded in its `.session.yaml`, and published.
    async fn the_orchestrator_reports(&self, status: &str) {
        tddy_core::update_activity_status(&session_dir(&self.a, &self.orchestrator), status)
            .expect("the status is recorded");
        self.bus
            .publish(SessionNotification {
                text: format!("Session main: {status}"),
                activity_status: Some(status.to_string()),
                ..self.a_notification_of(&self.orchestrator)
            })
            .await;
    }

    // ── what the orchestrator calls ──────────────────────────────────────────────────────────

    async fn call_as(&self, caller: &str, tool: &str, args: Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(an_exec_tool_call(caller, tool, args)))
            .await
            .expect("the exec-tool call is served")
            .into_inner()
    }

    async fn calls(&self, tool: &str, args: Value) -> ExecuteToolResponse {
        self.call_as(&self.orchestrator, tool, args).await
    }

    /// A watch `caller` created over the child's permission prompts, for an hour, opting into the
    /// wake: what a stream holds must reach neither the wake nor a tool answer.
    async fn a_watch_of_the_childs_prompts_by(&self, caller: &str) -> String {
        let answer = self
            .call_as(
                caller,
                CREATE_SESSION_WATCH,
                json!({
                    "session_ids": [self.child],
                    "kinds": ["permission"],
                    "ttl_seconds": 3_600,
                    "wake_when_idle": true
                }),
            )
            .await;
        assert!(
            !answer.is_error,
            "the watch was refused: {}",
            answer.error_message
        );
        result_of(&answer)["watch_id"]
            .as_str()
            .expect("the watch has an id")
            .to_string()
    }

    async fn a_watch_of_the_childs_prompts(&self) -> String {
        self.a_watch_of_the_childs_prompts_by(&self.orchestrator)
            .await
    }

    /// One `WaitSessionWatchEvents` call over `watch_id`, answering at once when nothing waits.
    async fn streams(&self, watch_id: &str, stream: &StreamPosition) -> ExecuteToolResponse {
        self.calls(WAIT_SESSION_WATCH_EVENTS, stream.args_for(watch_id, 0))
            .await
    }

    /// A stream's call that must be answered: its batch.
    async fn streamed_batch(&self, watch_id: &str, stream: &StreamPosition) -> Value {
        let answer = self.streams(watch_id, stream).await;
        assert!(
            !answer.is_error,
            "the stream was refused: {}",
            answer.error_message
        );
        result_of(&answer)
    }

    /// The first call of a new stream over `watch_id`, which must be served: where it stands.
    async fn starts_a_stream(&self, watch_id: &str) -> StreamPosition {
        StreamPosition::after(&self.streamed_batch(watch_id, &StreamPosition::new()).await)
    }

    /// The `watchEvents` of the orchestrator's next answer to any tool, `Null` when it has none.
    async fn next_answers_watch_events(&self) -> Value {
        let answer = self.calls(ANY_TOOL, json!({})).await;
        assert!(
            !answer.is_error,
            "{ANY_TOOL} was refused: {}",
            answer.error_message
        );
        result_of(&answer)["watchEvents"].clone()
    }
}

fn an_exec_tool_call(caller: &str, tool: &str, args: Value) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: caller.to_string(),
        tool_name: tool.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: String::new(),
        conversation_id: String::new(),
    }
}

fn result_of(answer: &ExecuteToolResponse) -> Value {
    serde_json::from_str(&answer.result_json).expect("the answer is JSON")
}

/// Where a stream stands: its id, and what it acknowledges on its next call.
#[derive(Debug, Clone, Default)]
struct StreamPosition {
    stream_id: Option<String>,
    ack_seq: Option<u64>,
}

impl StreamPosition {
    /// A stream not started yet.
    fn new() -> Self {
        Self::default()
    }

    /// The stream that was answered `batch`, acknowledging nothing of it yet.
    fn after(batch: &Value) -> Self {
        Self {
            stream_id: batch["stream_id"].as_str().map(str::to_string),
            ack_seq: None,
        }
    }

    /// This stream, acknowledging `batch` — as a monitor does once it has printed it.
    fn acknowledging(&self, batch: &Value) -> Self {
        Self {
            stream_id: self.stream_id.clone(),
            ack_seq: batch["next_ack_seq"].as_u64(),
        }
    }

    fn args_for(&self, watch_id: &str, timeout_ms: u64) -> Value {
        let mut args = json!({ "watch_ids": [watch_id], "timeout_ms": timeout_ms });
        if let Some(stream_id) = &self.stream_id {
            args["stream_id"] = json!(stream_id);
        }
        if let Some(ack_seq) = self.ack_seq {
            args["ack_seq"] = json!(ack_seq);
        }
        args
    }
}

/// The `request_id` of each event of a stream's batch, oldest first.
fn streamed_request_ids(batch: &Value) -> Vec<String> {
    batch["events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .map(|event| event["request_id"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The `request_id` of each question among the events on a tool answer.
fn answered_request_ids(watch_events: &Value) -> Vec<String> {
    watch_events["events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .map(|event| {
                    event["question"]["request_id"]
                        .as_str()
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── the stand-in agent ───────────────────────────────────────────────────────────────────────

/// A stand-in agent on a session's main terminal, and the file holding every byte typed into it.
struct RecordingAgent {
    _dir: tempfile::TempDir,
    record: PathBuf,
}

impl RecordingAgent {
    async fn on_the_main_terminal_of(host: &Host, session_id: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("typed.bin");
        let script = dir.path().join("stand_in_agent.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nstty raw -echo\nprintf '\\033[?2004h{READY_MARKER}\\r\\n'\nexec cat >> '{}'\n",
                record.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let handle = host
            .terminals
            .start_with_options(
                session_id,
                host.worktree_of(session_id),
                "claude-opus-4-8",
                vec![script.to_str().unwrap().to_string()],
                "claude-cli",
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
            )
            .await
            .expect("the main terminal starts");
        a_capture_showing(&handle.capture, READY_MARKER, PTY_STUB_OUTPUT).await;
        Self { _dir: dir, record }
    }

    fn typed(&self) -> String {
        String::from_utf8_lossy(&std::fs::read(&self.record).unwrap_or_default()).into_owned()
    }

    fn wakes(&self) -> usize {
        count_of(&std::fs::read(&self.record).unwrap_or_default(), WAKE_OPENS)
    }

    /// Wait until `n` wakes have been pasted and submitted; what was typed.
    async fn after_wakes(&self, n: usize) -> String {
        wait_for(&self.record, |typed| {
            count_of(typed, WAKE_OPENS) >= n && typed.ends_with(b"\r")
        })
        .await;
        self.typed()
    }

    /// The number of wakes once [`QUIET_PERIOD`] has passed: for asserting none came.
    async fn wakes_after_a_quiet_period(&self) -> usize {
        tokio::time::sleep(QUIET_PERIOD).await;
        self.wakes()
    }
}

fn count_of(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

async fn wait_for(record: &Path, done: impl Fn(&[u8]) -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let typed = std::fs::read(record).unwrap_or_default();
        if done(&typed) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the agent was never woken; typed so far: {:?}",
            String::from_utf8_lossy(&typed)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ── the stream takes a watch's events ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_streamed_watch_hands_its_events_to_the_stream_and_not_to_tool_answers() {
    // Given an orchestrator streaming its watch of the child's prompts
    let world = a_world_leasing_streams_for(A_HELD_LEASE).await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let stream = world.starts_a_stream(&watch_id).await;

    // When the child asks permission, and the orchestrator calls another tool before streaming on
    world.the_child_asks_permission("req-1").await;
    let on_the_answer = world.next_answers_watch_events().await;
    let streamed = world.streamed_batch(&watch_id, &stream).await;

    // Then the answer carries nothing, and the stream is handed the prompt with its request id
    assert_eq!(
        (on_the_answer, streamed_request_ids(&streamed)),
        (Value::Null, vec!["req-1".to_string()])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_streamed_watch_does_not_wake_its_idle_orchestrator() {
    // Given an idle orchestrator streaming its watch of the child's prompts
    let world = a_world_leasing_streams_for(A_HELD_LEASE).await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    world.the_orchestrator_reports("Done").await;
    let stream = world.starts_a_stream(&watch_id).await;

    // When the child asks permission
    world.the_child_asks_permission("req-1").await;

    // Then nothing is pasted, and the prompt waits for the stream
    assert_eq!(world.agent.wakes_after_a_quiet_period().await, 0);
    let streamed = world.streamed_batch(&watch_id, &stream).await;
    assert_eq!(streamed_request_ids(&streamed), vec!["req-1".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_after_the_last_acknowledgement_return_to_answers_and_the_wake_once_the_lease_lapses(
) {
    // Given an idle orchestrator whose stream printed and acknowledged the child's first prompt
    let world = a_world_leasing_streams_for(A_SHORT_LEASE).await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    world.the_orchestrator_reports("Done").await;
    let stream = world.starts_a_stream(&watch_id).await;
    world.the_child_asks_permission("req-1").await;
    let printed = world.streamed_batch(&watch_id, &stream).await;
    world
        .streamed_batch(&watch_id, &stream.acknowledging(&printed))
        .await;

    // When the child asks again, the stream stops calling, and its lease lapses
    world.the_child_asks_permission("req-2").await;
    tokio::time::sleep(PAST_THE_SHORT_LEASE).await;

    // Then the unacknowledged prompt rides on the next answer, the acknowledged one does not —
    let on_the_answer = world.next_answers_watch_events().await;
    assert_eq!(
        answered_request_ids(&on_the_answer),
        vec!["req-2".to_string()]
    );

    // — and the child's next prompt wakes the orchestrator again, naming only itself
    world.the_child_asks_permission("req-3").await;
    let typed = world.agent.after_wakes(1).await;
    assert_eq!(
        ["req-1", "req-2", "req-3"].map(|id| typed.contains(id)),
        [false, false, true],
        "typed: {typed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unacknowledged_batch_is_handed_again() {
    // Given a stream that was handed the child's prompt and stopped before acknowledging it
    let world = a_world_leasing_streams_for(A_SHORT_LEASE).await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let stream = world.starts_a_stream(&watch_id).await;
    world.the_child_asks_permission("req-1").await;
    let lost_with_the_process = world.streamed_batch(&watch_id, &stream).await;
    assert_eq!(
        streamed_request_ids(&lost_with_the_process),
        vec!["req-1".to_string()]
    );

    // When a new stream attaches to the watch once the old one's lease has lapsed
    tokio::time::sleep(PAST_THE_SHORT_LEASE).await;
    let reattached = world
        .streamed_batch(&watch_id, &StreamPosition::new())
        .await;

    // Then it is handed the same prompt, as a stream of its own
    assert_eq!(streamed_request_ids(&reattached), vec!["req-1".to_string()]);
    assert_ne!(reattached["stream_id"], lost_with_the_process["stream_id"]);
}

// ── refusals ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_stream_on_a_streamed_watch_takes_it_over_and_the_old_stream_is_refused() {
    // Given a stream that was handed the child's prompt and has not acknowledged it
    let world = a_world_leasing_streams_for(A_HELD_LEASE).await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let old = world.starts_a_stream(&watch_id).await;
    world.the_child_asks_permission("req-1").await;
    let handed_to_the_old = world.streamed_batch(&watch_id, &old).await;

    // When the orchestrator resumes its monitor — a new stream on the watch, inside the old lease
    let resumed = world
        .streamed_batch(&watch_id, &StreamPosition::new())
        .await;
    let old_again = world.streams(&watch_id, &old).await;

    // Then the new stream holds the watch under its own id and is handed the unacknowledged
    // prompt again —
    assert_ne!(resumed["stream_id"], handed_to_the_old["stream_id"]);
    assert_eq!(streamed_request_ids(&resumed), vec!["req-1".to_string()]);

    // — and the old stream's next call is refused, naming the watch it lost
    assert!(
        old_again.is_error,
        "the superseded stream was served: {}",
        old_again.result_json
    );
    assert!(
        old_again.error_message.contains(&watch_id)
            && old_again.error_message.contains("another monitor"),
        "the refusal {:?} does not say another monitor took {watch_id} over",
        old_again.error_message
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streaming_a_watch_that_is_not_the_callers_or_has_ended_is_refused() {
    // Given a watch of another orchestrator of the project, and a watch of this one it cancelled
    let world = a_world_leasing_streams_for(A_HELD_LEASE).await;
    let other_orchestrator = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    world.a.make_project_home(&other_orchestrator);
    records_a_running_claude_session(&world.a, &other_orchestrator);
    let anothers = world
        .a_watch_of_the_childs_prompts_by(&other_orchestrator)
        .await;
    let cancelled = world.a_watch_of_the_childs_prompts().await;
    let cancel = world
        .calls(CANCEL_SESSION_WATCH, json!({ "watch_id": cancelled }))
        .await;
    assert!(!cancel.is_error, "{}", cancel.error_message);

    for (case, watch_id) in [("another's", &anothers), ("ended", &cancelled)] {
        // When the orchestrator streams it
        let refused = world.streams(watch_id, &StreamPosition::new()).await;

        // Then it is refused as RenewSessionWatch refuses it, naming the watch
        assert!(
            refused.is_error,
            "{case}: the stream was served: {}",
            refused.result_json
        );
        assert!(
            refused.error_message.contains(watch_id.as_str())
                && refused.error_message.contains("ListSessionWatches"),
            "{case}: the refusal {:?} does not name the watch and where to find yours",
            refused.error_message
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_that_is_not_an_orchestrator_is_refused_the_stream_tool() {
    // Given an orchestrator's watch, and an ordinary session of the project
    let world = a_world_leasing_streams_for(A_HELD_LEASE).await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let args = StreamPosition::new().args_for(&watch_id, 0);

    // When the ordinary session streams it
    let refused = world
        .a
        .daemon
        .execute_tool(Request::direct(an_exec_tool_call(
            &world.child,
            WAIT_SESSION_WATCH_EVENTS,
            args.clone(),
        )))
        .await
        .expect_err("an ordinary session's stream was served");

    // Then it is denied as a project-home tool is to any other session, while the orchestrator's
    // same call is served
    assert_eq!(refused.code(), tddy_rpc::Code::PermissionDenied);
    let served = world.calls(WAIT_SESSION_WATCH_EVENTS, args).await;
    assert!(
        !served.is_error,
        "the orchestrator's stream was refused: {}",
        served.error_message
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_wait_answers_empty_at_its_timeout() {
    // Given a stream over a watch nothing happens to
    let world = a_world_leasing_streams_for(A_HELD_LEASE).await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let stream = world.starts_a_stream(&watch_id).await;

    // When it waits 300 ms
    let started = tokio::time::Instant::now();
    let answer = world
        .calls(WAIT_SESSION_WATCH_EVENTS, stream.args_for(&watch_id, 300))
        .await;
    let held = started.elapsed();

    // Then the call is held for its timeout and answers an empty batch
    assert!(
        !answer.is_error,
        "the wait was refused: {}",
        answer.error_message
    );
    let batch = result_of(&answer);
    assert_eq!(
        (batch["events"].clone(), batch["ended"].clone()),
        (json!([]), json!([]))
    );
    assert!(
        held >= Duration::from_millis(300),
        "the wait answered after {held:?}, before its 300 ms timeout"
    );
}
