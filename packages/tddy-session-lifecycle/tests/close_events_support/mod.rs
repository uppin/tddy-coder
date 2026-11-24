//! The events one `CloseSession` streamed, read to the stream's end, and what a test asks of them
//! (docs/ft/daemon/session-liveness.md § Closing a session) — and a directory a close cannot read.
//! Shared by the local close suite (through `tmux_agent_support`) and the target close suite.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use tddy_rpc::Status;
use tddy_service::proto::session::{
    close_session_event, CloseSessionEvent, CloseSessionOutcome, CloseSessionPreview,
    CloseSessionStarted, CloseSessionStep, CloseSessionStepKind, CloseSessionStepState,
};
use tddy_session_lifecycle::CloseSessionEventStream;

/// A safety net for a close to run to its end — a terminate's grace included — not a prediction
/// of how long one takes.
pub const CLOSE_RUNS: Duration = Duration::from_secs(60);

/// Every event one close streamed, in order.
#[derive(Debug)]
pub struct CloseEvents(pub Vec<CloseSessionEvent>);

impl CloseEvents {
    /// Read `stream` to its end, within [`CLOSE_RUNS`]. An error item fails the test.
    pub async fn read_to_end(stream: CloseSessionEventStream) -> Self {
        let items: Vec<Result<CloseSessionEvent, Status>> =
            tokio::time::timeout(CLOSE_RUNS, stream.collect())
                .await
                .expect("the close ends within its safety net");
        Self(
            items
                .into_iter()
                .map(|item| item.expect("a close streams no error"))
                .collect(),
        )
    }

    fn event(event: &CloseSessionEvent) -> &close_session_event::Event {
        event.event.as_ref().expect("every close event carries one")
    }

    /// The first event, which names the close's task.
    pub fn started(&self) -> &CloseSessionStarted {
        match self.0.first().map(Self::event) {
            Some(close_session_event::Event::Started(started)) => started,
            other => panic!("a close starts with `started`, not {other:?}"),
        }
    }

    /// The terminal `outcome`: the close ran to its end.
    pub fn outcome(&self) -> &CloseSessionOutcome {
        match self.0.last().map(Self::event) {
            Some(close_session_event::Event::Outcome(outcome)) => outcome,
            other => panic!("the close ends with an `outcome`, not {other:?}"),
        }
    }

    /// The terminal `needs_confirmation`: the close stopped before doing anything.
    pub fn needs_confirmation(&self) -> &CloseSessionPreview {
        match self.0.last().map(Self::event) {
            Some(close_session_event::Event::NeedsConfirmation(preview)) => preview,
            other => panic!("the close ends with `needs_confirmation`, not {other:?}"),
        }
    }

    /// How many terminal events — `needs_confirmation` or `outcome` — the close streamed.
    pub fn terminal_event_count(&self) -> usize {
        self.0
            .iter()
            .filter(|event| {
                matches!(
                    Self::event(event),
                    close_session_event::Event::NeedsConfirmation(_)
                        | close_session_event::Event::Outcome(_)
                )
            })
            .count()
    }

    /// Every step event the close streamed, in order.
    pub fn steps(&self) -> Vec<&CloseSessionStep> {
        self.0
            .iter()
            .filter_map(|event| match Self::event(event) {
                close_session_event::Event::Step(step) => Some(step),
                _ => None,
            })
            .collect()
    }

    /// The (kind, subject) of every step that ended done, in order.
    pub fn done_steps(&self) -> Vec<(CloseSessionStepKind, String)> {
        self.steps()
            .into_iter()
            .filter(|step| step.state == CloseSessionStepState::Done as i32)
            .map(|step| (step.kind(), step.subject.clone()))
            .collect()
    }

    /// Whether the close streamed any step of `kind`.
    pub fn streamed_a_step_of(&self, kind: CloseSessionStepKind) -> bool {
        self.steps().iter().any(|step| step.kind == kind as i32)
    }
}

/// The (kind, state) of each of `outcome`'s failed steps of `kind`, in order.
pub fn failed_steps_of(
    outcome: &CloseSessionOutcome,
    kind: CloseSessionStepKind,
) -> Vec<CloseSessionStepState> {
    outcome
        .failed_steps
        .iter()
        .filter(|step| step.kind == kind as i32)
        .map(|step| step.state())
        .collect()
}

/// Takes every permission off a directory until it is dropped, when they are given back — so a
/// test can make a directory unreadable without leaving one its temporary directory cannot remove.
pub struct UnreadableUntilDropped(pub PathBuf);

impl UnreadableUntilDropped {
    pub fn made_of(directory: PathBuf) -> Self {
        set_mode(&directory, 0o000);
        Self(directory)
    }
}

impl Drop for UnreadableUntilDropped {
    fn drop(&mut self) {
        set_mode(&self.0, 0o755);
    }
}

fn set_mode(directory: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|e| panic!("chmod {mode:o} {}: {e}", directory.display()));
}
