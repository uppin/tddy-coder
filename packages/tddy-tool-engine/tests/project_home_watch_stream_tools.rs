//! `WaitSessionWatchEvents`' own rules — who is offered it, its cap, and how its arguments reach
//! the port (`#activity-wake` 4/5).
//!
//! The stream is the daemon's: here the port is a fake that records what it was asked.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_tool_engine::project_home_disk_refresh::RefreshQueued;
use tddy_tool_engine::project_home_tools::{
    session_offered_catalog_for, ListProjectSessionsResult, ListedSession, ProjectHomePort,
    SessionToolRole,
};
use tddy_tool_engine::project_home_watch_stream_tools::{
    execute_project_home_watch_stream_tool, project_home_watch_stream_tool_defs,
    ProjectHomeWatchStreamPort, WatchStreamRequest, DEFAULT_WATCH_WAIT_MS, MAX_EVENTS_PER_WAIT,
    MAX_SETTLE_MS, MAX_WATCH_WAIT_MS, WAIT_SESSION_WATCH_EVENTS,
};
use tddy_tool_engine::project_home_worktree_tools::WorktreeEntry;
use tddy_tool_engine::session_follow::{SessionActivity, ToolCallBodies, TranscriptItem};
use tddy_tool_engine::start_ledger::{StartHandle, StartLedgerRecord};
use tddy_tool_engine::start_options::{StartOptions, StartOptionsQuery};
use tddy_tool_engine::start_session_args::{StartChoices, StartSessionArgs};
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget, TargetRecord};

fn the_home() -> HomeCaller {
    HomeCaller {
        session_id: "home".to_string(),
        project_id: "proj-a".to_string(),
    }
}

/// A port that records each request and answers an empty batch of stream `s-1`.
#[derive(Default)]
struct FakeStreams {
    asked: Mutex<Vec<WatchStreamRequest>>,
}

impl FakeStreams {
    fn asked(&self) -> Vec<WatchStreamRequest> {
        self.asked.lock().unwrap().clone()
    }
}

/// What [`FakeStreams`] answers every call.
fn an_empty_batch() -> Value {
    json!({ "stream_id": "s-1", "events": [], "lost": 0, "ended": [], "next_ack_seq": 41 })
}

#[async_trait]
impl ProjectHomeWatchStreamPort for FakeStreams {
    async fn wait_watch_events(
        &self,
        _: &HomeCaller,
        request: WatchStreamRequest,
    ) -> Result<Value, String> {
        self.asked.lock().unwrap().push(request);
        Ok(an_empty_batch())
    }
}

