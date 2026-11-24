//! Acceptance: a watch wakes its idle orchestrator only when it opts in (`wake_when_idle`), and
//! `WaitSessionWatchEvents` waits in a window — it returns once it holds `max_events` events, or
//! a `settle_ms` after its first, or when its `timeout_ms` elapses, whichever comes first — and
//! may pick by `kinds` and `session_ids`, leaving what it did not pick pending.
//!
//! The host keeps a project notification log, session watches over it and the wake, with the
//! wake's timings shortened. The orchestrator's agent is stood in for by a recording shell on its
//! main terminal, so a test reads exactly what a wake pasted. The children's events are published
//! on the host's bus as the daemon's publish sites publish them; the orchestrator's calls are the
//! real exec-tool calls its tddy-tools makes.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

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
/// Any tool an orchestrator calls: its answer is where unstreamed watch events ride.
const ANY_TOOL: &str = "ListProjectSessions";

const TIMINGS: WakeTimings = WakeTimings {
    settle: Duration::from_millis(50),
    min_gap: Duration::from_millis(200),
};
/// A lease no test outlives.
const A_HELD_LEASE: Duration = Duration::from_secs(15);
/// A lease a test waits out.
const A_SHORT_LEASE: Duration = Duration::from_millis(500);
/// Longer than [`A_SHORT_LEASE`]: a stream that stopped calling this long ago holds nothing.
const PAST_THE_SHORT_LEASE: Duration = Duration::from_millis(900);
/// Several settles long: a wake that was going to happen has happened by then.
const QUIET_PERIOD: Duration = Duration::from_millis(600);
/// How long a typed byte is given to reach the stand-in agent.
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// A window no test waits out: a call held this long answered for another reason.
const A_LONG_WINDOW_MS: u64 = 20_000;
/// Comfortably under [`A_LONG_WINDOW_MS`]: a call that answered within this did not wait it out.
const WELL_INSIDE_THE_WINDOW: Duration = Duration::from_secs(5);
const READY_MARKER: &str = "READY";
/// What every wake starts with: a bracketed paste of the `[tddy watch]` message.
const WAKE_OPENS: &[u8] = b"\x1b[200~[tddy watch]";

struct World {
    a: Host,
    bus: Arc<SessionNotificationBus>,
    /// The project's main session: a claude-cli orchestrator with a recording stand-in agent.
    orchestrator: String,
    child: String,
    sibling: String,
    agent: RecordingAgent,
    os_user: String,
}

/// One host whose orchestrator may watch two children, with streams leased for `lease`.
async fn a_world_leasing_streams_for(lease: Duration) -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_streaming_watches(HOST_A, &peers, TIMINGS, lease);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    records_a_running_claude_session(&a, &orchestrator);
    let agent = RecordingAgent::on_the_main_terminal_of(&a, &orchestrator).await;
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let sibling = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    World {
        a,
        bus,
        orchestrator,
        child,
        sibling,
        agent,
        os_user,
    }
}

