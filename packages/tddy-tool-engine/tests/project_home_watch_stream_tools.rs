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
    execute_project_home_watch_stream_tool, ProjectHomeWatchStreamPort, WatchStreamRequest,
    WAIT_SESSION_WATCH_EVENTS,
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
async fn the_wait_is_capped_at_twenty_seconds() {
    // Given a host that would answer any stream call
    let host = FakeStreams::default();

    // When a wait asks for 20 001 ms, and another for exactly 20 000
    let over = calling(&host, json!({ "watch_ids": ["w-1"], "timeout_ms": 20_001 })).await;
    let at_the_cap = calling(&host, json!({ "watch_ids": ["w-1"], "timeout_ms": 20_000 })).await;

    // Then the first is refused naming the cap before the port is asked, and the second is held
    // for the whole 20 s
    assert!(
        over.is_error,
        "a wait over the cap was served: {}",
        over.result_json
    );
    assert!(
        over.error_message.contains("20000"),
        "the refusal {:?} does not name the cap",
        over.error_message
    );
    assert!(!at_the_cap.is_error, "{}", at_the_cap.error_message);
    assert_eq!(
        host.asked()
            .iter()
            .map(|request| request.timeout_ms)
            .collect::<Vec<_>>(),
        vec![20_000]
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
        }]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&outcome.result_json).expect("the answer is JSON"),
        an_empty_batch()
    );
}