#[async_trait]
impl ProjectHomePort for FakeStreams {
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn list_sessions(&self, _: &str, _: &str) -> anyhow::Result<Vec<ListedSession>> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn list_peer_sessions(&self, _: &str, _: &str) -> ListProjectSessionsResult {
        ListProjectSessionsResult {
            sessions: Vec::new(),
            unreachable_hosts: Vec::new(),
        }
    }
    async fn start_session(
        &self,
        _: &str,
        _: &str,
        _: StartSessionArgs,
    ) -> anyhow::Result<StartHandle> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn get_session_start(&self, _: &str, _: &str) -> anyhow::Result<StartLedgerRecord> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn cancel_session_start(&self, _: &str, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn push_identity(&self, _: &SessionTarget) -> anyhow::Result<PushIdentity> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn list_worktrees(&self, _: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn session_activity(&self, _: &SessionTarget) -> anyhow::Result<SessionActivity> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn session_transcript(&self, _: &SessionTarget) -> anyhow::Result<Vec<TranscriptItem>> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn session_tool_call(
        &self,
        _: &SessionTarget,
        _: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn start_options(&self, _: &str, _: StartOptionsQuery) -> anyhow::Result<StartOptions> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn terminate_session(&self, _: &SessionTarget) -> anyhow::Result<()> {
        anyhow::bail!("not used by the stream tool")
    }
    async fn refresh_worktree_disk_usage(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<RefreshQueued> {
        anyhow::bail!("not used by the stream tool")
    }
}

async fn calling(host: &FakeStreams, args: Value) -> ToolOutcome {
    execute_project_home_watch_stream_tool(
        host,
        &the_home(),
        WAIT_SESSION_WATCH_EVENTS,
        &args.to_string(),
    )
    .await
}

/// Whether a session of `role` is offered the stream tool.
fn offered_to(role: SessionToolRole) -> bool {
    session_offered_catalog_for(
        role,
        &StartChoices {
            hosts: Vec::new(),
            recipes: Vec::new(),
            agents: Vec::new(),
            session_types: Vec::new(),
        },
    )
    .iter()
    .any(|tool| tool.name == WAIT_SESSION_WATCH_EVENTS)
}

#[test]
fn the_stream_tool_is_offered_to_the_project_home_role_only() {
    // Given each role a session can have
    let roles = [
        SessionToolRole::ProjectHome,
        SessionToolRole::ConversationSpawner,
        SessionToolRole::Ordinary,
    ];

    // When each is offered its catalogue
    let offered = roles.map(offered_to);

    // Then only the project-home session finds the stream tool in it
    assert_eq!(offered, [true, false, false]);
}

#[tokio::test]
async fn the_wait_holds_a_minute_by_default_and_at_most_thirty_minutes() {
    // Given a host that would answer any stream call
    let host = FakeStreams::default();

    // When a wait names no timeout, another the most it may, and a third a millisecond more
    let by_default = calling(&host, json!({ "watch_ids": ["w-1"] })).await;
    let at_the_most = calling(
        &host,
        json!({ "watch_ids": ["w-1"], "timeout_ms": 1_800_000 }),
    )
    .await;
    let over = calling(
        &host,
        json!({ "watch_ids": ["w-1"], "timeout_ms": 1_800_001 }),
    )
    .await;

    // Then the first is held a minute and the second thirty, and the third is refused naming the
    // most before the port is asked
    assert!(!by_default.is_error, "{}", by_default.error_message);
    assert!(!at_the_most.is_error, "{}", at_the_most.error_message);
    assert!(
        over.is_error && over.error_message.contains("1800000"),
        "a wait over the most was answered {} {:?}",
        over.result_json,
        over.error_message
    );
    assert_eq!(
        (
            host.asked()
                .iter()
                .map(|request| request.timeout_ms)
                .collect::<Vec<_>>(),
            DEFAULT_WATCH_WAIT_MS,
            MAX_WATCH_WAIT_MS,
        ),
        (vec![60_000, 1_800_000], 60_000, 1_800_000)
    );
}

#[tokio::test]
async fn a_wait_returns_its_first_event_alone_by_default_and_picks_nothing() {
    // Given a host that would answer any stream call
    let host = FakeStreams::default();

    // When a wait names only its watch
    let outcome = calling(&host, json!({ "watch_ids": ["w-1"] })).await;

    // Then it returns at one event, settles for nothing, and returns every kind of every session
    assert!(!outcome.is_error, "{}", outcome.error_message);
    let asked = host.asked();
    assert_eq!(
        (
            asked[0].max_events,
            asked[0].settle_ms,
            asked[0].kinds.clone(),
            asked[0].session_ids.clone()
        ),
        (1, 0, Vec::<String>::new(), Vec::<String>::new())
    );
}

#[tokio::test]
async fn the_stream_tool_hands_the_port_its_window_and_pick() {
    // Given a host that answers a batch
    let host = FakeStreams::default();

    // When a wait asks for up to five events, a settle, and one kind of one session
    let outcome = calling(
        &host,
        json!({
            "watch_ids": ["w-1"],
            "timeout_ms": 90_000,
            "max_events": 5,
            "settle_ms": 300,
            "kinds": ["permission"],
            "session_ids": ["s-9"]
        }),
    )
    .await;

    // Then the port is asked exactly that
    assert!(!outcome.is_error, "{}", outcome.error_message);
    assert_eq!(
        host.asked(),
        vec![WatchStreamRequest {
            watch_ids: vec!["w-1".to_string()],
            ack_seq: None,
            stream_id: None,
            timeout_ms: 90_000,
            max_events: 5,
            settle_ms: 300,
            kinds: vec!["permission".to_string()],
            session_ids: vec!["s-9".to_string()],
        }]
    );
}

#[tokio::test]
async fn a_window_or_pick_out_of_bounds_is_refused_before_the_port() {
    // Given a host that would answer any stream call
    let host = FakeStreams::default();
    let cases = [
        (
            json!({ "watch_ids": ["w-1"], "max_events": 0 }),
            "max_events",
        ),
        (
            json!({ "watch_ids": ["w-1"], "max_events": MAX_EVENTS_PER_WAIT + 1 }),
            "50",
        ),
        (
            json!({ "watch_ids": ["w-1"], "settle_ms": MAX_SETTLE_MS + 1 }),
            "5000",
        ),
        (
            json!({ "watch_ids": ["w-1"], "kinds": ["everything"] }),
            "everything",
        ),
        (
            json!({ "watch_ids": ["w-1"], "session_ids": [7] }),
            "session_ids",
        ),
    ];

    for (args, says) in cases {
        // When
        let outcome = calling(&host, args.clone()).await;

        // Then
        assert!(
            outcome.is_error && outcome.error_message.contains(says),
            "{args}: answered {} {:?}, not a refusal saying {says:?}",
            outcome.result_json,
            outcome.error_message
        );
    }
    assert_eq!(host.asked(), Vec::<WatchStreamRequest>::new());
}

#[test]
fn the_tool_says_a_long_hold_becomes_a_background_task() {
    // Given the stream tool's definition
    let def = project_home_watch_stream_tool_defs()
        .into_iter()
        .find(|def| def.name == WAIT_SESSION_WATCH_EVENTS)
        .expect("the stream tool is defined");
    let schema: Value = serde_json::from_str(&def.input_schema_json).unwrap();

    // When its timeout is read
    let timeout = &schema["properties"]["timeout_ms"];

    // Then it defaults to a minute, allows thirty, and says a hold past two minutes runs in the
    // background
    assert_eq!(
        (timeout["default"].clone(), timeout["maximum"].clone()),
        (json!(60_000), json!(1_800_000))
    );
    assert!(
        def.description.contains("background"),
        "the description does not say a long hold runs in the background: {}",
        def.description
    );
}

#[tokio::test]
async fn the_stream_tool_hands_the_port_its_watches_and_acknowledgement() {
    // Given a host that answers a batch
    let host = FakeStreams::default();

    // When a stream's call names two watches, its stream and what it printed
    let outcome = calling(
        &host,
        json!({
            "watch_ids": ["w-1", "w-2"],
            "stream_id": "s-1",
            "ack_seq": 40,
            "timeout_ms": 5_000
        }),
    )
    .await;

    // Then the port is asked exactly that, and its batch is the answer
    assert!(!outcome.is_error, "{}", outcome.error_message);
    assert_eq!(
        host.asked(),
        vec![WatchStreamRequest {
            watch_ids: vec!["w-1".to_string(), "w-2".to_string()],
            ack_seq: Some(40),
            stream_id: Some("s-1".to_string()),
            timeout_ms: 5_000,
            max_events: 1,
            settle_ms: 0,
            kinds: Vec::new(),
            session_ids: Vec::new(),
        }]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&outcome.result_json).expect("the answer is JSON"),
        an_empty_batch()
    );
}
