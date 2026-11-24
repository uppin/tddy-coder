//! Acceptance: an idle orchestrator with events its watches matched is sent one turn listing them;
//! a busy one is not, until its turn ends; bursts coalesce and wakes are spaced (`#activity-wake`
//! 3/3).
//!
//! The orchestrator's agent is stood in for by a shell on its main terminal that puts the terminal
//! in raw mode, turns bracketed paste on as Claude Code's TUI does, and records every byte typed
//! into it — so a test reads exactly what a wake pasted. Its status is recorded in its
//! `.session.yaml` and reported on the bus, as its hooks do; the watched child's events are
//! published on the bus as the daemon's publish sites publish them. Wake timings are shortened
//! through the waker's own constructor.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use support::{
    a_host_waking_orchestrators, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_core::pending_question::{write_pending_question, PendingQuestion};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::session_notification::QuestionNotice;
use tddy_daemon_rpc::exec_tool::watch_wake::WakeTimings;
use tddy_rpc::Request;
use tddy_service::proto::activity::{ActivityService as _, ReportSessionStatusRequest};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const TIMINGS: WakeTimings = WakeTimings {
    settle: Duration::from_millis(50),
    min_gap: Duration::from_millis(800),
};
/// How long a typed byte is given to reach the stand-in agent.
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// Several settles long: a wake that was going to happen has happened by then.
const QUIET_PERIOD: Duration = Duration::from_millis(400);
/// The credential the idle-prompt hook presents.
const IDLE_HOOK_TOKEN: &str = "tok-idle-prompt";
const READY_MARKER: &str = "READY";
/// What every wake starts with: a bracketed paste of the `[tddy watch]` message.
const WAKE_OPENS: &[u8] = b"\x1b[200~[tddy watch]";

struct World {
    a: Host,
    bus: Arc<SessionNotificationBus>,
    orchestrator: String,
    child: String,
    agent: RecordingAgent,
    os_user: String,
}

