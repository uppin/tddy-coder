//! A session start that outlives `session_start_timeout_secs` is cancelled, leaves nothing behind
//! and fails `DEADLINE_EXCEEDED` naming the step it was on — nobody waits on it forever.
//!
//! The start here is a stand-in that reports its phases as a real one does
//! (`AttachmentProgressSink::begin_phase`) and then hangs, or finishes late. The clock is paused,
//! so the deadlines cost no wall time; every test is itself bounded by [`A_START_THAT_HANGS`].
//!
//! Feature docs: docs/ft/daemon/session-agent-roster.md (A start has a deadline)

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_agent_launch::start_deadline::StartDeadline;
use tddy_rpc::{Code, Status};
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::start_session_event::Event;
use tddy_service::proto::session::StartSessionEvent;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_task::CancellationToken;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

const DEADLINE: Duration = Duration::from_secs(600);
const GRACE: Duration = Duration::from_secs(30);

/// Past this the start is not bounded at all.
const A_START_THAT_HANGS: Duration = Duration::from_secs(3600);

fn a_start_deadline() -> StartDeadline {
    StartDeadline::new(DEADLINE).with_cancel_grace(GRACE)
}

fn a_streaming_sink() -> (
    AttachmentProgressSink,
    UnboundedReceiver<Result<StartSessionEvent, Status>>,
) {
    let (tx, rx) = unbounded_channel();
    (AttachmentProgressSink::streaming(tx), rx)
}

fn drained(rx: &mut UnboundedReceiver<Result<StartSessionEvent, Status>>) -> Vec<Event> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .map(|item| {
            item.expect("an event, not a status")
                .event
                .expect("a variant")
        })
        .collect()
}

/// A start that begins `step` and never returns, cancelled or not.
async fn a_start_stuck_in(step: StartStep, progress: AttachmentProgressSink) -> Result<(), Status> {
    progress.begin_phase(StartStep::Clone);
    progress.end_phase(StartStep::Clone);
    progress.begin_phase(step);
    std::future::pending().await
}

/// A start that creates its session directory, then spends `takes` launching its agent without
/// looking at the cancel, and returns the session it created.
async fn a_start_that_creates_its_session_and_finishes_after(
    takes: Duration,
    session_dir: PathBuf,
    progress: AttachmentProgressSink,
) -> Result<PathBuf, Status> {
    progress.begin_phase(StartStep::Worktree);
    std::fs::create_dir_all(&session_dir).expect("the session directory is created");
    progress.end_phase(StartStep::Worktree);
    progress.begin_phase(StartStep::Agent);
    tokio::time::sleep(takes).await;
    progress.end_phase(StartStep::Agent);
    Ok(session_dir)
}

async fn nothing_to_undo<T>(_: T) {}

/// What undoing a created session does: what the cancelled-start cleanup does to it.
fn removing_the_session(undone: Arc<AtomicBool>) -> impl FnOnce(PathBuf) -> std::future::Ready<()> {
    move |session_dir: PathBuf| {
        std::fs::remove_dir_all(&session_dir).expect("the session directory is removed");
        undone.store(true, Ordering::SeqCst);
        std::future::ready(())
    }
}

fn a_deadline_exceeded(result: Result<impl std::fmt::Debug, Status>) -> String {
    let status = result.expect_err("a start past its deadline does not succeed");
    assert_eq!(status.code, Code::DeadlineExceeded, "{status:?}");
    status.message
}

fn session_dir_in(dir: &Path) -> PathBuf {
    dir.join("sessions")
        .join("019a0000-0000-7000-8000-000000000001")
}

// -- acceptance ---------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn a_start_whose_step_never_returns_fails_deadline_exceeded_naming_the_step() {
    // Given a start that cut nothing yet and is stuck cutting its worktree
    let (sink, _rx) = a_streaming_sink();
    let cancel = CancellationToken::new();

    // When it runs under a 600 s start deadline
    let ended = tokio::time::timeout(
        A_START_THAT_HANGS,
        a_start_deadline().run(
            &sink,
            &cancel,
            |progress| a_start_stuck_in(StartStep::Worktree, progress),
            nothing_to_undo,
        ),
    )
    .await
    .expect("the start waited on a step that never returns with no deadline of its own");

    // Then it failed DEADLINE_EXCEEDED, naming the worktree step and the deadline, and was cancelled
    let message = a_deadline_exceeded(ended);
    assert!(message.contains("worktree"), "{message}");
    assert!(message.contains("600"), "{message}");
    assert!(
        cancel.is_cancelled(),
        "the start's own token was not cancelled"
    );
}

