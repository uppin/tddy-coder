//! The deadline a whole session start runs under (`session_start_timeout_secs`, 600 s by default).
//!
//! Every wait inside a start has a bound of its own — a supervisor call, a clone's progress, an ACP
//! `open_session` — but a start strings many of them together, and some steps (a worktree cut, an
//! index build) have none. [`StartDeadline`] bounds the whole: when it expires the start's
//! cancellation token is cancelled, exactly as a caller's cancel would, and the start is given
//! [`START_CANCEL_GRACE`] to stop at its next checkpoint. A start that comes back with a session
//! inside the grace has that session undone by the caller's `undo`; one that does not come back is
//! dropped. Either way the caller gets `DEADLINE_EXCEEDED` naming the step ([`StartStep`]) the start
//! had begun and not ended when the deadline expired, as its progress reported it.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-11-06-bounded-session-waits.md

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tddy_rpc::Status;
use tddy_service::proto::session::start_phase::{Boundary, Step as StartStep};
use tddy_service::proto::session::start_session_event::Event;
use tddy_service::proto::session::{StartPhase, StartSessionEvent};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_task::CancellationToken;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::Instant;

/// How long a start whose deadline expired is given, once cancelled, to come back with what it
/// created so it can be undone. A start still running after this is dropped.
pub const START_CANCEL_GRACE: Duration = Duration::from_secs(30);

/// The bound one session start runs under.
#[derive(Debug, Clone, Copy)]
pub struct StartDeadline {
    deadline: Duration,
    cancel_grace: Duration,
}

impl StartDeadline {
    /// A start bounded by `deadline` (the daemon passes `DaemonConfig::session_start_timeout`),
    /// with [`START_CANCEL_GRACE`] to come back once cancelled.
    pub fn new(deadline: Duration) -> StartDeadline {
        StartDeadline {
            deadline,
            cancel_grace: START_CANCEL_GRACE,
        }
    }

    /// This deadline with `grace` instead of [`START_CANCEL_GRACE`].
    pub fn with_cancel_grace(mut self, grace: Duration) -> StartDeadline {
        self.cancel_grace = grace;
        self
    }

    /// Run `start` under this deadline.
    ///
    /// `start` is handed the sink it reports its progress to: every event reaches `progress`
    /// unchanged, and the phases it begins and ends are how the deadline knows which step was
    /// running — also when `progress` reports nowhere (the unary start). `cancel` is the start's own
    /// token, cancelled when the deadline expires. `undo` is run on a start that returned a session
    /// during the grace after its deadline expired, so the timed-out start leaves nothing behind.
    ///
    /// Returns `start`'s own result when it finished in time; otherwise `DEADLINE_EXCEEDED` naming
    /// the deadline and the running step. Every event the start reported is relayed before this
    /// returns.
    pub async fn run<T, S, SF, U, UF>(
        &self,
        progress: &AttachmentProgressSink,
        cancel: &CancellationToken,
        start: S,
        undo: U,
    ) -> Result<T, Status>
    where
        S: FnOnce(AttachmentProgressSink) -> SF,
        SF: Future<Output = Result<T, Status>>,
        U: FnOnce(T) -> UF,
        UF: Future<Output = ()>,
    {
        let (events, received) = tokio::sync::mpsc::unbounded_channel();
        let mut relay = Relay {
            received,
            progress,
            running: None,
        };
        // Boxed so that dropping a start that ignores its cancel is an explicit `drop` below.
        let mut start = Box::pin(start(AttachmentProgressSink::streaming(events)));

        let expires_at = Instant::now() + self.deadline;
        let finished = run_until(start.as_mut(), expires_at, &mut relay).await;
        relay.drain();
        if let Some(result) = finished {
            return result;
        }

        // The step is read at expiry: what the start does during the grace is not what it was doing
        // when the deadline hit.
        let running = relay.running;
        cancel.cancel();
        let grace_ends_at = Instant::now() + self.cancel_grace;
        match run_until(start.as_mut(), grace_ends_at, &mut relay).await {
            Some(Ok(session)) => undo(session).await,
            Some(Err(status)) => {
                log::debug!("a start's own refusal after its deadline is not reported: {status:?}")
            }
            None => drop(start),
        }
        relay.drain();
        Err(deadline_exceeded(self.deadline, running))
    }
}

/// Polls `start` until it returns (`Some`) or `until` passes (`None`), relaying what it reports as
/// it goes. The start is polled in the calling task, so the events it reported before either
/// outcome are on the relay's channel and the phase the relay tracks is current when this returns.
async fn run_until<T, SF>(
    mut start: Pin<&mut SF>,
    until: Instant,
    relay: &mut Relay<'_>,
) -> Option<Result<T, Status>>
where
    SF: Future<Output = Result<T, Status>>,
{
    let mut timer = std::pin::pin!(tokio::time::sleep_until(until));
    loop {
        tokio::select! {
            biased;
            result = start.as_mut() => return Some(result),
            Some(item) = relay.received.recv() => relay.forward(item),
            () = timer.as_mut() => return None,
        }
    }
}

/// Carries a start's events to the caller's sink and tracks the step the start is on: the one it
/// has begun and not yet ended.
///
/// The events are read from the channel by the task that runs the start, not by a forwarding task,
/// so the step named when the deadline expires is exact. A forwarding task would lag the timer by
/// however long it took to be scheduled. The channel is never awaited to close: a leftover task
/// that still holds a sink clone can only send into a channel nobody reads once `run` returns.
struct Relay<'a> {
    received: UnboundedReceiver<Result<StartSessionEvent, Status>>,
    progress: &'a AttachmentProgressSink,
    running: Option<StartStep>,
}

impl Relay<'_> {
    /// Passes one event on, unchanged, and tracks its phase. A closed caller stream is ignored, as
    /// [`AttachmentProgressSink`] ignores it.
    fn forward(&mut self, item: Result<StartSessionEvent, Status>) {
        if let Ok(StartSessionEvent {
            event: Some(Event::Phase(phase)),
        }) = &item
        {
            self.track(phase);
        }
        if let Some(tx) = &self.progress.tx {
            let _ = tx.send(item);
        }
    }

    /// Relays everything the start has already reported, so none of it is still in flight when
    /// `run` returns.
    fn drain(&mut self) {
        while let Ok(item) = self.received.try_recv() {
            self.forward(item);
        }
    }

    fn track(&mut self, phase: &StartPhase) {
        let (Ok(step), Ok(boundary)) = (
            StartStep::try_from(phase.step),
            Boundary::try_from(phase.boundary),
        ) else {
            return;
        };
        match boundary {
            Boundary::Begin => self.running = Some(step),
            Boundary::End if self.running == Some(step) => self.running = None,
            _ => {}
        }
    }
}

/// `DEADLINE_EXCEEDED`, naming the deadline and the step the start was on when it expired.
fn deadline_exceeded(deadline: Duration, running: Option<StartStep>) -> Status {
    let on = match running {
        Some(step) => format!("it was on its {} step", step_name(step)),
        None => "no step was running".to_string(),
    };
    Status::deadline_exceeded(format!(
        "the session start did not finish within {deadline:?}; {on}"
    ))
}

/// The lowercase word for a step, as the daemon's messages name it: `STEP_WORKTREE` is `worktree`.
fn step_name(step: StartStep) -> String {
    let proto_name = step.as_str_name();
    proto_name
        .strip_prefix("STEP_")
        .unwrap_or(proto_name)
        .to_ascii_lowercase()
}
