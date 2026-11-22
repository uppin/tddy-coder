//! Acceptance: an orchestrator watches the sessions of its project it can see, and every tool answer
//! it receives carries what they did since its last answer, each event once (`#activity-wake` 2/3).
//!
//! The host keeps a project notification log and session watches over it. A child's events are
//! published on the host's bus as the daemon's publish sites publish them; the orchestrator's calls
//! are the real exec-tool calls its tddy-tools makes, and the assertions read the answers' JSON.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use support::{
    a_host_watching_sessions, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_daemon_kernel::session_notification::QuestionNotice;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};

const CREATE_SESSION_WATCH: &str = "CreateSessionWatch";
const LIST_SESSION_WATCHES: &str = "ListSessionWatches";
const RENEW_SESSION_WATCH: &str = "RenewSessionWatch";
const CANCEL_SESSION_WATCH: &str = "CancelSessionWatch";
/// Any tool an orchestrator calls: its answer is where watch events ride.
const ANY_TOOL: &str = "ListProjectSessions";
const AN_HOUR: u64 = 3_600;

struct World {
    a: Host,
    bus: Arc<SessionNotificationBus>,
    /// The project's main session: an orchestrator whose agent is claude-cli.
    orchestrator: String,
    child: String,
    other_child: String,
    os_user: String,
}

/// One host holding an orchestrator of the project (a claude-cli main session) and two sessions
/// of the project, with a notification log and session watches.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_watching_sessions(HOST_A, &peers);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    records_session_type(&a, &orchestrator, "claude-cli");
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let other_child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    World {
        a,
        bus,
        orchestrator,
        child,
        other_child,
        os_user,
    }
}