#[tokio::test(start_paused = true)]
async fn a_timed_out_start_leaves_no_partial_session() {
    // Given a start that creates its session, then takes 620 s launching its agent — past the
    // 600 s deadline but inside the 30 s grace
    let dir = tempfile::tempdir().expect("a temp dir");
    let session_dir = session_dir_in(dir.path());
    let (sink, _rx) = a_streaming_sink();
    let undone = Arc::new(AtomicBool::new(false));

    // When it runs under the deadline
    let ended = tokio::time::timeout(
        A_START_THAT_HANGS,
        a_start_deadline().run(
            &sink,
            &CancellationToken::new(),
            |progress| {
                a_start_that_creates_its_session_and_finishes_after(
                    Duration::from_secs(620),
                    session_dir.clone(),
                    progress,
                )
            },
            removing_the_session(Arc::clone(&undone)),
        ),
    )
    .await
    .expect("the start ran past its deadline with no deadline of its own");

    // Then the start failed DEADLINE_EXCEEDED naming the agent step, and the session it created
    // late was undone
    let message = a_deadline_exceeded(ended);
    assert!(message.contains("agent"), "{message}");
    assert!(
        undone.load(Ordering::SeqCst),
        "the late session was not undone"
    );
    assert!(
        !session_dir.exists(),
        "a timed-out start left {} behind",
        session_dir.display()
    );
}

// -- unit ---------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn the_running_step_is_named_when_the_caller_reports_nowhere() {
    // Given a unary start (its progress goes nowhere) stuck launching its agent
    let sink = AttachmentProgressSink::discarding();

    // When it runs under the deadline
    let ended = tokio::time::timeout(
        A_START_THAT_HANGS,
        a_start_deadline().run(
            &sink,
            &CancellationToken::new(),
            |progress| a_start_stuck_in(StartStep::Agent, progress),
            nothing_to_undo,
        ),
    )
    .await
    .expect("the unary start hung with no deadline of its own");

    // Then the step is still named: the deadline tracks it itself, not through the caller's stream
    let message = a_deadline_exceeded(ended);
    assert!(message.contains("agent"), "{message}");
}

#[tokio::test(start_paused = true)]
async fn a_start_that_stops_when_cancelled_by_its_deadline_fails_deadline_exceeded_not_cancelled() {
    // Given a start stuck cloning that stops as soon as its token is cancelled
    let (sink, _rx) = a_streaming_sink();
    let cancel = CancellationToken::new();
    let watched = cancel.clone();

    // When it runs under the deadline
    let ended = tokio::time::timeout(
        A_START_THAT_HANGS,
        a_start_deadline().run(
            &sink,
            &cancel,
            |progress| async move {
                progress.begin_phase(StartStep::Clone);
                watched.cancelled().await;
                Err::<(), _>(Status::cancelled("start cancelled"))
            },
            nothing_to_undo,
        ),
    )
    .await
    .expect("the start hung with no deadline of its own");

    // Then the caller is told the start ran out of time, not that somebody cancelled it
    let message = a_deadline_exceeded(ended);
    assert!(message.contains("clone"), "{message}");
}

#[tokio::test(start_paused = true)]
async fn a_start_inside_its_deadline_returns_its_own_result_uncancelled() {
    // Guard: a start that finishes in time is untouched
    let (sink, _rx) = a_streaming_sink();
    let cancel = CancellationToken::new();

    let ended = a_start_deadline()
        .run(
            &sink,
            &cancel,
            |_progress| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok::<_, Status>("started")
            },
            nothing_to_undo,
        )
        .await;

    assert_eq!(ended.expect("the start succeeded"), "started");
    assert!(!cancel.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_start_inside_its_deadline_fails_with_its_own_error() {
    // Guard: the deadline does not rewrite a start's own refusal
    let (sink, _rx) = a_streaming_sink();

    let ended = a_start_deadline()
        .run(
            &sink,
            &CancellationToken::new(),
            |_progress| async { Err::<(), _>(Status::not_found("no such project")) },
            nothing_to_undo,
        )
        .await;

    let status = ended.expect_err("the start's own error");
    assert_eq!(status.code, Code::NotFound);
}

#[tokio::test(start_paused = true)]
async fn the_starts_progress_still_reaches_the_caller() {
    // Guard: the phases the start reports reach the caller's stream unchanged
    let (sink, mut rx) = a_streaming_sink();

    a_start_deadline()
        .run(
            &sink,
            &CancellationToken::new(),
            |progress| async move {
                progress.begin_phase(StartStep::Worktree);
                progress.end_phase(StartStep::Worktree);
                Ok::<_, Status>(())
            },
            nothing_to_undo,
        )
        .await
        .expect("the start succeeded");

    let phases: Vec<i32> = drained(&mut rx)
        .into_iter()
        .map(|event| match event {
            Event::Phase(phase) => phase.step,
            other => panic!("expected only phases, got {other:?}"),
        })
        .collect();
    assert_eq!(
        phases,
        vec![StartStep::Worktree as i32, StartStep::Worktree as i32]
    );
}
