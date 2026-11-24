use crate::connection_service::svc_close_session::close_task::CloseRun;
use bytes::Bytes;
use tddy_service::proto::session::{
    close_session_event, CloseSessionEvent, CloseSessionStep, CloseSessionStepKind,
    CloseSessionStepState,
};

impl CloseRun {
    /// Stream one step, write it to the progress channel, and keep it when it failed or was skipped.
    pub(super) fn step(
        &mut self,
        kind: CloseSessionStepKind,
        subject: &str,
        host: &str,
        state: CloseSessionStepState,
        message: &str,
    ) {
        let step = CloseSessionStep {
            kind: kind as i32,
            subject: subject.to_string(),
            state: state as i32,
            message: message.to_string(),
            host: host.to_string(),
        };
        self.note(&progress_line(&step));
        if matches!(
            state,
            CloseSessionStepState::Failed | CloseSessionStepState::Skipped
        ) {
            self.failed.push(step.clone());
        }
        self.send(close_session_event::Event::Step(step));
    }

    /// Write `line` to the task's progress channel.
    pub(super) fn note(&self, line: &str) {
        self.progress.write(Bytes::from(format!("{line}\n")));
    }

    pub(super) fn send(&self, event: close_session_event::Event) {
        // A caller that went away does not stop the close: it runs to its end, and its task
        // records how it went.
        let _ = self
            .events
            .send(Ok(CloseSessionEvent { event: Some(event) }));
    }
}

/// One step as a line of the task's progress output.
fn progress_line(step: &CloseSessionStep) -> String {
    let kind = match step.kind() {
        CloseSessionStepKind::Check => "check",
        CloseSessionStepKind::Terminate => "terminate",
        CloseSessionStepKind::Remove => "remove",
        CloseSessionStepKind::Record => "record",
        CloseSessionStepKind::Unspecified => "step",
    };
    let state = match step.state() {
        CloseSessionStepState::Running => "running",
        CloseSessionStepState::Done => "done",
        CloseSessionStepState::Failed => "failed",
        CloseSessionStepState::Skipped => "skipped",
        CloseSessionStepState::Unspecified => "unknown",
    };
    let host = if step.host.is_empty() {
        String::new()
    } else {
        format!(" on {}", step.host)
    };
    let message = if step.message.is_empty() {
        String::new()
    } else {
        format!(" — {}", step.message)
    };
    format!("{kind} {}{host}: {state}{message}", step.subject)
}
