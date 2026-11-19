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
    start_phase, start_remote_call::Outcome, start_session_event::Event, StartPhase,
    StartRemoteCall, StartSessionEvent, StartSessionResponse, StartTaskStarted, StartWorktreeReady,
};
use tddy_tool_engine::start_ledger::{
    begin_recorded_start, RemoteCallOutcome, StartLedger, StartLedgerRecord, StartState, StartStep,
    MAX_REMOTE_CALLS,
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

// ── the remote-call log (`#coder-start` 8/10) ────────────────────────────────────────────────

/// A call the start made on `remote-dev`: `command` exited `exit_code`, leaving `stderr` behind.
fn a_call(command: &str, exit_code: i32, stderr: &str) -> Event {
    Event::RemoteCall(StartRemoteCall {
        host: "ssh target remote-dev".to_string(),
        command: command.to_string(),
        duration_ms: 12,
        outcome: Outcome::Exited as i32,
        exit_code,
        stderr_tail: stderr.to_string(),
    })
}

fn the_worktree_was_cut(branch: &str, worktree: &str) -> Event {
    Event::WorktreeReady(StartWorktreeReady {
        branch: branch.to_string(),
        worktree: worktree.to_string(),
    })
}

/// A begun start in a fresh ledger, and the handle's id.
async fn a_begun_start() -> (tempfile::TempDir, StartLedger, ScriptedStart, String) {
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (start, events) = a_scripted_start();
    start.send(started("task-1", "host-a"));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();
    (dir, ledger, start, handle.start_id)
}

#[tokio::test]
async fn remote_calls_are_folded_into_the_start_record_in_order() {
    // Given a begun start
    let (_dir, ledger, start, start_id) = a_begun_start().await;

    // When it reports a call that worked, one that failed, and one that never reached the target
    start.send(a_call("test -d /srv/repo/.git", 0, ""));
    start.send(a_call(
        "git -C /srv/repo fetch --all",
        128,
        "fatal: could not read from remote repository",
    ));
    start.send(Event::RemoteCall(StartRemoteCall {
        host: "ssh target remote-dev".to_string(),
        command: "git -C /srv/repo worktree add".to_string(),
        duration_ms: 3,
        outcome: Outcome::Unreachable as i32,
        exit_code: 0,
        stderr_tail: "Connection refused".to_string(),
    }));

    // Then the record lists the calls oldest first, with their outcomes and stderr tails
    let record = eventually(&ledger, &start_id, |r| r.remote_calls.len() == 3).await;
    let seen: Vec<_> = record
        .remote_calls
        .iter()
        .map(|call| {
            (
                call.command.as_str(),
                call.outcome,
                call.exit_code,
                call.stderr_tail.as_str(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (
                "test -d /srv/repo/.git",
                RemoteCallOutcome::Exited,
                Some(0),
                ""
            ),
            (
                "git -C /srv/repo fetch --all",
                RemoteCallOutcome::Exited,
                Some(128),
                "fatal: could not read from remote repository"
            ),
            (
                "git -C /srv/repo worktree add",
                RemoteCallOutcome::Unreachable,
                None,
                "Connection refused"
            ),
        ]
    );
    assert_eq!(record.remote_calls[0].host, "ssh target remote-dev");
    assert_eq!(record.remote_calls[0].duration_ms, 12);
}

#[tokio::test]
async fn the_log_is_bounded_and_keeps_the_newest_calls() {
    // Given a begun start
    let (_dir, ledger, start, start_id) = a_begun_start().await;

    // When it reports two more calls than the log holds
    let total = MAX_REMOTE_CALLS + 2;
    for n in 0..total {
        start.send(a_call(&format!("call {n}"), 0, ""));
    }
    let last = format!("call {}", total - 1);

    // Then the record holds the newest MAX_REMOTE_CALLS, the oldest dropped
    let record = eventually(&ledger, &start_id, |r| {
        r.remote_calls
            .last()
            .is_some_and(|call| call.command == last)
    })
    .await;
    assert_eq!(record.remote_calls.len(), MAX_REMOTE_CALLS);
    assert_eq!(record.remote_calls[0].command, "call 2");
}

#[tokio::test]
async fn a_failed_start_keeps_the_branch_and_worktree_it_cut() {
    // Given a begun start that cut its worktree
    let (_dir, ledger, start, start_id) = a_begun_start().await;
    start.send(the_worktree_was_cut(
        "feat/x",
        "/home/dev/.tddy/repos/repo/.worktrees/s-1",
    ));
    eventually(&ledger, &start_id, |r| r.branch.is_some()).await;

    // When the start then fails
    start.fail(Status::internal("the agent could not be launched"));

    // Then the failed record still names the branch and worktree it left behind
    let done = eventually(&ledger, &start_id, |r| r.state == StartState::Failed).await;
    assert_eq!(
        (done.branch.as_deref(), done.worktree.as_deref()),
        (
            Some("feat/x"),
            Some("/home/dev/.tddy/repos/repo/.worktrees/s-1")
        )
    );
}

#[tokio::test]
async fn a_succeeded_start_whose_session_is_not_listed_keeps_the_branch_and_worktree_it_cut() {
    // Given a begun start that cut its worktree, on a ledger with no session listing to ask
    let (_dir, ledger, start, start_id) = a_begun_start().await;
    start.send(the_worktree_was_cut(
        "feat/x",
        "/home/dev/.tddy/repos/repo/.worktrees/s-1",
    ));
    eventually(&ledger, &start_id, |r| r.branch.is_some()).await;

    // When the start succeeds
    start.send(result("s-1"));

    // Then the record still names the branch and worktree the start reported
    let done = eventually(&ledger, &start_id, |r| r.state == StartState::Succeeded).await;
    assert_eq!(
        (done.branch.as_deref(), done.worktree.as_deref()),
        (
            Some("feat/x"),
            Some("/home/dev/.tddy/repos/repo/.worktrees/s-1")
        )
    );
}

#[tokio::test]
async fn every_remote_call_advances_updated_at() {
    // Given a begun start
    let (_dir, ledger, start, start_id) = a_begun_start().await;
    let begun_at = ledger.read(&start_id).unwrap().updated_at_unix_ms;

    // When it reports one call, and later another
    tokio::time::sleep(Duration::from_millis(15)).await;
    start.send(a_call("first", 0, ""));
    let after_first = eventually(&ledger, &start_id, |r| r.updated_at_unix_ms > begun_at)
        .await
        .updated_at_unix_ms;
    tokio::time::sleep(Duration::from_millis(15)).await;
    start.send(a_call("second", 0, ""));

    // Then each moved the record's clock
    eventually(&ledger, &start_id, |r| r.updated_at_unix_ms > after_first).await;
}

#[tokio::test]
async fn a_credential_in_a_reported_call_is_scrubbed_before_it_is_recorded() {
    // Given a begun start
    let (_dir, ledger, start, start_id) = a_begun_start().await;

    // When a call reports a URL with a password in its command and in its stderr
    start.send(a_call(
        "git clone https://bot:s3cret-token@example.com/acme/repo.git",
        128,
        "fatal: unable to access 'https://bot:s3cret-token@example.com/acme/repo.git/'",
    ));

    // Then the record holds neither
    let record = eventually(&ledger, &start_id, |r| !r.remote_calls.is_empty()).await;
    let shown = serde_json::to_string(&record.remote_calls).unwrap();
    assert!(!shown.contains("s3cret-token"), "{shown}");
}