async fn a_world() -> World {
    a_world_leasing_streams_for(A_HELD_LEASE).await
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
            label: if session_id == self.child {
                "fix-login"
            } else {
                "add-docs"
            }
            .to_string(),
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

    /// `session_id` asks permission to run `cargo test`; answered by `request_id`.
    async fn asks_permission(&self, session_id: &str, request_id: &str) {
        self.bus
            .publish(SessionNotification {
                source: SessionNotificationSource::PendingQuestion,
                text: "🔔 wants to run Bash: cargo test".to_string(),
                question: Some(QuestionNotice {
                    request_id: request_id.to_string(),
                    kind: "permission".to_string(),
                    tool_name: "Bash".to_string(),
                    summary: "cargo test".to_string(),
                    questions: Vec::new(),
                }),
                ..self.a_notification_of(session_id)
            })
            .await;
    }

    async fn the_child_asks_permission(&self, request_id: &str) {
        self.asks_permission(&self.child, request_id).await;
    }

    /// `session_id` finishes its turn.
    async fn finishes_its_turn(&self, session_id: &str) {
        self.bus
            .publish(SessionNotification {
                text: "Done".to_string(),
                activity_status: Some("Done".to_string()),
                ..self.a_notification_of(session_id)
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

    async fn calls(&self, tool: &str, args: Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.orchestrator.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .expect("the exec-tool call is served")
            .into_inner()
    }

    /// A watch created with `args` for an hour: its id.
    async fn a_watch(&self, mut args: Value) -> String {
        args["ttl_seconds"] = json!(3_600);
        let answer = self.calls(CREATE_SESSION_WATCH, args).await;
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

    /// A watch over the child's permission prompts, created with no word on the wake.
    async fn a_watch_of_the_childs_prompts(&self) -> String {
        self.a_watch(json!({ "session_ids": [self.child], "kinds": ["permission"] }))
            .await
    }

    /// One `WaitSessionWatchEvents` call over `watch_id` with `args` besides it: the batch, and
    /// how long the call was held.
    async fn waits(&self, watch_id: &str, mut args: Value) -> (Value, Duration) {
        args["watch_ids"] = json!([watch_id]);
        let started = tokio::time::Instant::now();
        let answer = self.calls(WAIT_SESSION_WATCH_EVENTS, args).await;
        let held = started.elapsed();
        assert!(
            !answer.is_error,
            "the wait was refused: {}",
            answer.error_message
        );
        (result_of(&answer), held)
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

fn result_of(answer: &ExecuteToolResponse) -> Value {
    serde_json::from_str(&answer.result_json).expect("the answer is JSON")
}

/// Each event of a batch: its `request_id`, or its session's label and kind when it has none.
fn described(batch: &Value) -> Vec<String> {
    batch["events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .map(|event| match event["request_id"].as_str() {
                    Some(request_id) => request_id.to_string(),
                    None => format!(
                        "{}:{}",
                        event["session"].as_str().unwrap_or(""),
                        event["kind"].as_str().unwrap_or("")
                    ),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
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
                "#!/bin/sh\nstty raw echo -echoctl\nprintf '\\033[?2004h{READY_MARKER}\\r\\n'\nexec cat >> '{}'\n",
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

// ── the wake is opt-in ───────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watch_that_does_not_opt_in_never_wakes_its_idle_orchestrator() {
    // Given an idle orchestrator watching the child's prompts with no word on the wake
    let world = a_world().await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    world.the_orchestrator_reports("Done").await;

    // When the child asks permission
    world.the_child_asks_permission("req-1").await;

    // Then nothing is pasted into the orchestrator's terminal, and the prompt waits for it to
    // collect
    assert_eq!(
        world.agent.wakes_after_a_quiet_period().await,
        0,
        "typed: {:?}",
        world.agent.typed()
    );
    let (collected, _) = world.waits(&watch_id, json!({ "timeout_ms": 0 })).await;
    assert_eq!(described(&collected), strings(&["req-1"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watch_that_opts_in_still_wakes_its_idle_orchestrator() {
    // Given an idle orchestrator watching the child's prompts with the wake on
    let world = a_world().await;
    world
        .a_watch(json!({
            "session_ids": [world.child],
            "kinds": ["permission"],
            "wake_when_idle": true
        }))
        .await;
    world.the_orchestrator_reports("Done").await;

    // When the child asks permission
    world.the_child_asks_permission("req-1").await;

    // Then one turn is pasted, naming the request
    let typed = world.agent.after_wakes(1).await;
    assert!(typed.contains("req-1"), "typed: {typed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renewing_a_watch_with_the_wake_on_starts_waking() {
    // Given an idle orchestrator whose quiet watch it renews turning the wake on
    let world = a_world().await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let renewed = world
        .calls(
            "RenewSessionWatch",
            json!({ "watch_id": watch_id, "ttl_seconds": 3_600, "wake_when_idle": true }),
        )
        .await;
    assert!(!renewed.is_error, "{}", renewed.error_message);
    world.the_orchestrator_reports("Done").await;

    // When the child asks permission
    world.the_child_asks_permission("req-1").await;

    // Then the renewal said the watch wakes, and the orchestrator is woken
    assert_eq!(result_of(&renewed)["wake_when_idle"], json!(true));
    let typed = world.agent.after_wakes(1).await;
    assert!(typed.contains("req-1"), "typed: {typed:?}");
}

// ── the window ───────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_naming_no_count_returns_at_its_first_event() {
    // Given a wait held over the child's prompts with a long window and no count
    let world = Arc::new(a_world().await);
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let waiting = {
        let world = Arc::clone(&world);
        let watch_id = watch_id.clone();
        tokio::spawn(async move {
            world
                .waits(&watch_id, json!({ "timeout_ms": A_LONG_WINDOW_MS }))
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;

    // When the child asks twice
    world.the_child_asks_permission("req-1").await;
    world.the_child_asks_permission("req-2").await;

    // Then the wait answers well inside its window, carrying the first prompt alone
    let (batch, held) = waiting.await.expect("the wait finished");
    assert!(held < WELL_INSIDE_THE_WINDOW, "the wait was held {held:?}");
    assert_eq!(described(&batch), strings(&["req-1"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_returns_once_it_holds_max_events() {
    // Given a wait for three of the child's prompts, held with a long window
    let world = Arc::new(a_world().await);
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let waiting = {
        let world = Arc::clone(&world);
        let watch_id = watch_id.clone();
        tokio::spawn(async move {
            world
                .waits(
                    &watch_id,
                    json!({ "timeout_ms": A_LONG_WINDOW_MS, "max_events": 3 }),
                )
                .await
        })
    };

    // When the child asks three times, a moment apart, then a fourth
    for request_id in ["req-1", "req-2", "req-3"] {
        tokio::time::sleep(Duration::from_millis(150)).await;
        world.the_child_asks_permission(request_id).await;
    }
    let (batch, held) = waiting.await.expect("the wait finished");
    world.the_child_asks_permission("req-4").await;

    // Then it answered the three once it held them, well inside its window
    assert!(held < WELL_INSIDE_THE_WINDOW, "the wait was held {held:?}");
    assert_eq!(described(&batch), strings(&["req-1", "req-2", "req-3"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_carries_at_most_max_events_and_leaves_the_rest_for_the_next() {
    // Given three of the child's prompts waiting on a stream
    let world = a_world().await;
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let (started, _) = world.waits(&watch_id, json!({ "timeout_ms": 0 })).await;
    let stream_id = started["stream_id"].clone();
    for request_id in ["req-1", "req-2", "req-3"] {
        world.the_child_asks_permission(request_id).await;
    }

    // When a wait asks for at most two, and the next acknowledges them
    let (first, _) = world
        .waits(
            &watch_id,
            json!({ "stream_id": stream_id, "timeout_ms": 0, "max_events": 2 }),
        )
        .await;
    let (second, _) = world
        .waits(
            &watch_id,
            json!({
                "stream_id": stream_id,
                "ack_seq": first["next_ack_seq"],
                "timeout_ms": 0,
                "max_events": 2
            }),
        )
        .await;

    // Then the first carries the two oldest and the second the one left
    assert_eq!(
        (described(&first), described(&second)),
        (strings(&["req-1", "req-2"]), strings(&["req-3"]))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_answers_what_it_holds_when_its_window_elapses() {
    // Given a wait for three of the child's prompts with a 400 ms window
    let world = Arc::new(a_world().await);
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let waiting = {
        let world = Arc::clone(&world);
        let watch_id = watch_id.clone();
        tokio::spawn(async move {
            world
                .waits(&watch_id, json!({ "timeout_ms": 400, "max_events": 3 }))
                .await
        })
    };

    // When the child asks only once
    tokio::time::sleep(Duration::from_millis(50)).await;
    world.the_child_asks_permission("req-1").await;

    // Then the wait answers that one prompt once its window has elapsed, not before
    let (batch, held) = waiting.await.expect("the wait finished");
    assert!(
        held >= Duration::from_millis(400),
        "the wait answered after {held:?}, before its window"
    );
    assert_eq!(described(&batch), strings(&["req-1"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settle_gathers_a_burst_into_one_answer() {
    // Given a wait for up to ten of the child's prompts that settles 600 ms after its first
    let world = Arc::new(a_world().await);
    let watch_id = world.a_watch_of_the_childs_prompts().await;
    let waiting = {
        let world = Arc::clone(&world);
        let watch_id = watch_id.clone();
        tokio::spawn(async move {
            world
                .waits(
                    &watch_id,
                    json!({
                        "timeout_ms": A_LONG_WINDOW_MS,
                        "max_events": 10,
                        "settle_ms": 600
                    }),
                )
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;

    // When the child asks twice, 150 ms apart
    world.the_child_asks_permission("req-1").await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    world.the_child_asks_permission("req-2").await;

    // Then both arrive in one answer, well inside the window
    let (batch, held) = waiting.await.expect("the wait finished");
    assert!(held < WELL_INSIDE_THE_WINDOW, "the wait was held {held:?}");
    assert_eq!(described(&batch), strings(&["req-1", "req-2"]));
}

// ── picking at wait time ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_picking_a_kind_leaves_the_other_kinds_pending_and_hands_nothing_twice() {
    // Given a stream over the child's prompts and finished turns, the child finishing a turn
    // before it asks permission
    let world = a_world_leasing_streams_for(A_SHORT_LEASE).await;
    let watch_id = world
        .a_watch(json!({ "session_ids": [world.child], "kinds": ["permission", "turn_finished"] }))
        .await;
    let (started, _) = world.waits(&watch_id, json!({ "timeout_ms": 0 })).await;
    let stream_id = started["stream_id"].clone();
    world.finishes_its_turn(&world.child).await;
    world.the_child_asks_permission("req-1").await;

    // When a wait picks permission prompts, the next acknowledges it picking anything, and the one
    // after acknowledges that
    let (prompts, _) = world
        .waits(
            &watch_id,
            json!({
                "stream_id": stream_id,
                "timeout_ms": 0,
                "max_events": 10,
                "kinds": ["permission"]
            }),
        )
        .await;
    let (the_rest, _) = world
        .waits(
            &watch_id,
            json!({
                "stream_id": stream_id,
                "ack_seq": prompts["next_ack_seq"],
                "timeout_ms": 0,
                "max_events": 10
            }),
        )
        .await;
    let (after_both, _) = world
        .waits(
            &watch_id,
            json!({
                "stream_id": stream_id,
                "ack_seq": the_rest["next_ack_seq"],
                "timeout_ms": 0,
                "max_events": 10
            }),
        )
        .await;

    // Then the prompt came first, the finished turn was left for the next wait, nothing came twice
    assert_eq!(
        (
            described(&prompts),
            described(&the_rest),
            described(&after_both)
        ),
        (
            strings(&["req-1"]),
            strings(&["fix-login:turn_finished"]),
            Vec::<String>::new()
        )
    );

    // — and once the stream stops calling, nothing rides on the next answer either
    tokio::time::sleep(PAST_THE_SHORT_LEASE).await;
    assert_eq!(world.next_answers_watch_events().await, Value::Null);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_picking_a_session_leaves_the_others_pending_for_the_next_answer() {
    // Given a stream over both children's prompts, the sibling asking before the child
    let world = a_world_leasing_streams_for(A_SHORT_LEASE).await;
    let watch_id = world
        .a_watch(json!({ "session_ids": [world.child, world.sibling], "kinds": ["permission"] }))
        .await;
    let (started, _) = world.waits(&watch_id, json!({ "timeout_ms": 0 })).await;
    let stream_id = started["stream_id"].clone();
    world.asks_permission(&world.sibling, "req-sibling").await;
    world.the_child_asks_permission("req-child").await;

    // When a wait picks the child's events, the next acknowledges them, and the stream then stops
    let (the_childs, _) = world
        .waits(
            &watch_id,
            json!({
                "stream_id": stream_id,
                "timeout_ms": 0,
                "max_events": 10,
                "session_ids": [world.child]
            }),
        )
        .await;
    world
        .waits(
            &watch_id,
            json!({
                "stream_id": stream_id,
                "ack_seq": the_childs["next_ack_seq"],
                "timeout_ms": 0,
                "max_events": 10,
                "session_ids": [world.child]
            }),
        )
        .await;
    tokio::time::sleep(PAST_THE_SHORT_LEASE).await;

    // Then the child's prompt was picked, and the sibling's, never picked, rides on the next answer
    assert_eq!(described(&the_childs), strings(&["req-child"]));
    assert_eq!(
        answered_request_ids(&world.next_answers_watch_events().await),
        strings(&["req-sibling"])
    );
}
