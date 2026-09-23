//! `StreamExecuteTool`'s framing: how a completed tool result is split into bounded frames.
//!
//! Moved from `tddy-session-lifecycle`'s `connection_service.rs` with the exec-tool handlers, its
//! only caller.

use tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;
use tddy_service::proto::exec_tools::{ExecuteToolChunk, ExecuteToolResponse};

/// Bytes of tool result carried per `StreamExecuteTool` frame.
///
/// Defined *as* [`HOST_DOCUMENT_FRAME_BYTES`] rather than as the same number, because the budget is
/// a property of the transport rather than of what rides on it: both are what every transport in the
/// stack carries per message without applying its own chunk framing. Two constants free to drift
/// would be two answers to one question, and only one of them could be right.
pub const EXEC_TOOL_FRAME_BYTES: usize = HOST_DOCUMENT_FRAME_BYTES;

/// Split a completed tool result into ordered [`EXEC_TOOL_FRAME_BYTES`] frames.
///
/// The outcome rides the **final** frame — a tool error is a result, not an RPC failure, matching
/// unary `ExecuteTool`'s contract. An empty result still yields exactly one frame, so a consumer
/// never has to tell "empty result" from "stream produced nothing", and a stream ending without a
/// `last` frame is unambiguously a truncation.
pub(super) fn exec_tool_result_frames(response: ExecuteToolResponse) -> Vec<ExecuteToolChunk> {
    let bytes = response.result_json.into_bytes();
    let mut frames: Vec<ExecuteToolChunk> = bytes
        .chunks(EXEC_TOOL_FRAME_BYTES)
        .map(|chunk| ExecuteToolChunk {
            result_chunk: chunk.to_vec(),
            ..Default::default()
        })
        .collect();
    if frames.is_empty() {
        frames.push(ExecuteToolChunk::default());
    }
    let last = frames.last_mut().expect("at least one frame");
    last.is_error = response.is_error;
    last.error_message = response.error_message;
    last.job_id = response.job_id;
    last.job_running = response.job_running;
    last.last = true;
    frames
}
