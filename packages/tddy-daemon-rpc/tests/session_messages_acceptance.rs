//! Acceptance: sessions send each other messages through `PostSessionMessage`, and the recipient
//! reads them through `ReadSessionMessages` — never typed into its terminal, never among the
//! operator's notifications. Who may address whom is read from the sessions' own records; an
//! orchestrator's watch of kind `message` delivers what its sessions send, once, and wakes it when
//! idle (`#activity-wake` 5/5).
//!
//! The host keeps a project notification log, session watches over it and the wake of
//! `#activity-wake` 3/3. Any session may watch its own messages (the kind `message` alone); every
//! other watch stays an orchestrator's. The project's main session is a claude-cli orchestrator; the sessions it
//! started name it as their `orchestrator_session_id` in their changesets, as a start records it.
//! Every call is the real exec-tool call the session's tddy-tools makes.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use support::{
    a_host_waking_orchestrators, a_workspace_in, Host, InProcessPeers, HOST_A, OTHER_PROJECT,
    THIS_PROJECT,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_rpc::exec_tool::watch_wake::WakeTimings;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const POST_SESSION_MESSAGE: &str = "PostSessionMessage";
const READ_SESSION_MESSAGES: &str = "ReadSessionMessages";
const READ_PROJECT_NOTIFICATIONS: &str = "ReadProjectNotifications";
const CREATE_SESSION_WATCH: &str = "CreateSessionWatch";
/// Any tool an orchestrator calls: its answer is where watch events ride.
const ANY_TOOL: &str = "ListProjectSessions";
/// Any tool a session that is not an orchestrator calls, reading nothing that matters.
const ANY_WORKSPACE_TOOL: &str = "Glob";
/// A session id no session of this host's OS user has: how a session of another OS user looks to
/// the caller's daemon, which reads only the caller's own sessions.
const ANOTHER_USERS_SESSION: &str = "01900000-0000-7000-8000-AABB00000999";
const FROM_THE_START: &str = "0";

const TIMINGS: WakeTimings = WakeTimings {
    settle: Duration::from_millis(50),
    min_gap: Duration::from_millis(200),
};
/// How long a typed byte is given to reach the stand-in agent.
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const READY_MARKER: &str = "READY";
/// What every wake starts with: a bracketed paste of the `[tddy watch]` message.
const WAKE_OPENS: &[u8] = b"\x1b[200~[tddy watch]";

struct World {
    a: Host,
    bus: Arc<SessionNotificationBus>,
    /// The project's main session: a claude-cli orchestrator.
    orchestrator: String,
    /// A session the orchestrator started.
    child: String,
    /// Another session the orchestrator started.
    sibling: String,
    /// A session of another project of the same OS user.
    stranger: String,
    os_user: String,
}

