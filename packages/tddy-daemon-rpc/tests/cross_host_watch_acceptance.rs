//! Acceptance: an orchestrator on one host watches, messages and lists the sessions it started on
//! another connected host, and they message it back (n5 / n6, cross-host watches).
//!
//! Two daemons, `host-a` and `host-b`, run in this process and reach each other through the
//! in-memory peer forwarder the cross-host suites share — each forwarded call handed to the other
//! daemon's own service, stamped as arriving over the common room. No LiveKit. A third eligible host,
//! `host-c`, has no daemon behind it.
//!
//! The orchestrator is the project's main session on `host-a`, a claude-cli session. Its child runs
//! on `host-b`; the child's changeset names the orchestrator and its host, as a start addressed at
//! `host-b` records it. Every event of the child is published on `host-b`'s own bus — where its hooks
//! publish it — and every call is the real exec-tool call the session's tddy-tools makes on its own
//! host. Whatever reaches `host-a` gets there through the notification relay over the peer forward.
//!
//! Answering a remote child's question or permission, and typing a turn into a remote child
//! (`SendSessionMessage`), already route to the child's host: `cross_host_answer_wait_acceptance`
//! and `project_home_turn_control_acceptance` prove them, so they are not repeated here.
//!
//! Feature: docs/ft/daemon/project-concept.md § Watching sessions; session messages.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use support::{
    a_host_relaying, a_workspace_in, arriving_from_a_peer, Host, InProcessPeers, HOST_A, HOST_B,
    THIS_PROJECT,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::session_notification::{QuestionNotice, SessionEndNotice};
use tddy_daemon_rpc::exec_tool::watch_wake::WakeTimings;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
};
use tddy_service::proto::session::{ListSessionsRequest, SessionService as _};
use tddy_session_activity::notification_relay::{
    FOLLOW_SESSION_NOTIFICATIONS, RELAY_SESSION_NOTIFICATIONS,
};
use tddy_session_activity::project_notification_log::PROJECT_NOTIFICATION_CAPACITY;
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const POST_SESSION_MESSAGE: &str = "PostSessionMessage";
const READ_SESSION_MESSAGES: &str = "ReadSessionMessages";
const CREATE_SESSION_WATCH: &str = "CreateSessionWatch";
const LIST_PROJECT_SESSIONS: &str = "ListProjectSessions";
/// Any tool an orchestrator calls: its answer is where watch events ride.
const ANY_TOOL: &str = "ListProjectSessions";
/// A host the common room does not list.
const AN_UNKNOWN_HOST: &str = "host-z";

const TIMINGS: WakeTimings = WakeTimings {
    settle: Duration::from_millis(50),
    min_gap: Duration::from_millis(200),
};
/// How soon a relay retries a host that did not take its batch.
const RELAY_RETRY: Duration = Duration::from_millis(50);
/// How long anything relayed is given to arrive.
const ARRIVAL_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a test waits for something that must NOT arrive.
const QUIET_PERIOD: Duration = Duration::from_millis(400);
const READY_MARKER: &str = "READY";
const WAKE_OPENS: &[u8] = b"\x1b[200~[tddy watch]";

struct World {
    peers: Arc<InProcessPeers>,
    a: Host,
    b: Host,
    b_bus: Arc<SessionNotificationBus>,
    /// The project's main session on `host-a`: a claude-cli orchestrator.
    orchestrator: String,
    /// A session the orchestrator started on `host-b`.
    child: String,
    /// A session the orchestrator started on `host-a`.
    sibling: String,
    /// A session of the project on `host-b` that nobody started.
    bystander: String,
    os_user: String,
}