/// `session_id`'s record names `session_type` as the agent it runs.
fn records_session_type(host: &Host, session_id: &str, session_type: &str) {
    let dir =
        tddy_core::session_lifecycle::unified_session_dir_path(host.sessions.path(), session_id);
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_type = Some(session_type.to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
}

/// `child`'s changeset names `orchestrator` as the session that started it, as a start records it.
fn started_by(host: &Host, child: &str, orchestrator: &str) {
    let dir = tddy_core::session_lifecycle::unified_session_dir_path(host.sessions.path(), child);
    let mut changeset = tddy_core::read_changeset(&dir).unwrap_or_default();
    changeset.orchestrator_session_id = Some(orchestrator.to_string());
    tddy_core::write_changeset(&dir, &changeset).expect("the child's changeset");
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

    /// `session_id`'s agent asks permission to run `cargo test`; answered by `request_id`.
    async fn asks_permission(&self, session_id: &str, request_id: &str) {
        let notification = SessionNotification {
            source: SessionNotificationSource::PendingQuestion,
            text: "🔔 Session fix-login wants to run Bash: cargo test".to_string(),
            question: Some(QuestionNotice {
                request_id: request_id.to_string(),
                kind: "permission".to_string(),
                tool_name: "Bash".to_string(),
                summary: "cargo test".to_string(),
                questions: Vec::new(),
            }),
            ..self.a_notification_of(session_id)
        };
        self.bus.publish(notification).await;
    }

    /// `session_id`'s agent reports `status` (`Done`, `WaitingForInput`) with no question.
    async fn reports(&self, session_id: &str, status: &str) {
        let notification = SessionNotification {
            text: format!("Session fix-login: {status}"),
            activity_status: Some(status.to_string()),
            ..self.a_notification_of(session_id)
        };
        self.bus.publish(notification).await;
    }

    /// `session_id`'s agent ended.
    async fn ends(&self, session_id: &str) {
        let notification = SessionNotification {
            source: SessionNotificationSource::SessionEnded,
            text: "⏹ Session fix-login ended: exit code 0".to_string(),
            ended: Some(tddy_daemon_kernel::session_notification::SessionEndNotice {
                outcome: "exit code 0".to_string(),
                exit_code: Some(0),
            }),
            ..self.a_notification_of(session_id)
        };
        self.bus.publish(notification).await;
    }

    // ── what the orchestrator calls ──────────────────────────────────────────────────────────

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

    async fn calls(&self, tool: &str, args: Value) -> ExecuteToolResponse {
        self.call_as(&self.orchestrator, tool, args).await
    }

    /// A watch the orchestrator created with `args` (plus an hour's TTL unless `args` names one).
    async fn watches(&self, mut args: Value) -> String {
        if args.get("ttl_seconds").is_none() {
            args["ttl_seconds"] = json!(AN_HOUR);
        }
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

/// The `request_id`s of the questions among `watch_events`' events.
fn request_ids(watch_events: &Value) -> Vec<String> {
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

/// The `activity_status` of each of `watch_events`' events.
fn statuses(watch_events: &Value) -> Vec<String> {
    watch_events["events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .map(|event| event["activity_status"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ── delivery ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_permission_prompt_of_a_watched_session_rides_on_the_orchestrators_next_answer() {
    // Given an orchestrator watching the permission prompts of the sessions it started
    let world = a_world().await;
    started_by(&world.a, &world.child, &world.orchestrator);
    world
        .watches(json!({ "started_by_me": true, "kinds": ["permission"] }))
        .await;

    // When a session it started asks permission
    world.asks_permission(&world.child, "req-1").await;

    // Then its next answer, whatever the tool, carries the prompt with the id it is answered by
    let events = world.next_answers_watch_events().await;
    assert_eq!(request_ids(&events), vec!["req-1".to_string()]);
}

#[tokio::test]
async fn a_delivered_event_is_not_delivered_again() {
    // Given an orchestrator that has already received a watched prompt
    let world = a_world().await;
    world
        .watches(json!({ "session_ids": [world.child], "kinds": ["permission"] }))
        .await;
    world.asks_permission(&world.child, "req-1").await;
    assert_eq!(
        request_ids(&world.next_answers_watch_events().await),
        vec!["req-1".to_string()]
    );

    // When it calls another tool
    let again = world.next_answers_watch_events().await;

    // Then that answer carries no watch events
    assert_eq!(again, Value::Null);
}

#[tokio::test]
async fn an_unwatched_kind_or_session_is_not_delivered() {
    // Given an orchestrator watching the child's permission prompts only
    let world = a_world().await;
    world
        .watches(json!({ "session_ids": [world.child], "kinds": ["permission"] }))
        .await;

    // When the child finishes a turn, another session asks permission, and then the child does
    world.reports(&world.child, "Done").await;
    world.asks_permission(&world.other_child, "req-other").await;
    world.asks_permission(&world.child, "req-child").await;

    // Then only the child's prompt is delivered
    assert_eq!(
        request_ids(&world.next_answers_watch_events().await),
        vec!["req-child".to_string()]
    );
}

#[tokio::test]
async fn a_finished_turn_and_a_wait_for_input_are_told_apart() {
    // Given one watch for finished turns and one for waits for input, on the same session
    let world = a_world().await;
    world
        .watches(json!({ "session_ids": [world.child], "kinds": ["turn_finished"] }))
        .await;
    world
        .watches(json!({ "session_ids": [world.other_child], "kinds": ["waiting_for_input"] }))
        .await;

    // When each session finishes a turn and then waits for input
    for session in [&world.child, &world.other_child] {
        world.reports(session, "Done").await;
        world.reports(session, "WaitingForInput").await;
    }

    // Then the child's finished turn and the other session's wait are delivered, nothing else
    assert_eq!(
        statuses(&world.next_answers_watch_events().await),
        vec!["Done".to_string(), "WaitingForInput".to_string()]
    );
}

#[tokio::test]
async fn the_orchestrators_own_events_are_never_delivered() {
    // Given an orchestrator watching everything of the sessions it started — and itself started
    let world = a_world().await;
    started_by(&world.a, &world.orchestrator, &world.orchestrator);
    world.watches(json!({ "started_by_me": true })).await;

    // When the orchestrator's own agent finishes a turn
    world.reports(&world.orchestrator, "Done").await;

    // Then nothing is delivered to it
    assert_eq!(world.next_answers_watch_events().await, Value::Null);
}

#[tokio::test]
async fn an_error_answer_consumes_nothing() {
    // Given a watched prompt waiting
    let world = a_world().await;
    world
        .watches(json!({ "session_ids": [world.child], "kinds": ["permission"] }))
        .await;
    world.asks_permission(&world.child, "req-1").await;

    // When the orchestrator's next call is refused
    let refused = world
        .calls(
            RENEW_SESSION_WATCH,
            json!({ "watch_id": "no-such-watch", "ttl_seconds": AN_HOUR }),
        )
        .await;
    assert!(refused.is_error, "renewing an unknown watch was answered");

    // Then the good answer after it carries the prompt
    assert_eq!(
        request_ids(&world.next_answers_watch_events().await),
        vec!["req-1".to_string()]
    );
}

// ── the watch tools ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_watch_is_listed_renewed_and_cancelled() {
    // Given a watch with one matched event waiting
    let world = a_world().await;
    let watch_id = world
        .watches(
            json!({ "session_ids": [world.child], "kinds": ["permission"], "ttl_seconds": 60 }),
        )
        .await;
    world.asks_permission(&world.child, "req-1").await;

    // When it is listed
    let listed = result_of(&world.calls(LIST_SESSION_WATCHES, json!({})).await);

    // Then it is the one live watch, with one event waiting
    assert_eq!(
        listed["watches"]
            .as_array()
            .map(|watches| watches
                .iter()
                .map(|w| (w["watch_id"].clone(), w["waiting"].clone()))
                .collect::<Vec<_>>())
            .unwrap_or_default(),
        vec![(json!(watch_id), json!(1))]
    );

    // When it is renewed for an hour, its expiry moves later
    let before = listed["watches"][0]["expires_at_unix_ms"]
        .as_u64()
        .expect("an expiry");
    let renewed = result_of(
        &world
            .calls(
                RENEW_SESSION_WATCH,
                json!({ "watch_id": watch_id, "ttl_seconds": AN_HOUR }),
            )
            .await,
    );
    assert!(
        renewed["expires_at_unix_ms"].as_u64().expect("an expiry") > before,
        "renewing did not move the expiry: {renewed}"
    );

    // When it is cancelled, nothing more is delivered and it is no longer listed
    let cancelled = world
        .calls(CANCEL_SESSION_WATCH, json!({ "watch_id": watch_id }))
        .await;
    assert!(!cancelled.is_error, "{}", cancelled.error_message);
    world.asks_permission(&world.child, "req-2").await;
    let after_cancel = world.next_answers_watch_events().await;
    assert_eq!(request_ids(&after_cancel), Vec::<String>::new());
    assert_eq!(
        result_of(&world.calls(LIST_SESSION_WATCHES, json!({})).await)["watches"],
        json!([])
    );
}

#[tokio::test]
async fn a_watch_whose_sessions_all_ended_is_reported_ended_once() {
    // Given a watch naming two sessions
    let world = a_world().await;
    let watch_id = world
        .watches(
            json!({ "session_ids": [world.child, world.other_child], "kinds": ["permission"] }),
        )
        .await;

    // When both sessions' agents end
    world.ends(&world.child).await;
    world.ends(&world.other_child).await;

    // Then the next answer reports the watch ended, once, and it is gone
    assert_eq!(
        world.next_answers_watch_events().await["ended"],
        json!([{ "watch_id": watch_id, "reason": "sessions_ended" }])
    );
    assert_eq!(world.next_answers_watch_events().await, Value::Null);
    assert_eq!(
        result_of(&world.calls(LIST_SESSION_WATCHES, json!({})).await)["watches"],
        json!([])
    );
}

// ── refusals ─────────────────────────────────────────────────────────────────────────────────

/// The refusal `CreateSessionWatch` with `args`, called by `caller`, is answered with.
async fn creating_as(world: &World, caller: &str, args: Value) -> String {
    let answer = world.call_as(caller, CREATE_SESSION_WATCH, args).await;
    assert!(
        answer.is_error,
        "the watch was created: {}",
        answer.result_json
    );
    answer.error_message
}

#[tokio::test]
async fn creating_a_watch_is_refused_with_what_to_do() {
    let world = a_world().await;
    let child = world.child.clone();
    let orchestrator = world.orchestrator.clone();
    let cases: Vec<(&str, String, Value, &str)> = vec![
        (
            "no ttl",
            orchestrator.clone(),
            json!({ "session_ids": [child] }),
            "ttl_seconds",
        ),
        (
            "a zero ttl",
            orchestrator.clone(),
            json!({ "session_ids": [child], "ttl_seconds": 0 }),
            "ttl_seconds",
        ),
        (
            "a ttl over a day",
            orchestrator.clone(),
            json!({ "session_ids": [child], "ttl_seconds": 86_401 }),
            "86400",
        ),
        (
            "no selection",
            orchestrator.clone(),
            json!({ "ttl_seconds": AN_HOUR }),
            "session_ids or started_by_me",
        ),
        (
            "an unknown kind",
            orchestrator.clone(),
            json!({ "session_ids": [child], "kinds": ["everything"], "ttl_seconds": AN_HOUR }),
            "everything",
        ),
        (
            "its own session",
            orchestrator.clone(),
            json!({ "session_ids": [orchestrator], "ttl_seconds": AN_HOUR }),
            "own session",
        ),
        (
            "an unknown session",
            orchestrator.clone(),
            json!({ "session_ids": ["no-such-session"], "ttl_seconds": AN_HOUR }),
            "no-such-session",
        ),
    ];

    for (case, caller, args, says) in cases {
        let refusal = creating_as(&world, &caller, args).await;
        assert!(
            refusal.contains(says),
            "{case}: the refusal {refusal:?} does not say {says:?}"
        );
    }
}

#[tokio::test]
async fn an_orchestrator_may_watch_its_own_trunk_activity_and_nothing_else_of_itself() {
    // Given an orchestrator
    let world = a_world().await;
    let orchestrator = world.orchestrator.clone();

    // When it watches its own session for trunk commits only
    let answer = world
        .call_as(
            &orchestrator,
            CREATE_SESSION_WATCH,
            json!({ "session_ids": [orchestrator], "kinds": ["trunk_commit"], "ttl_seconds": AN_HOUR }),
        )
        .await;

    // Then the watch is created
    assert!(!answer.is_error, "{}", answer.error_message);

    // And a self-watch of any other kind is still refused
    let refusal = creating_as(
        &world,
        &orchestrator,
        json!({ "session_ids": [orchestrator], "kinds": ["trunk_commit", "ended"], "ttl_seconds": AN_HOUR }),
    )
    .await;
    assert!(refusal.contains("own session"), "{refusal}");
}

#[tokio::test]
async fn a_session_that_is_not_an_orchestrator_is_refused_the_watch_tools() {
    // Given an ordinary session of the project
    let world = a_world().await;

    // When it creates a watch
    let refused = world
        .a
        .daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: world.child.clone(),
            tool_name: CREATE_SESSION_WATCH.to_string(),
            args_json: json!({ "session_ids": [world.orchestrator], "ttl_seconds": AN_HOUR })
                .to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await
        .expect_err("an ordinary session's watch was served");

    // Then the call is denied as a project-home tool is to any other session
    assert_eq!(refused.code(), tddy_rpc::Code::PermissionDenied);
}

#[tokio::test]
async fn an_orchestrator_whose_agent_takes_no_turn_is_refused_a_waking_watch_naming_the_agent() {
    // Given an orchestrator whose agent declares no turn input
    let world = a_world().await;
    records_session_type(&world.a, &world.orchestrator, "codex-cli");

    // When it creates a watch that would wake it when idle
    let refusal = creating_as(
        &world,
        &world.orchestrator,
        json!({ "session_ids": [world.child], "ttl_seconds": AN_HOUR, "wake_when_idle": true }),
    )
    .await;

    // Then it is refused, naming the agent and why
    assert!(
        refusal.contains("agent codex declares no turn_input"),
        "the refusal {refusal:?} does not name the agent"
    );
}

#[tokio::test]
async fn an_orchestrator_whose_agent_takes_no_turn_may_hold_a_watch_that_does_not_wake() {
    // Given an orchestrator whose agent declares no turn input
    let world = a_world().await;
    records_session_type(&world.a, &world.orchestrator, "codex-cli");

    // When it creates a watch with no word on the wake
    let answer = world
        .calls(
            CREATE_SESSION_WATCH,
            json!({ "session_ids": [world.child], "ttl_seconds": AN_HOUR }),
        )
        .await;

    // Then the watch is created, and says it does not wake: its events wait to be collected
    assert!(
        !answer.is_error,
        "the watch was refused: {}",
        answer.error_message
    );
    assert_eq!(result_of(&answer)["wake_when_idle"], json!(false));
}

#[tokio::test]
async fn the_seventeenth_watch_is_refused() {
    // Given an orchestrator holding sixteen watches
    let world = a_world().await;
    for _ in 0..16 {
        world.watches(json!({ "session_ids": [world.child] })).await;
    }

    // When it creates another
    let refusal = creating_as(
        &world,
        &world.orchestrator,
        json!({ "session_ids": [world.child], "ttl_seconds": AN_HOUR }),
    )
    .await;

    // Then it is refused, saying to cancel one
    assert!(
        refusal.contains("16"),
        "the refusal {refusal:?} does not name the limit"
    );
}

#[tokio::test]
async fn an_ordinary_session_is_not_offered_the_watch_tools() {
    // Given an ordinary session of the project
    let world = a_world().await;

    // When it lists the tools it is offered
    let offered = world
        .a
        .daemon
        .list_exec_tools(Request::direct(ListExecToolsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: world.child.clone(),
            ..Default::default()
        }))
        .await
        .expect("ListExecTools is served")
        .into_inner();

    // Then none of the watch tools is among them, while the orchestrator is offered all four
    let names = |tools: &[tddy_service::proto::exec_tools::ToolDef]| {
        tools
            .iter()
            .map(|t| t.name.clone())
            .filter(|name| name.contains("SessionWatch"))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&offered.tools), Vec::<String>::new());
    let orchestrators = world
        .a
        .daemon
        .list_exec_tools(Request::direct(ListExecToolsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: world.orchestrator.clone(),
            ..Default::default()
        }))
        .await
        .expect("ListExecTools is served")
        .into_inner();
    assert_eq!(
        names(&orchestrators.tools),
        vec![
            CREATE_SESSION_WATCH.to_string(),
            LIST_SESSION_WATCHES.to_string(),
            RENEW_SESSION_WATCH.to_string(),
            CANCEL_SESSION_WATCH.to_string(),
            // The stream over them (`#activity-wake` 4/5).
            "WaitSessionWatchEvents".to_string(),
        ]
    );
}

/// The orchestrator's queue, holding `n1` then `n2`, and its address.
fn a_queue_of(world: &World, owner: &str) -> tddy_service::proto::work_queue::WorkQueueAddress {
    use tddy_core::changeset::{WorkItem, WorkQueue};
    let queues = tddy_core::session_work_queue::SessionWorkQueues::new(
        world.a.sessions.path().join("sessions"),
    );
    queues
        .create_for_session(owner, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    let item = |id: &str| WorkItem {
        node_id: id.to_string(),
        title: format!("Item {id}"),
        ..WorkItem::default()
    };
    queues
        .update(owner, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![item("n1"), item("n2")],
            }
        })
        .expect("the queue is filled");
    tddy_service::proto::work_queue::WorkQueueAddress {
        project_id: THIS_PROJECT.to_string(),
        queue_id: owner.to_string(),
        daemon_instance_id: HOST_A.to_string(),
    }
}