/// One host whose orchestrator — the project's main, a claude-cli session with a recording
/// stand-in on its main terminal — watches its child's permission prompts with a watch that opts
/// into the wake.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_waking_orchestrators(HOST_A, &peers, TIMINGS);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    records_a_running_claude_session(&a, &orchestrator);
    let agent = RecordingAgent::on_the_main_terminal_of(&a, &orchestrator).await;
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    let world = World {
        a,
        bus,
        orchestrator,
        child,
        agent,
        os_user,
    };
    let created = world
        .calls(
            "CreateSessionWatch",
            json!({
                "session_ids": [world.child],
                "kinds": ["permission"],
                "ttl_seconds": 3_600,
                "wake_when_idle": true
            }),
        )
        .await;
    assert!(!created.0, "the watch was refused: {}", created.1);
    world
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

    /// The orchestrator's hooks report `status`: recorded in its `.session.yaml`, and published.
    async fn the_orchestrator_reports(&self, status: &str) {
        tddy_core::update_activity_status(&session_dir(&self.a, &self.orchestrator), status)
            .expect("the status is recorded");
        let kind = if status == "Running" || status == "ExecutingTool" {
            SessionNotificationKind::Activity
        } else {
            SessionNotificationKind::AttentionRequired
        };
        self.bus
            .publish(SessionNotification {
                kind,
                text: format!("Session main: {status}"),
                activity_status: Some(status.to_string()),
                ..self.a_notification_of(&self.orchestrator)
            })
            .await;
    }

    /// The orchestrator's agent finished its turn: Claude's `idle_prompt` `Notification` hook
    /// reports it through the daemon's activity service, as `tddy-tools session-hook` does.
    async fn the_orchestrator_reports_its_idle_prompt(&self) {
        let dir = session_dir(&self.a, &self.orchestrator);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.hook_token = Some(IDLE_HOOK_TOKEN.to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
        self.a
            .daemon
            .activity_service()
            .report_session_status(Request::direct(ReportSessionStatusRequest {
                session_id: self.orchestrator.clone(),
                hook_token: IDLE_HOOK_TOKEN.to_string(),
                os_user: self.os_user.clone(),
                status: "WaitingForInput".to_string(),
                hook_event_name: "Notification".to_string(),
                message: "Claude is waiting for your input".to_string(),
                ..Default::default()
            }))
            .await
            .expect("the idle prompt is accepted");
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

    /// The orchestrator's own agent waits on a question of its own.
    fn the_orchestrator_asks_a_question(&self) {
        write_pending_question(
            &session_dir(&self.a, &self.orchestrator),
            &PendingQuestion {
                kind: "select".to_string(),
                header: "Plan".to_string(),
                question: "Which plan?".to_string(),
                options: vec!["A".to_string(), "B".to_string()],
                allow_other: true,
                request_id: "req-orchestrators-own".to_string(),
                tool_name: "AskUserQuestion".to_string(),
                tool_input_json: "{}".to_string(),
                questions: Vec::new(),
                at_unix_ms: 1_793_000_000_000,
            },
        )
        .unwrap();
    }

    /// `(refused, text)` of the orchestrator's call to `tool`.
    async fn calls(&self, tool: &str, args: Value) -> (bool, String) {
        let response = self
            .a
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
            .into_inner();
        if response.is_error {
            (true, response.error_message)
        } else {
            (false, response.result_json)
        }
    }

    /// The `watchEvents` of the orchestrator's next answer, `Null` when it has none.
    async fn next_answers_watch_events(&self) -> Value {
        let (refused, text) = self.calls("ListProjectSessions", json!({})).await;
        assert!(!refused, "ListProjectSessions was refused: {text}");
        serde_json::from_str::<Value>(&text).expect("the answer is JSON")["watchEvents"].clone()
    }
}

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

    /// How many wakes have been pasted so far.
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

// ── waking ───────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_idle_orchestrator_is_sent_a_turn_naming_a_watched_permission_prompt() {
    // Given an orchestrator that finished its turn
    let world = a_world().await;
    world.the_orchestrator_reports("Done").await;

    // When the child it watches asks permission
    world.the_child_asks_permission("req-1").await;

    // Then one turn is pasted and submitted, naming the child and the request id
    let typed = world.agent.after_wakes(1).await;
    assert_eq!(world.agent.wakes(), 1, "typed: {typed:?}");
    assert!(
        typed.contains("fix-login") && typed.contains("req-1"),
        "the wake does not name the session and its request: {typed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_waiting_for_input_with_nothing_to_answer_wakes_as_idle() {
    // Given an orchestrator whose agent finished its turn: its idle_prompt hook reported
    let world = a_world().await;
    world.the_orchestrator_reports_its_idle_prompt().await;

    // When the child it watches asks permission
    world.the_child_asks_permission("req-1").await;

    // Then it is idle: one turn is pasted, naming the child and the request id
    let typed = world.agent.after_wakes(1).await;
    assert_eq!(world.agent.wakes(), 1, "typed: {typed:?}");
    assert!(
        typed.contains("fix-login") && typed.contains("req-1"),
        "the wake does not name the session and its request: {typed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_busy_orchestrator_is_woken_only_once_its_turn_ends() {
    // Given an orchestrator mid-turn
    let world = a_world().await;
    world.the_orchestrator_reports("Running").await;

    // When the child asks permission, nothing is pasted while the turn runs
    world.the_child_asks_permission("req-1").await;
    assert_eq!(world.agent.wakes_after_a_quiet_period().await, 0);

    // Then once its turn ends with the prompt still waiting, it is woken
    world.the_orchestrator_reports("Done").await;
    let typed = world.agent.after_wakes(1).await;
    assert!(typed.contains("req-1"), "{typed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_orchestrator_waiting_on_its_own_prompt_is_not_woken() {
    // Given an orchestrator waiting on a question of its own
    let world = a_world().await;
    world.the_orchestrator_asks_a_question();
    world.the_orchestrator_reports("WaitingForInput").await;

    // When the child asks permission
    world.the_child_asks_permission("req-1").await;

    // Then nothing is pasted: a paste would answer the orchestrator's own prompt
    assert_eq!(world.agent.wakes_after_a_quiet_period().await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_inside_the_settle_is_one_turn() {
    // Given an idle orchestrator
    let world = a_world().await;
    world.the_orchestrator_reports("Done").await;

    // When the child asks three times in quick succession
    for request_id in ["req-1", "req-2", "req-3"] {
        world.the_child_asks_permission(request_id).await;
    }

    // Then one turn lists all three
    let typed = world.agent.after_wakes(1).await;
    assert_eq!(
        (
            world.agent.wakes_after_a_quiet_period().await,
            ["req-1", "req-2", "req-3"].map(|id| typed.contains(id)),
        ),
        (1, [true, true, true]),
        "typed: {typed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_event_inside_the_gap_waits_for_the_gap() {
    // Given an orchestrator just woken
    let world = a_world().await;
    world.the_orchestrator_reports("Done").await;
    world.the_child_asks_permission("req-1").await;
    world.agent.after_wakes(1).await;
    let first_wake = tokio::time::Instant::now();

    // When the child asks again right away
    world.the_child_asks_permission("req-2").await;

    // Then the second wake comes, but not before the gap has passed
    let typed = world.agent.after_wakes(2).await;
    assert!(
        first_wake.elapsed() + TIMINGS.settle >= TIMINGS.min_gap,
        "the second wake came {:?} after the first, inside the {:?} gap",
        first_wake.elapsed(),
        TIMINGS.min_gap
    );
    assert!(typed.contains("req-2"), "{typed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_event_delivered_by_a_wake_is_not_attached_to_a_later_answer() {
    // Given an orchestrator woken with the child's prompt
    let world = a_world().await;
    world.the_orchestrator_reports("Done").await;
    world.the_child_asks_permission("req-1").await;
    world.agent.after_wakes(1).await;

    // When it calls a tool in the turn it was woken into
    let events = world.next_answers_watch_events().await;

    // Then that answer carries no watch events
    assert_eq!(events, Value::Null);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_orchestrator_with_no_live_terminal_keeps_its_events() {
    // Given an idle orchestrator whose main terminal is gone
    let world = a_world().await;
    world.the_orchestrator_reports("Done").await;
    world.a.terminals.stop_session(&world.orchestrator).await;

    // When the child asks permission
    world.the_child_asks_permission("req-1").await;
    tokio::time::sleep(QUIET_PERIOD).await;

    // Then the prompt is still waiting, on the orchestrator's next answer
    let events = world.next_answers_watch_events().await;
    assert_eq!(
        events["events"][0]["question"]["request_id"],
        json!("req-1"),
        "the prompt was lost: {events}"
    );
}