/// The orchestrator on `host-a`, its child on `host-b` and its sibling on `host-a`, and a session
/// on `host-b` nobody started.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, _a_bus) = a_host_relaying(HOST_A, &peers, TIMINGS, RELAY_RETRY);
    let (b, b_bus) = a_host_relaying(HOST_B, &peers, TIMINGS, RELAY_RETRY);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    records_a_running_claude_session(&a, &orchestrator);
    let child = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    records_its_orchestrator(&b, &child, &orchestrator, Some(HOST_A));
    let sibling = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    records_its_orchestrator(&a, &sibling, &orchestrator, None);
    let bystander = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    World {
        peers,
        a,
        b,
        b_bus,
        orchestrator,
        child,
        sibling,
        bystander,
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

/// `started`'s changeset names `orchestrator` — on `orchestrator_host`, or this host for `None` —
/// as the session that started it, as a start does.
fn records_its_orchestrator(
    host: &Host,
    started: &str,
    orchestrator: &str,
    orchestrator_host: Option<&str>,
) {
    let dir = session_dir(host, started);
    let mut changeset = tddy_core::read_changeset(&dir).unwrap_or_default();
    changeset.orchestrator_session_id = Some(orchestrator.to_string());
    changeset.orchestrator_daemon_instance_id = orchestrator_host.map(str::to_string);
    tddy_core::write_changeset(&dir, &changeset).unwrap();
}

// ── what the sessions call, each on its own host ─────────────────────────────────────────────

async fn call_on(host: &Host, caller: &str, tool: &str, args: Value) -> ExecuteToolResponse {
    host.daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: caller.to_string(),
            tool_name: tool.to_string(),
            args_json: args.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await
        .expect("the exec-tool call is served")
        .into_inner()
}

fn result_of(answer: &ExecuteToolResponse) -> Value {
    serde_json::from_str(&answer.result_json).expect("the answer is JSON")
}

fn accepted(answer: ExecuteToolResponse, what: &str) -> Value {
    assert!(
        !answer.is_error,
        "{what} was refused: {}",
        answer.error_message
    );
    result_of(&answer)
}

fn message_id_of(answer: ExecuteToolResponse) -> String {
    accepted(answer, POST_SESSION_MESSAGE)["message_id"]
        .as_str()
        .expect("an accepted message has an id")
        .to_string()
}

impl World {
    /// The child, on `host-b`, sends `to` the message `text` (`tddy-tools send`).
    async fn the_child_posts(&self, to: &str, args: Value) -> ExecuteToolResponse {
        let mut args = args;
        args["to_session_id"] = json!(to);
        call_on(&self.b, &self.child, POST_SESSION_MESSAGE, args).await
    }

    /// The orchestrator, on `host-a`, sends `to` a message with `args`.
    async fn the_orchestrator_posts(&self, to: &str, args: Value) -> ExecuteToolResponse {
        let mut args = args;
        args["to_session_id"] = json!(to);
        call_on(&self.a, &self.orchestrator, POST_SESSION_MESSAGE, args).await
    }

    /// The orchestrator watches the sessions it started, of `kinds`, for an hour.
    async fn the_orchestrator_watches_its_sessions(&self, kinds: &[&str], wake: bool) {
        accepted(
            call_on(
                &self.a,
                &self.orchestrator,
                CREATE_SESSION_WATCH,
                json!({
                    "started_by_me": true,
                    "kinds": kinds,
                    "ttl_seconds": 3_600,
                    "wake_when_idle": wake,
                }),
            )
            .await,
            CREATE_SESSION_WATCH,
        );
    }

    /// The watch events on the orchestrator's next answer, waiting until at least `count` arrived.
    async fn the_orchestrators_watch_events(&self, count: usize) -> (Vec<Value>, u64) {
        let deadline = tokio::time::Instant::now() + ARRIVAL_TIMEOUT;
        let mut events = Vec::new();
        let mut lost = 0;
        loop {
            let answer = accepted(
                call_on(&self.a, &self.orchestrator, ANY_TOOL, json!({})).await,
                ANY_TOOL,
            );
            if let Some(more) = answer["watchEvents"]["events"].as_array() {
                events.extend(more.iter().cloned());
            }
            lost += answer["watchEvents"]["lost"].as_u64().unwrap_or(0);
            if events.len() >= count {
                return (events, lost);
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "only {} of {count} watch events arrived: {events:#?}",
                events.len()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// How many watch events and how many lost the orchestrator's answers carry, until together
    /// they reach `count`.
    async fn the_orchestrators_watch_events_and_losses(&self, count: u64) -> (u64, u64) {
        let deadline = tokio::time::Instant::now() + ARRIVAL_TIMEOUT;
        let (mut events, mut lost) = (0_u64, 0_u64);
        loop {
            let answer = accepted(
                call_on(&self.a, &self.orchestrator, ANY_TOOL, json!({})).await,
                ANY_TOOL,
            );
            events += answer["watchEvents"]["events"]
                .as_array()
                .map_or(0, |more| more.len() as u64);
            lost += answer["watchEvents"]["lost"].as_u64().unwrap_or(0);
            if events + lost >= count {
                return (events, lost);
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "only {events} events and {lost} lost of {count} arrived"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// What `reader` reads of its messages on `host`, waiting until at least `count` arrived.
    async fn reads_messages(&self, host: &Host, reader: &str, count: usize) -> Vec<Value> {
        let deadline = tokio::time::Instant::now() + ARRIVAL_TIMEOUT;
        let mut read = Vec::new();
        loop {
            let answer = accepted(
                call_on(host, reader, READ_SESSION_MESSAGES, json!({})).await,
                READ_SESSION_MESSAGES,
            );
            read.extend(answer["messages"].as_array().cloned().unwrap_or_default());
            if read.len() >= count {
                return read;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "only {} of {count} messages reached {reader}: {read:#?}",
                read.len()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// The child's hooks publish `notification` on `host-b`'s bus, as they do for a real child.
    async fn the_child_publishes(&self, source: SessionNotificationSource, shape: Shape) {
        let mut notification = SessionNotification {
            session_id: self.child.clone(),
            os_user: self.os_user.clone(),
            label: "child".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source,
            text: "Session child".to_string(),
            agent: None,
            question: None,
            ended: None,
            activity_status: None,
            message: None,
            at_unix_ms: 1_790_000_000_000,
        };
        match shape {
            Shape::Question(request_id) => {
                notification.text = "Session child asks: Which database?".to_string();
                notification.question = Some(QuestionNotice {
                    request_id: request_id.to_string(),
                    kind: "select".to_string(),
                    tool_name: "AskUserQuestion".to_string(),
                    summary: "Which database?".to_string(),
                    questions: Vec::new(),
                });
            }
            Shape::Ended => {
                notification.text = "Session child ended: exit code 0".to_string();
                notification.ended = Some(SessionEndNotice {
                    outcome: "exit code 0".to_string(),
                    exit_code: Some(0),
                });
            }
            Shape::TurnFinished(n) => {
                notification.text = format!("Session child: Done ({n})");
                notification.activity_status = Some("Done".to_string());
            }
        }
        self.b_bus.publish(notification).await;
    }
}

enum Shape {
    Question(&'static str),
    Ended,
    TurnFinished(usize),
}

fn kinds_and_hosts(events: &[Value]) -> Vec<(String, String, String)> {
    events
        .iter()
        .map(|event| {
            (
                event["session_id"].as_str().unwrap_or_default().to_string(),
                event["source"].as_str().unwrap_or_default().to_string(),
                event["daemon_instance_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

// ── messages, both ways ──────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_childs_message_lands_in_its_orchestrators_inbox_naming_its_host() {
    // Given the orchestrator on host-a and the child it started on host-b
    let world = a_world().await;

    // When the child sends its orchestrator a message, naming no host
    let message_id = message_id_of(
        world
            .the_child_posts(
                &world.orchestrator,
                json!({ "text": "Tests pass; pushed 3f2a1c." }),
            )
            .await,
    );

    // Then the orchestrator reads it on its own host, from the child, on host-b
    let read = world.reads_messages(&world.a, &world.orchestrator, 1).await;
    assert_eq!(
        read.iter()
            .map(|m| (
                m["message_id"].clone(),
                m["from_session_id"].clone(),
                m["from_daemon_instance_id"].clone(),
                m["text"].clone(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            json!(message_id),
            json!(world.child),
            json!(HOST_B),
            json!("Tests pass; pushed 3f2a1c."),
        )]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_orchestrators_reply_reaches_the_remote_child_by_reply_to_alone() {
    // Given the child's question, which the orchestrator read
    let world = a_world().await;
    let question = message_id_of(
        world
            .the_child_posts(&world.orchestrator, json!({ "text": "Open the PR?" }))
            .await,
    );
    world.reads_messages(&world.a, &world.orchestrator, 1).await;

    // When the orchestrator replies to it, naming no host
    let reply = message_id_of(
        world
            .the_orchestrator_posts(
                &world.child,
                json!({ "text": "Yes, open it.", "reply_to": question }),
            )
            .await,
    );

    // Then the child reads the reply on its own host, naming the question and the orchestrator's host
    let read = world.reads_messages(&world.b, &world.child, 1).await;
    assert_eq!(
        read.iter()
            .map(|m| (
                m["message_id"].clone(),
                m["from_session_id"].clone(),
                m["from_daemon_instance_id"].clone(),
                m["reply_to"].clone(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            json!(reply),
            json!(world.orchestrator),
            json!(HOST_A),
            json!(question)
        )]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_orchestrator_messages_a_remote_child_by_naming_its_host() {
    // Given the orchestrator and its child on host-b
    let world = a_world().await;

    // When the orchestrator sends the child a message, naming host-b
    let message_id = message_id_of(
        world
            .the_orchestrator_posts(
                &world.child,
                json!({ "text": "Rebase on main.", "daemon_instance_id": HOST_B }),
            )
            .await,
    );

    // Then the child reads it on host-b
    let read = world.reads_messages(&world.b, &world.child, 1).await;
    assert_eq!(
        read.iter()
            .map(|m| (m["message_id"].clone(), m["text"].clone()))
            .collect::<Vec<_>>(),
        vec![(json!(message_id), json!("Rebase on main."))]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_session_is_found_on_its_host_when_none_is_named() {
    // Given the orchestrator and its child on host-b
    let world = a_world().await;

    // When the orchestrator sends the child a message naming no host
    let message_id = message_id_of(
        world
            .the_orchestrator_posts(&world.child, json!({ "text": "Status?" }))
            .await,
    );

    // Then the child reads it on host-b
    let read = world.reads_messages(&world.b, &world.child, 1).await;
    assert_eq!(read[0]["message_id"], json!(message_id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_sibling_is_refused_exactly_as_a_sibling_on_the_same_host() {
    // Given the child on host-b and its sibling on host-a, neither an orchestrator
    let world = a_world().await;

    // When the child sends its sibling a message, unprompted
    let refused = world
        .the_child_posts(
            &world.sibling,
            json!({ "text": "Hi.", "daemon_instance_id": HOST_A }),
        )
        .await;

    // Then it is refused as not addressable, as on one host
    assert!(
        refused.is_error
            && refused
                .error_message
                .contains("is not a session you may message"),
        "a sibling on another host was addressable: {refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_to_a_host_the_room_does_not_list_is_refused_naming_the_host() {
    // Given the orchestrator
    let world = a_world().await;

    // When it messages a session on a host the common room does not list
    let refused = world
        .the_orchestrator_posts(
            &world.child,
            json!({ "text": "Hi.", "daemon_instance_id": AN_UNKNOWN_HOST }),
        )
        .await;

    // Then the sender is told, naming the host — nothing is queued for nowhere
    assert!(
        refused.is_error && refused.error_message.contains(AN_UNKNOWN_HOST),
        "a message to an unknown host was not refused naming it: {refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_sent_while_the_link_is_down_is_delivered_once_it_is_back() {
    // Given the link to host-a is down
    let world = a_world().await;
    world.peers.cut_link(HOST_A);

    // When the child sends its orchestrator a message, and the link comes back later
    let message_id = message_id_of(
        world
            .the_child_posts(
                &world.orchestrator,
                json!({ "text": "Done while you were away." }),
            )
            .await,
    );
    tokio::time::sleep(QUIET_PERIOD).await;
    let while_down = accepted(
        call_on(
            &world.a,
            &world.orchestrator,
            READ_SESSION_MESSAGES,
            json!({}),
        )
        .await,
        READ_SESSION_MESSAGES,
    );
    world.peers.restore_link(HOST_A);

    // Then nothing arrived while the link was down, and the message arrives exactly once after
    assert_eq!(while_down["messages"], json!([]));
    let read = world.reads_messages(&world.a, &world.orchestrator, 1).await;
    tokio::time::sleep(QUIET_PERIOD).await;
    let again = accepted(
        call_on(
            &world.a,
            &world.orchestrator,
            READ_SESSION_MESSAGES,
            json!({}),
        )
        .await,
        READ_SESSION_MESSAGES,
    );
    assert_eq!(
        (
            read.len(),
            read[0]["message_id"].clone(),
            again["messages"].clone()
        ),
        (1, json!(message_id), json!([]))
    );
}

// ── watches ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_childs_message_question_and_end_arrive_through_the_orchestrators_watch() {
    // Given the orchestrator watches the sessions it started
    let world = a_world().await;
    world
        .the_orchestrator_watches_its_sessions(&["message", "question", "ended"], false)
        .await;

    // When its child on host-b sends it a message, asks a question, and ends
    message_id_of(
        world
            .the_child_posts(&world.orchestrator, json!({ "text": "Halfway." }))
            .await,
    );
    world
        .the_child_publishes(
            SessionNotificationSource::PendingQuestion,
            Shape::Question("q-db"),
        )
        .await;
    world
        .the_child_publishes(SessionNotificationSource::SessionEnded, Shape::Ended)
        .await;

    // Then the watch delivers all three, in order, each naming the child's host, the question with
    // the request id it is answered by
    let (events, lost) = world.the_orchestrators_watch_events(3).await;
    assert_eq!(
        (kinds_and_hosts(&events), lost),
        (
            vec![
                (
                    world.child.clone(),
                    "message".to_string(),
                    HOST_B.to_string()
                ),
                (
                    world.child.clone(),
                    "pending_question".to_string(),
                    HOST_B.to_string()
                ),
                (
                    world.child.clone(),
                    "session_ended".to_string(),
                    HOST_B.to_string()
                ),
            ],
            0
        )
    );
    assert_eq!(events[1]["question"]["request_id"], json!("q-db"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_sessions_watch_event_names_this_host() {
    // Given the orchestrator watches the sessions it started
    let world = a_world().await;
    world
        .the_orchestrator_watches_its_sessions(&["message"], false)
        .await;

    // When its sibling on host-a sends it a message
    message_id_of(
        call_on(
            &world.a,
            &world.sibling,
            POST_SESSION_MESSAGE,
            json!({ "to_session_id": world.orchestrator, "text": "Local." }),
        )
        .await,
    );

    // Then the event names host-a
    let (events, _) = world.the_orchestrators_watch_events(1).await;
    assert_eq!(events[0]["daemon_instance_id"], json!(HOST_A));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watch_naming_a_remote_session_it_did_not_start_follows_it() {
    // Given the orchestrator watches a session on host-b that it did not start
    let world = a_world().await;
    accepted(
        call_on(
            &world.a,
            &world.orchestrator,
            CREATE_SESSION_WATCH,
            json!({
                "session_ids": [world.bystander],
                "kinds": ["turn_finished"],
                "ttl_seconds": 3_600,
            }),
        )
        .await,
        CREATE_SESSION_WATCH,
    );

    // When that session finishes a turn on host-b
    world
        .b_bus
        .publish(SessionNotification {
            session_id: world.bystander.clone(),
            os_user: world.os_user.clone(),
            label: "bystander".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::ActivityStatus,
            text: "Session bystander: Done".to_string(),
            agent: None,
            question: None,
            ended: None,
            activity_status: Some("Done".to_string()),
            message: None,
            at_unix_ms: 1_790_000_000_000,
        })
        .await;

    // Then the watch delivers it, naming host-b
    let (events, _) = world.the_orchestrators_watch_events(1).await;
    assert_eq!(
        kinds_and_hosts(&events),
        vec![(
            world.bystander.clone(),
            "activity_status".to_string(),
            HOST_B.to_string()
        )]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watch_naming_a_session_on_no_connected_host_is_refused() {
    // Given the orchestrator
    let world = a_world().await;

    // When it asks to watch a session no host holds
    let refused = call_on(
        &world.a,
        &world.orchestrator,
        CREATE_SESSION_WATCH,
        json!({
            "session_ids": ["01900000-0000-7000-8000-AABB00000999"],
            "kinds": ["ended"],
            "ttl_seconds": 3_600,
        }),
    )
    .await;

    // Then it is refused, naming the session
    assert!(
        refused.is_error
            && refused
                .error_message
                .contains("01900000-0000-7000-8000-AABB00000999"),
        "a watch over a session nobody holds was created: {refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remote_childs_event_wakes_its_idle_orchestrator() {
    // Given the orchestrator idles in its main terminal, watching its sessions with a wake
    let world = a_world().await;
    let agent = RecordingAgent::on_the_main_terminal_of(&world.a, &world.orchestrator).await;
    tddy_core::update_activity_status(&session_dir(&world.a, &world.orchestrator), "Done")
        .expect("the status is recorded");
    world
        .the_orchestrator_watches_its_sessions(&["question"], true)
        .await;

    // When its child on host-b asks a question
    world
        .the_child_publishes(
            SessionNotificationSource::PendingQuestion,
            Shape::Question("q-wake"),
        )
        .await;

    // Then the orchestrator is woken with it
    let typed = agent.after_a_wake().await;
    assert!(
        typed.contains("q-wake"),
        "the wake did not carry the remote question: {typed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_relayed_after_the_link_comes_back_arrive_once_and_in_order() {
    // Given the orchestrator watches its sessions, and the link to host-a goes down
    let world = a_world().await;
    world
        .the_orchestrator_watches_its_sessions(&["turn_finished"], false)
        .await;
    world.peers.cut_link(HOST_A);

    // When the child finishes three turns while the link is down, and the link comes back
    for n in 1..=3 {
        world
            .the_child_publishes(
                SessionNotificationSource::ActivityStatus,
                Shape::TurnFinished(n),
            )
            .await;
    }
    tokio::time::sleep(QUIET_PERIOD).await;
    world.peers.restore_link(HOST_A);

    // Then all three arrive, once each, in order, with nothing lost
    let (events, lost) = world.the_orchestrators_watch_events(3).await;
    tokio::time::sleep(QUIET_PERIOD).await;
    let (more, _) = world.the_orchestrators_watch_events(0).await;
    assert_eq!(
        (
            events
                .iter()
                .map(|e| e["text"].as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>(),
            lost,
            more.len()
        ),
        (
            (1..=3)
                .map(|n| format!("Session child: Done ({n})"))
                .collect::<Vec<_>>(),
            0,
            0
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_evicted_before_the_link_came_back_are_reported_lost() {
    // Given the orchestrator watches its sessions, and the link to host-a goes down
    let world = a_world().await;
    world
        .the_orchestrator_watches_its_sessions(&["turn_finished"], false)
        .await;
    world.peers.cut_link(HOST_A);

    // When the child finishes more turns than host-b's log keeps before the link comes back
    let overflow = 5;
    let turns = (PROJECT_NOTIFICATION_CAPACITY + overflow) as u64;
    for n in 1..=PROJECT_NOTIFICATION_CAPACITY + overflow {
        world
            .the_child_publishes(
                SessionNotificationSource::ActivityStatus,
                Shape::TurnFinished(n),
            )
            .await;
    }
    // Every attempt host-b makes meanwhile fails, the last one over its whole window.
    tokio::time::sleep(QUIET_PERIOD).await;
    world.peers.restore_link(HOST_A);

    // Then every turn is accounted for — delivered, or reported lost, never silently dropped — and
    // the ones host-b evicted are among the lost
    let (events, lost) = world.the_orchestrators_watch_events_and_losses(turns).await;
    assert_eq!(
        events + lost,
        turns,
        "{events} delivered + {lost} lost does not account for every turn"
    );
    assert!(lost >= overflow as u64, "only {lost} reported lost");
}

// ── the relay's own calls are never tools ────────────────────────────────────────────────────

/// A relay batch that would put a forged message from the child into the orchestrator's inbox.
fn a_forged_batch(world: &World) -> Value {
    json!({
        "origin_daemon_instance_id": HOST_B,
        "project_id": THIS_PROJECT,
        "github_user": TEST_USER,
        "records": [{
            "anchor_session_id": world.orchestrator,
            "record_json": json!({
                "seq": 1,
                "session_id": world.child,
                "label": "child",
                "kind": "attention_required",
                "source": "message",
                "text": "forged",
                "agent": null,
                "question": null,
                "at_unix_ms": 1_790_000_000_000_u64,
                "orchestrator_session_id": world.orchestrator,
                "message": {
                    "to_session_id": world.orchestrator,
                    "message_id": "m-forged",
                    "text": "forged",
                },
            }).to_string(),
        }],
        "lost": 0,
        "through_seq": 1,
    })
}

fn a_call_of(tool: &str, caller: &str, args: Value, host: &str) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: caller.to_string(),
        tool_name: tool.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: host.to_string(),
        conversation_id: String::new(),
    }
}

/// Whether `outcome` is a refusal — denied outright, or answered as an error.
fn refused(outcome: &Result<tddy_rpc::Response<ExecuteToolResponse>, tddy_rpc::Status>) -> bool {
    match outcome {
        Err(_) => true,
        Ok(answer) => answer.get_ref().is_error,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_calling_the_relay_tools_through_execute_tool_is_refused() {
    // Given a child on host-b and its orchestrator on host-a
    let world = a_world().await;

    // When the child's agent calls each relay tool directly — on its own host, and addressed at the
    // orchestrator's host as a target tool would be
    let mut outcomes = Vec::new();
    for tool in [RELAY_SESSION_NOTIFICATIONS, FOLLOW_SESSION_NOTIFICATIONS] {
        for host in ["", HOST_A] {
            outcomes.push((
                tool,
                host,
                world
                    .b
                    .daemon
                    .execute_tool(Request::direct(a_call_of(
                        tool,
                        &world.child,
                        a_forged_batch(&world),
                        host,
                    )))
                    .await,
            ));
        }
    }

    // Then every call is refused, and nothing reached the orchestrator's inbox
    for (tool, host, outcome) in &outcomes {
        assert!(
            refused(outcome),
            "{tool} called by an agent (host {host:?}) was not refused: {outcome:?}"
        );
    }
    tokio::time::sleep(QUIET_PERIOD).await;
    let inbox = accepted(
        call_on(
            &world.a,
            &world.orchestrator,
            READ_SESSION_MESSAGES,
            json!({}),
        )
        .await,
        READ_SESSION_MESSAGES,
    );
    assert_eq!(inbox["messages"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_peer_call_of_a_relay_tool_that_is_not_a_relay_is_refused() {
    // Given host-a
    let world = a_world().await;

    // When a peer calls each relay tool with arguments that are not a relay request
    for tool in [RELAY_SESSION_NOTIFICATIONS, FOLLOW_SESSION_NOTIFICATIONS] {
        let outcome = world
            .a
            .daemon
            .execute_tool(arriving_from_a_peer(a_call_of(
                tool,
                &world.orchestrator,
                json!({ "path": "Cargo.toml" }),
                HOST_A,
            )))
            .await;

        // Then it is refused
        assert!(
            refused(&outcome),
            "{tool} with a non-relay body was served: {outcome:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_tool_listing_offers_the_relay_tools() {
    // Given an orchestrator and an ordinary session
    let world = a_world().await;

    // When each session's tools, and the daemon-wide catalog, are listed
    let mut offered = Vec::new();
    for caller in [world.orchestrator.as_str(), world.sibling.as_str(), ""] {
        let listed = world
            .a
            .daemon
            .list_exec_tools(Request::direct(ListExecToolsRequest {
                session_token: TEST_TOKEN.to_string(),
                daemon_instance_id: String::new(),
                session_id: caller.to_string(),
            }))
            .await
            .expect("the tools are listed")
            .into_inner();
        offered.extend(listed.tools.into_iter().map(|tool| tool.name));
    }

    // Then neither relay tool is among them
    assert!(
        !offered.iter().any(|name| {
            name == RELAY_SESSION_NOTIFICATIONS || name == FOLLOW_SESSION_NOTIFICATIONS
        }),
        "a relay tool is offered: {offered:?}"
    );
}

// ── the session lists ────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_project_sessions_shows_the_remote_child_labelled_with_its_host() {
    // Given the orchestrator on host-a and its child on host-b
    let world = a_world().await;

    // When the orchestrator lists the project's sessions
    let listed = accepted(
        call_on(
            &world.a,
            &world.orchestrator,
            LIST_PROJECT_SESSIONS,
            json!({}),
        )
        .await,
        LIST_PROJECT_SESSIONS,
    );

    // Then the child is there, on host-b, labelled with host-b's name
    let child = listed["sessions"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["session_id"] == json!(world.child))
        })
        .cloned()
        .unwrap_or_else(|| panic!("the child is not listed: {listed:#}"));
    assert_eq!(
        (
            child["daemon_instance_id"].clone(),
            child["host_label"].clone()
        ),
        (json!(HOST_B), json!(HOST_B))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_sessions_with_peers_shows_the_remote_child_with_its_host() {
    // Given the orchestrator on host-a and its child on host-b
    let world = a_world().await;

    // When host-a is asked for every connected host's sessions, as the session drawer asks
    let listed = world
        .a
        .daemon
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
            include_peers: true,
        }))
        .await
        .expect("ListSessions is served")
        .into_inner();

    // Then the child is listed on host-b beside the orchestrator on host-a
    let hosts: Vec<(String, String)> = listed
        .sessions
        .iter()
        .filter(|s| s.session_id == world.child || s.session_id == world.orchestrator)
        .map(|s| (s.session_id.clone(), s.daemon_instance_id.clone()))
        .collect();
    let mut expected = vec![
        (world.orchestrator.clone(), HOST_A.to_string()),
        (world.child.clone(), HOST_B.to_string()),
    ];
    let mut hosts = hosts;
    hosts.sort();
    expected.sort();
    assert_eq!(hosts, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_sessions_without_peers_lists_this_host_only() {
    // Given the orchestrator on host-a and its child on host-b
    let world = a_world().await;

    // When host-a is asked for its sessions, as a peer asks it
    let listed = world
        .a
        .daemon
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
            include_peers: false,
        }))
        .await
        .expect("ListSessions is served")
        .into_inner();

    // Then only host-a's sessions are listed
    assert!(
        listed.sessions.iter().all(|s| s.session_id != world.child),
        "a peer's session was listed without include_peers"
    );
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

    /// Wait until one wake has been pasted and submitted; what was typed.
    async fn after_a_wake(&self) -> String {
        wait_for(&self.record, |typed| {
            typed.windows(WAKE_OPENS.len()).any(|w| w == WAKE_OPENS) && typed.ends_with(b"\r")
        })
        .await;
        String::from_utf8_lossy(&std::fs::read(&self.record).unwrap_or_default()).into_owned()
    }
}

async fn wait_for(record: &Path, done: impl Fn(&[u8]) -> bool) {
    let deadline = tokio::time::Instant::now() + ARRIVAL_TIMEOUT;
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