#[tokio::test]
async fn an_owner_watching_work_queue_is_woken_by_a_user_move_from_the_queue_view() {
    use tddy_service::proto::work_queue::{ReorderWorkItemRequest, WorkQueueService as _};

    // Given an orchestrator watching its own queue's moves
    let world = a_world().await;
    let orchestrator = world.orchestrator.clone();
    let address = a_queue_of(&world, &orchestrator);
    world
        .watches(json!({ "session_ids": [orchestrator], "kinds": ["work_queue"] }))
        .await;

    // When the operator moves n2 up from the queue view
    world
        .a
        .daemon
        .reorder_work_item(Request::direct(ReorderWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: String::new(),
            item_id: "n2".to_string(),
            direction: "up".to_string(),
            stack: Some(address),
        }))
        .await
        .expect("the move is served");

    // Then the orchestrator's next answer carries the move, naming the operator who made it
    let events = world.next_answers_watch_events().await;
    let delivered: Vec<(String, String, String)> = events["events"]
        .as_array()
        .expect("events were delivered")
        .iter()
        .map(|event| {
            (
                event["session_id"].as_str().unwrap_or("").to_string(),
                event["source"].as_str().unwrap_or("").to_string(),
                event["text"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    assert_eq!(
        delivered,
        vec![(
            orchestrator,
            "work_queue".to_string(),
            "Work item Item n2 (n2) moved from position 2 to 1, by testuser".to_string(),
        )]
    );
}