/// One host: the project's main orchestrator, two sessions it started, and a session of another
/// project.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_waking_orchestrators(HOST_A, &peers, TIMINGS);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    records_a_running_claude_session(&a, &orchestrator);
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let sibling = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    for started in [&child, &sibling] {
        records_its_orchestrator(&a, started, &orchestrator);
    }
    let stranger = a_workspace_in(&a.daemon, OTHER_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    World {
        a,
        bus,
        orchestrator,
        child,
        sibling,
        stranger,
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

/// `started`'s changeset names `orchestrator` as the session that started it, as a start does.
fn records_its_orchestrator(host: &Host, started: &str, orchestrator: &str) {
    let dir = session_dir(host, started);
    let mut changeset = tddy_core::read_changeset(&dir).unwrap_or_default();
    changeset.orchestrator_session_id = Some(orchestrator.to_string());
    changeset.orchestrator_daemon_instance_id = None;
    tddy_core::write_changeset(&dir, &changeset).unwrap();
}

/// `session_id` is a codex-cli session: its agent declares no `turn_input`, so no turn can be
/// pasted into it.
fn records_a_session_taking_no_turn(host: &Host, session_id: &str) {
    let dir = session_dir(host, session_id);
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_type = Some("codex-cli".to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
}

// ── what the sessions call ───────────────────────────────────────────────────────────────────

impl World {
    async fn call_as(&self, caller: &str, tool: &str, args: Value) -> ExecuteToolResponse {
        self.a
            .daemon
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

    /// `from` posts `text` to `to`; the answer as it came.
    async fn posts(&self, from: &str, to: &str, text: &str) -> ExecuteToolResponse {
        self.call_as(
            from,
            POST_SESSION_MESSAGE,
            json!({ "to_session_id": to, "text": text }),
        )
        .await
    }

    /// `from` sends `to` the message `text`, which must be accepted; its `message_id`.
    async fn sends(&self, from: &str, to: &str, text: &str) -> String {
        message_id_of(&self.posts(from, to, text).await)
    }

    /// `from` replies to message `reply_to` with `text`; the answer as it came.
    async fn replies(
        &self,
        from: &str,
        to: &str,
        reply_to: &str,
        text: &str,
    ) -> ExecuteToolResponse {
        self.call_as(
            from,
            POST_SESSION_MESSAGE,
            json!({ "to_session_id": to, "text": text, "reply_to": reply_to }),
        )
        .await
    }

    /// `reader`'s unread messages, which it must be answered.
    async fn reads_its_messages(&self, reader: &str) -> Vec<Value> {
        let answer = self.call_as(reader, READ_SESSION_MESSAGES, json!({})).await;
        assert!(
            !answer.is_error,
            "ReadSessionMessages was refused: {}",
            answer.error_message
        );
        result_of(&answer)["messages"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// The sources of every notification the orchestrator's inbox of attention records holds.
    async fn the_orchestrators_notification_sources(&self) -> Vec<Value> {
        let answer = self
            .call_as(
                &self.orchestrator,
                READ_PROJECT_NOTIFICATIONS,
                json!({ "cursor": FROM_THE_START }),
            )
            .await;
        assert!(
            !answer.is_error,
            "ReadProjectNotifications was refused: {}",
            answer.error_message
        );
        result_of(&answer)["items"]
            .as_array()
            .map(|items| items.iter().map(|item| item["source"].clone()).collect())
            .unwrap_or_default()
    }

    /// A watch of the orchestrator over the messages the sessions it started send, for an hour,
    /// waking it when it is idle.
    async fn the_orchestrator_watches_what_its_sessions_send(&self) {
        let answer = self
            .call_as(
                &self.orchestrator,
                CREATE_SESSION_WATCH,
                json!({
                    "started_by_me": true,
                    "kinds": ["message"],
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
    }

    /// `caller` asks for a watch with `args`, for an hour: the answer, or the status it was
    /// denied with.
    async fn asks_for_a_watch(
        &self,
        caller: &str,
        args: Value,
    ) -> Result<ExecuteToolResponse, tddy_rpc::Status> {
        let mut args = args;
        args["ttl_seconds"] = json!(3_600);
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: caller.to_string(),
                tool_name: CREATE_SESSION_WATCH.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// `caller`'s watch with `args`, which must be created.
    async fn watches_with(&self, caller: &str, args: Value) -> ExecuteToolResponse {
        let answer = self
            .asks_for_a_watch(caller, args)
            .await
            .unwrap_or_else(|status| panic!("the watch was denied: {}", status.message()));
        assert!(
            !answer.is_error,
            "the watch was refused: {}",
            answer.error_message
        );
        answer
    }

    /// The `message_id` of each message among the watch events on the orchestrator's next answer.
    async fn messages_on_the_orchestrators_next_answer(&self) -> Vec<String> {
        let answer = self.call_as(&self.orchestrator, ANY_TOOL, json!({})).await;
        assert!(
            !answer.is_error,
            "{ANY_TOOL} was refused: {}",
            answer.error_message
        );
        message_ids_on(&answer)
    }

    /// The `message_id` of each message among the watch events on the child's next answer.
    async fn messages_on_the_childs_next_answer(&self) -> Vec<String> {
        let answer = self
            .call_as(
                &self.child,
                ANY_WORKSPACE_TOOL,
                json!({ "pattern": "*.no-such-file" }),
            )
            .await;
        assert!(
            !answer.is_error,
            "{ANY_WORKSPACE_TOOL} was refused: {}",
            answer.error_message
        );
        message_ids_on(&answer)
    }
}

/// The `message_id` of each message among the watch events `answer` carries.
fn message_ids_on(answer: &ExecuteToolResponse) -> Vec<String> {
    result_of(answer)["watchEvents"]["events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .filter_map(|event| event["message"]["message_id"].as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

impl World {
    /// The orchestrator's hooks report `status`: recorded in its `.session.yaml`, and published.
    async fn the_orchestrator_reports(&self, status: &str) {
        tddy_core::update_activity_status(&session_dir(&self.a, &self.orchestrator), status)
            .expect("the status is recorded");
        self.bus
            .publish(SessionNotification {
                session_id: self.orchestrator.clone(),
                os_user: self.os_user.clone(),
                label: "main".to_string(),
                kind: SessionNotificationKind::AttentionRequired,
                source: SessionNotificationSource::ActivityStatus,
                text: format!("Session main: {status}"),
                agent: None,
                question: None,
                ended: None,
                activity_status: Some(status.to_string()),
                message: None,
                at_unix_ms: 1_790_000_000_000,
            })
            .await;
    }
}

fn result_of(answer: &ExecuteToolResponse) -> Value {
    serde_json::from_str(&answer.result_json).expect("the answer is JSON")
}

/// The `message_id` an accepted post was answered.
fn message_id_of(answer: &ExecuteToolResponse) -> String {
    assert!(
        !answer.is_error,
        "PostSessionMessage was refused: {}",
        answer.error_message
    );
    result_of(answer)["message_id"]
        .as_str()
        .expect("an accepted message has an id")
        .to_string()
}

/// Each message as `(message_id, from_session_id, reply_to, text)`.
fn as_read(messages: &[Value]) -> Vec<(Value, Value, Value, Value)> {
    messages
        .iter()
        .map(|m| {
            (
                m["message_id"].clone(),
                m["from_session_id"].clone(),
                m["reply_to"].clone(),
                m["text"].clone(),
            )
        })
        .collect()
}

fn ids_of(messages: &[Value]) -> Vec<String> {
    messages
        .iter()
        .map(|m| m["message_id"].as_str().unwrap_or("").to_string())
        .collect()
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

// ── sending and reading ──────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_started_session_messages_its_orchestrator_and_the_orchestrator_reads_it() {
    // Given a session the orchestrator started
    let world = a_world().await;

    // When it sends the orchestrator a message, and the orchestrator reads its messages
    let message_id = world
        .sends(
            &world.child,
            &world.orchestrator,
            "Tests pass; pushed 3f2a1c.",
        )
        .await;
    let read = world.reads_its_messages(&world.orchestrator).await;

    // Then the orchestrator reads exactly that message, from the session that sent it
    assert_eq!(
        as_read(&read),
        vec![(
            json!(message_id),
            json!(world.child),
            Value::Null,
            json!("Tests pass; pushed 3f2a1c."),
        )]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reply_carries_reply_to_and_is_refused_for_a_message_never_received() {
    // Given the orchestrator's question to the child, which the child read, and another it sent
    // the sibling
    let world = a_world().await;
    let question = world
        .sends(&world.orchestrator, &world.child, "Open the PR?")
        .await;
    let to_the_sibling = world
        .sends(&world.orchestrator, &world.sibling, "Rebase on main.")
        .await;
    world.reads_its_messages(&world.child).await;

    // When the child replies to the question, and then to the message it never received
    let reply = world
        .replies(
            &world.child,
            &world.orchestrator,
            &question,
            "Yes, opening it.",
        )
        .await;
    let misdirected = world
        .replies(&world.child, &world.orchestrator, &to_the_sibling, "Done.")
        .await;

    // Then the orchestrator reads the reply naming the question, and the second is refused naming
    // the message the child never received
    let reply_id = message_id_of(&reply);
    assert_eq!(
        as_read(&world.reads_its_messages(&world.orchestrator).await),
        vec![(
            json!(reply_id),
            json!(world.child),
            json!(question),
            json!("Yes, opening it."),
        )]
    );
    assert!(
        misdirected.is_error && misdirected.error_message.contains(&to_the_sibling),
        "the reply to a message never received was not refused naming it: {:?}",
        misdirected.error_message
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn addressing_outside_the_rule_is_refused_naming_who_may_be_addressed() {
    // Given a session the orchestrator started
    let world = a_world().await;

    // When it messages a session of another project, a session of another OS user, itself, and a
    // sibling the same orchestrator started
    let mut answers = Vec::new();
    for to in [
        world.stranger.as_str(),
        ANOTHER_USERS_SESSION,
        world.child.as_str(),
        world.sibling.as_str(),
    ] {
        answers.push(world.posts(&world.child, to, "Hello?").await);
    }

    // Then each is refused, and each refusal names the orchestrator it may address
    assert_eq!(
        answers
            .iter()
            .map(|answer| (
                answer.is_error,
                answer.error_message.contains(&world.orchestrator)
            ))
            .collect::<Vec<_>>(),
        vec![(true, true); 4],
        "the refusals: {:?}",
        answers
            .iter()
            .map(|answer| answer.error_message.as_str())
            .collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_is_not_in_read_project_notifications() {
    // Given a message a session sent its orchestrator
    let world = a_world().await;
    world
        .sends(&world.child, &world.orchestrator, "Tests pass.")
        .await;

    // When the orchestrator reads its project's notifications
    let sources = world.the_orchestrators_notification_sources().await;

    // Then the message is not among them
    assert!(
        !sources.contains(&json!("message")),
        "a message reached ReadProjectNotifications: {sources:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_session_messages_returns_each_message_once() {
    // Given two messages a session sent its orchestrator
    let world = a_world().await;
    let first = world
        .sends(&world.child, &world.orchestrator, "first")
        .await;
    let second = world
        .sends(&world.child, &world.orchestrator, "second")
        .await;

    // When the orchestrator reads its messages twice
    let once = world.reads_its_messages(&world.orchestrator).await;
    let again = world.reads_its_messages(&world.orchestrator).await;

    // Then the first read answers both, oldest first, and the second answers none
    assert_eq!(
        (ids_of(&once), ids_of(&again)),
        (vec![first, second], Vec::<String>::new())
    );
}

// ── watching what sessions send ──────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_orchestrators_message_watch_delivers_what_its_sessions_send_once_even_when_also_in_its_inbox(
) {
    // Given an orchestrator watching the messages the sessions it started send
    let world = a_world().await;
    world
        .the_orchestrator_watches_what_its_sessions_send()
        .await;

    // When the child sends it a message, which its next answer carries, and it then reads its
    // inbox
    let message_id = world
        .sends(&world.child, &world.orchestrator, "Tests pass.")
        .await;
    let on_the_answer = world.messages_on_the_orchestrators_next_answer().await;
    let in_the_inbox = world.reads_its_messages(&world.orchestrator).await;

    // Then the watch delivered the message, and the inbox does not deliver it again
    assert_eq!(
        (on_the_answer, ids_of(&in_the_inbox)),
        (vec![message_id], Vec::<String>::new())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_idle_orchestrator_is_woken_by_a_watched_message() {
    // Given an idle orchestrator, with a stand-in agent on its main terminal, watching the
    // messages the sessions it started send
    let world = a_world().await;
    let agent = RecordingAgent::on_the_main_terminal_of(&world.a, &world.orchestrator).await;
    world
        .the_orchestrator_watches_what_its_sessions_send()
        .await;
    world.the_orchestrator_reports("Done").await;

    // When the child sends it a message
    world
        .sends(
            &world.child,
            &world.orchestrator,
            "Tests pass; want the PR?",
        )
        .await;

    // Then it is woken with the message's text
    let typed = agent.after_a_wake().await;
    assert!(
        typed.contains("Tests pass; want the PR?"),
        "the wake did not carry the message: {typed:?}"
    );
}

// ── a session's own message watch ────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_started_session_may_watch_its_own_messages_and_no_other_watch() {
    // Given a session the orchestrator started — not an orchestrator itself
    let world = a_world().await;

    // When it asks for its own messages (the kind `message` alone), then for a sibling's prompts,
    // the orchestrator's messages, and the messages of the sessions it started
    let own = world
        .watches_with(&world.child, json!({ "kinds": ["message"] }))
        .await;
    let others = [
        json!({ "session_ids": [world.sibling], "kinds": ["permission"] }),
        json!({ "session_ids": [world.orchestrator], "kinds": ["message"] }),
        json!({ "started_by_me": true, "kinds": ["message"] }),
    ];
    let mut denied = Vec::new();
    for args in others {
        denied.push(
            world
                .asks_for_a_watch(&world.child, args)
                .await
                .err()
                .map(|status| status.code()),
        );
    }

    // Then its own message watch is created, and every other watch is denied as the watch tools
    // are to a session that is not an orchestrator
    assert!(
        result_of(&own)["watch_id"].is_string(),
        "the session's own message watch has no id: {}",
        own.result_json
    );
    assert_eq!(denied, vec![Some(tddy_rpc::Code::PermissionDenied); 3]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sessions_own_message_watch_carries_the_messages_addressed_to_it() {
    // Given a session the orchestrator started, watching its own messages
    let world = a_world().await;
    world
        .watches_with(&world.child, json!({ "kinds": ["message"] }))
        .await;

    // When the orchestrator sends it a message, and the session calls any tool
    let message_id = world
        .sends(&world.orchestrator, &world.child, "Rebase on main.")
        .await;
    let on_the_answer = world.messages_on_the_childs_next_answer().await;

    // Then that answer carries the message addressed to it
    assert_eq!(on_the_answer, vec![message_id]);
}

// ── a session no turn can be pasted into ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_without_turn_input_reads_its_messages() {
    // Given a codex-cli session the orchestrator started, which no turn can be pasted into
    let world = a_world().await;
    records_a_session_taking_no_turn(&world.a, &world.child);

    // When the orchestrator sends it a message, and it reads its messages
    let message_id = world
        .sends(&world.orchestrator, &world.child, "Rebase on main, please.")
        .await;
    let read = world.reads_its_messages(&world.child).await;

    // Then it reads the message from its orchestrator
    assert_eq!(
        as_read(&read),
        vec![(
            json!(message_id),
            json!(world.orchestrator),
            Value::Null,
            json!("Rebase on main, please."),
        )]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sessions_own_message_watch_wakes_it_only_when_asked_and_only_if_it_takes_a_turn() {
    // Given a claude session and a codex session the orchestrator started
    let world = a_world().await;
    records_a_running_claude_session(&world.a, &world.child);
    let codex = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    records_its_orchestrator(&world.a, &codex, &world.orchestrator);
    records_a_session_taking_no_turn(&world.a, &codex);
    let own_messages = json!({ "kinds": ["message"] });

    // When the claude session creates its message watch with no word on the wake, and again
    // asking for it; and the codex session asks for it too
    let quiet = world.watches_with(&world.child, own_messages.clone()).await;
    let mut waking_args = own_messages.clone();
    waking_args["wake_when_idle"] = json!(true);
    let waking = world.watches_with(&world.child, waking_args.clone()).await;
    let refused = world
        .asks_for_a_watch(&codex, waking_args)
        .await
        .expect("the codex session's watch call is served");

    // Then the first does not wake, the second does, and the codex session is refused saying why
    assert_eq!(
        (
            result_of(&quiet)["wake_when_idle"].clone(),
            result_of(&waking)["wake_when_idle"].clone()
        ),
        (json!(false), json!(true))
    );
    assert!(
        refused.is_error && refused.error_message.contains("wake_when_idle"),
        "the codex session's waking watch was answered {} {:?}",
        refused.result_json,
        refused.error_message
    );
}
