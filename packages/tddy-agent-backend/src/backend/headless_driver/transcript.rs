//! One turn's transcript: the events a CLI streamed, mapped to the [`AcpUpdate`]s an ACP turn
//! produces, and the turn's final text and session id once the turn ends.

use std::process::ExitStatus;

use crate::backend::acp_driver::{AcpStopReason, AcpTurnEnd, AcpUpdate, AcpUpdateSink};
use crate::error::BackendError;

pub(crate) use crate::adapters::{TurnEnding as Ending, TurnFinished as Finished};

/// The sink a turn's updates go to, when the caller gave one.
pub(crate) struct Updates(Option<AcpUpdateSink>);

impl Updates {
    pub(crate) fn new(sink: Option<AcpUpdateSink>) -> Self {
        Self(sink)
    }

    pub(super) fn send(&self, update: AcpUpdate) {
        if let Some(sink) = &self.0 {
            sink(update);
        }
    }
}

/// A turn that ended on its own with `text` as its reply.
pub(super) fn ended(text: String) -> Result<AcpTurnEnd, BackendError> {
    Ok(AcpTurnEnd {
        stop_reason: AcpStopReason::EndTurn,
        text,
    })
}

/// A turn the caller cancelled, with the text that had streamed by then.
pub(super) fn cancelled(text: String) -> Result<AcpTurnEnd, BackendError> {
    Ok(AcpTurnEnd {
        stop_reason: AcpStopReason::Cancelled,
        text,
    })
}

/// A CLI that exited non-zero without a usable result: its status and the detail it left.
pub(super) fn exit_error(agent: &str, status: ExitStatus, detail: &str) -> BackendError {
    let detail = match detail.trim() {
        "" => "no error output",
        trimmed => trimmed,
    };
    BackendError::InvocationFailed(format!("{agent} turn exited with {status}: {detail}"))
}
