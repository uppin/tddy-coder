//! The start ledger hands back a start's handle at its first event and records the rest of the
//! start as it happens, durably (`#start-parity` 4/9).
//!
//! Each test drives a scripted `StreamStartSession` event stream, so every state the record can be
//! in is reached without a daemon.

use std::time::Duration;

use futures_util::stream;
use pretty_assertions::assert_eq;
use tddy_rpc::Status;
use tddy_service::proto::session::{
    start_phase, start_session_event::Event, StartPhase, StartSessionEvent, StartSessionResponse,
    StartTaskStarted,
};
use tddy_tool_engine::start_ledger::{
    begin_recorded_start, StartLedger, StartLedgerRecord, StartState, StartStep,
};
use tokio::sync::mpsc;

type EventResult = Result<StartSessionEvent, Status>;

/// A start whose events the test sends one at a time.
struct ScriptedStart {
    tx: mpsc::UnboundedSender<EventResult>,
}

fn a_scripted_start() -> (
    ScriptedStart,
    impl futures_util::Stream<Item = EventResult> + Send + Unpin + 'static,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let events = Box::pin(stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|event| (event, rx))
    }));
    (ScriptedStart { tx }, events)
}

impl ScriptedStart {
    fn send(&self, event: Event) {
        self.tx
            .send(Ok(StartSessionEvent { event: Some(event) }))
            .unwrap();
    }

    fn fail(&self, status: Status) {
        self.tx.send(Err(status)).unwrap();
    }
}

fn started(task_id: &str, on: &str) -> Event {
    Event::Started(StartTaskStarted {
        task_id: task_id.to_string(),
        daemon_instance_id: on.to_string(),
    })
}

fn phase(step: start_phase::Step, boundary: start_phase::Boundary) -> Event {
    Event::Phase(StartPhase {
        step: step as i32,
        boundary: boundary as i32,
    })
}

fn result(session_id: &str) -> Event {
    Event::Result(StartSessionResponse {
        session_id: session_id.to_string(),
        ..Default::default()
    })
}

async fn eventually(
    ledger: &StartLedger,
    start_id: &str,
    until: impl Fn(&StartLedgerRecord) -> bool,
) -> StartLedgerRecord {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(record) = ledger.read(start_id) {
            if until(&record) {
                return record;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the record of {start_id} never reached the expected state; last: {:?}",
            ledger.read(start_id)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn the_handle_names_the_starts_task_and_host_before_the_start_ends() {
    // Given a start that has only named its task so far
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-b"));

    // When it is begun
    let handle = tokio::time::timeout(
        Duration::from_secs(5),
        begin_recorded_start(ledger.clone(), events),
    )
    .await
    .expect("the handle comes back without waiting for the start to end")
    .expect("the start named its task");

    // Then the handle names that task and host, and the start reads running
    assert_eq!(
        (handle.task_id.as_str(), handle.daemon_instance_id.as_str()),
        ("task-1", "host-b")
    );
    let record = ledger.read(&handle.start_id).expect("a record exists");
    assert_eq!(record.state, StartState::Running);
}

#[tokio::test]
async fn the_record_follows_the_starts_step_and_ends_with_its_session() {
    // Given a begun start
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-a"));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();

    // When it enters the worktree step
    start.send(phase(
        start_phase::Step::Worktree,
        start_phase::Boundary::Begin,
    ));

    // Then the record is in that step
    let in_worktree = eventually(&ledger, &handle.start_id, |r| r.step.is_some()).await;
    assert_eq!(
        in_worktree.step,
        Some(StartStep {
            step: "worktree".to_string(),
            status: "running".to_string()
        })
    );

    // And when it ends with a session
    start.send(phase(
        start_phase::Step::Worktree,
        start_phase::Boundary::End,
    ));
    start.send(result("session-9"));
    drop(start);

    // Then the record says so
    let done = eventually(&ledger, &handle.start_id, |r| {
        r.state != StartState::Running
    })
    .await;
    assert_eq!(done.state, StartState::Succeeded);
    assert_eq!(done.session_id.as_deref(), Some("session-9"));
    assert_eq!(done.error, None);
}

#[tokio::test]
async fn a_start_ending_cancelled_reads_cancelled() {
    // Given a begun start
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-a"));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();

    // When its stream ends CANCELLED
    start.fail(Status::cancelled("start cancelled"));

    // Then
    let done = eventually(&ledger, &handle.start_id, |r| {
        r.state != StartState::Running
    })
    .await;
    assert_eq!(done.state, StartState::Cancelled);
    assert_eq!(done.session_id, None);
}

#[tokio::test]
async fn a_start_failing_reads_failed_with_its_reason() {
    // Given a begun start
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-a"));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();

    // When its stream fails
    start.fail(Status::internal("git clone failed: repository not found"));

    // Then
    let done = eventually(&ledger, &handle.start_id, |r| {
        r.state != StartState::Running
    })
    .await;
    assert_eq!(done.state, StartState::Failed);
    assert_eq!(
        done.error.as_deref(),
        Some("git clone failed: repository not found")
    );
}

#[tokio::test]
async fn a_stream_that_ends_without_a_result_reads_failed_saying_so() {
    // Given a begun start
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-a"));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();

    // When its stream just ends
    drop(start);

    // Then it is not left running forever
    let done = eventually(&ledger, &handle.start_id, |r| {
        r.state != StartState::Running
    })
    .await;
    assert_eq!(done.state, StartState::Failed);
    assert!(done.error.unwrap().contains("ended without a result"));
}

#[tokio::test]
async fn a_start_refused_before_naming_its_task_is_the_callers_error() {
    // Given a start whose stream fails at once
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.fail(Status::invalid_argument("model is required for claude-cli"));

    // When it is begun
    let refused = begin_recorded_start(ledger, events).await;

    // Then the refusal is returned, not a handle
    assert_eq!(refused.unwrap_err(), "model is required for claude-cli");
}

#[tokio::test]
async fn the_record_outlives_the_ledger_that_wrote_it() {
    // Given a finished start
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-a"));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();
    start.send(result("session-9"));
    drop(start);
    eventually(&ledger, &handle.start_id, |r| {
        r.state == StartState::Succeeded
    })
    .await;

    // When a fresh ledger reads the same session directory
    let reread = StartLedger::in_session_dir(dir.path())
        .read(&handle.start_id)
        .unwrap();

    // Then it holds the same record
    assert_eq!(reread.state, StartState::Succeeded);
    assert_eq!(reread.session_id.as_deref(), Some("session-9"));
}

#[tokio::test]
async fn a_record_left_running_by_a_restart_reads_failed_naming_the_restart() {
    // Given a start whose stream is still open when the daemon goes away
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-a"));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();
    std::mem::forget(start);

    // When the restarted daemon recovers its ledger
    let recovered = StartLedger::in_session_dir(dir.path())
        .recover_interrupted()
        .unwrap();

    // Then that start reads failed, saying why
    assert_eq!(recovered, 1);
    let record = ledger.read(&handle.start_id).unwrap();
    assert_eq!(record.state, StartState::Failed);
    assert!(record.error.unwrap().contains("daemon restarted"));
}

#[test]
fn an_unknown_start_is_refused_naming_it() {
    // Given
    let dir = tempfile::tempdir().unwrap();

    // When
    let refused = StartLedger::in_session_dir(dir.path()).read("no-such-start");

    // Then
    assert!(refused.unwrap_err().to_string().contains("no-such-start"));
}
