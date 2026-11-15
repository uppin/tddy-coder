//! A session's name in the orchestrator's tools: `StartSession` takes one and the request carries
//! it, a peer that lists no names still answers `ListProjectSessions`, and a finished start's record
//! names its session as the host running it lists it.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_service::proto::session::{
    start_session_event::Event, StartSessionEvent, StartSessionResponse, StartTaskStarted,
};
use tddy_tool_engine::project_home_tools::ListedSession;
use tddy_tool_engine::start_ledger::{
    begin_recorded_start_with, StartLedger, StartLedgerRecord, StartState, StartedSessionDetails,
    StartedSessionListing,
};
use tddy_tool_engine::start_session_args::{
    start_session_schema, StartChoices, StartContext, StartSessionArgs,
};

const NAME: &str = "Fix the login flow";

fn no_choices() -> StartChoices {
    StartChoices {
        hosts: Vec::new(),
        recipes: Vec::new(),
        agents: Vec::new(),
        session_types: Vec::new(),
    }
}

fn a_context() -> StartContext {
    StartContext {
        session_token: "tok".to_string(),
        project_id: "proj-1".to_string(),
        orchestrator_session_id: "orch-1".to_string(),
        orchestrator_daemon_instance_id: String::new(),
    }
}

// ── StartSession ─────────────────────────────────────────────────────────────────────────────

#[test]
fn start_session_offers_a_name_and_passes_it_through() {
    // Given
    let schema = start_session_schema(&no_choices());
    let args: StartSessionArgs = serde_json::from_value(json!({
        "session_type": "workspace",
        "display_name": NAME,
    }))
    .expect("display_name is a StartSession argument");

    // When
    let request = args.into_request(a_context());

    // Then — a string the schema offers, carried into the request
    assert_eq!(
        (
            schema["properties"]["display_name"]["type"].clone(),
            request.display_name
        ),
        (json!("string"), NAME.to_string())
    );
}

// ── ListProjectSessions ──────────────────────────────────────────────────────────────────────

#[test]
fn a_peer_that_lists_no_names_answers_with_unnamed_sessions() {
    // Given a row as a peer from before session names answers it
    let row = json!({
        "session_id": "s-1",
        "branch": "feature/login",
        "worktree": "/worktrees/s-1",
        "is_active": false,
        "agent_status": "idle",
        "pending_elicitation": false,
        "daemon_instance_id": "host-b",
    });

    // When it is read
    let listed: ListedSession = serde_json::from_value(row).expect("an older row still reads");

    // Then the session is unnamed, and says so when listed on
    assert_eq!(listed.display_name, None);
    assert_eq!(
        serde_json::to_value(&listed).unwrap()["display_name"],
        Value::Null
    );
}

// ── GetSessionStart ──────────────────────────────────────────────────────────────────────────

/// A host that lists every started session with the branch, worktree and name it was given.
struct ListsWithName(&'static str);

#[async_trait]
impl StartedSessionDetails for ListsWithName {
    async fn listing(&self, _: &str, session_id: &str) -> Option<StartedSessionListing> {
        Some(StartedSessionListing {
            branch: format!("feature/{session_id}"),
            worktree: format!("/worktrees/{session_id}"),
            display_name: self.0.to_string(),
        })
    }
}

fn a_start_ending_with(
    session_id: &str,
) -> impl futures_util::Stream<Item = Result<StartSessionEvent, tddy_rpc::Status>> + Send + Unpin + 'static
{
    let events = vec![
        Ok(StartSessionEvent {
            event: Some(Event::Started(StartTaskStarted {
                task_id: "task-1".to_string(),
                daemon_instance_id: "host-a".to_string(),
            })),
        }),
        Ok(StartSessionEvent {
            event: Some(Event::Result(StartSessionResponse {
                session_id: session_id.to_string(),
                ..Default::default()
            })),
        }),
    ];
    stream::iter(events)
}

async fn finished(ledger: &StartLedger, start_id: &str) -> StartLedgerRecord {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(record) = ledger.read(start_id) {
            if record.state != StartState::Running {
                return record;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "start {start_id} never finished"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_finished_start_names_its_session_as_the_host_lists_it() {
    // Given a start whose session the host lists with a name
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());

    // When it succeeds
    let handle = begin_recorded_start_with(
        ledger.clone(),
        a_start_ending_with("session-9"),
        Some(Arc::new(ListsWithName(NAME))),
    )
    .await
    .unwrap();

    // Then its record carries the name beside the session's branch
    let record = finished(&ledger, &handle.start_id).await;
    assert_eq!(
        (
            record.state,
            record.branch.as_deref(),
            record.display_name.as_deref()
        ),
        (StartState::Succeeded, Some("feature/session-9"), Some(NAME))
    );
}

#[tokio::test]
async fn a_finished_start_of_an_unnamed_session_names_none() {
    // Given a start whose session the host lists without a name
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());

    // When it succeeds
    let handle = begin_recorded_start_with(
        ledger.clone(),
        a_start_ending_with("session-9"),
        Some(Arc::new(ListsWithName(""))),
    )
    .await
    .unwrap();

    // Then its record has no name rather than an empty one
    let record = finished(&ledger, &handle.start_id).await;
    assert_eq!(
        (record.state, record.display_name),
        (StartState::Succeeded, None)
    );
}
