//! One turn's transcript: the events a CLI streamed, mapped to the [`AcpUpdate`]s an ACP turn
//! produces, and the turn's final text and session id once the turn ends.

use std::process::ExitStatus;

use crate::backend::acp_driver::{AcpStopReason, AcpTurnEnd, AcpUpdate, AcpUpdateSink};
use crate::error::BackendError;

use super::codex_jsonl::CodexTurn;
use super::oz_ndjson::OzTurn;
use super::stream_json::StreamJsonTurn;
use super::HeadlessHarness;

/// How a turn's CLI process ended.
pub(super) enum Ending<'a> {
    /// The caller cancelled the turn and the process was killed.
    Cancelled,
    /// The process exited on its own; `stderr` is the tail it kept.
    Exited { status: ExitStatus, stderr: &'a str },
}

/// A turn's end: the session the CLI named during it, if any, and the turn's result.
pub(super) struct Finished {
    pub(super) session_id: Option<String>,
    pub(super) result: Result<AcpTurnEnd, BackendError>,
}

/// The sink a turn's updates go to, when the caller gave one.
pub(super) struct Updates(Option<AcpUpdateSink>);

impl Updates {
    pub(super) fn new(sink: Option<AcpUpdateSink>) -> Self {
        Self(sink)
    }

    pub(super) fn send(&self, update: AcpUpdate) {
        if let Some(sink) = &self.0 {
            sink(update);
        }
    }
}

/// The transcript of one turn, in the shape its harness streams.
pub(super) enum Transcript {
    StreamJson(StreamJsonTurn),
    Codex(CodexTurn),
    Oz(OzTurn),
}

impl Transcript {
    pub(super) fn new(harness: HeadlessHarness, sink: Option<AcpUpdateSink>) -> Self {
        let updates = Updates::new(sink);
        match harness {
            HeadlessHarness::Claude | HeadlessHarness::Cursor => {
                Self::StreamJson(StreamJsonTurn::new(harness, updates))
            }
            HeadlessHarness::Codex => Self::Codex(CodexTurn::new(updates)),
            HeadlessHarness::Oz => Self::Oz(OzTurn::new(updates)),
        }
    }

    /// One stdout line as it arrives.
    pub(super) fn push_line(&mut self, line: &str) {
        match self {
            Self::StreamJson(turn) => turn.push_line(line),
            Self::Codex(turn) => turn.push_line(line),
            Self::Oz(turn) => turn.push_line(line),
        }
    }

    pub(super) fn finish(self, ending: Ending<'_>) -> Finished {
        match self {
            Self::StreamJson(turn) => turn.finish(ending),
            Self::Codex(turn) => turn.finish(ending),
            Self::Oz(turn) => turn.finish(ending),
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
pub(super) fn exit_error(
    harness: HeadlessHarness,
    status: ExitStatus,
    detail: &str,
) -> BackendError {
    let detail = match detail.trim() {
        "" => "no error output",
        trimmed => trimmed,
    };
    BackendError::InvocationFailed(format!(
        "{} turn exited with {status}: {detail}",
        harness.id()
    ))
}
