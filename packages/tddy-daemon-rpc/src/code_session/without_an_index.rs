//! A code session on a worktree that has no code index: no `Cargo.toml` at its root, so it is not a
//! Rust worktree. Nothing is warmed or followed — the index daemon is not even dialled. The pane is
//! told once that the worktree has no index (`NONE`, with why), and every query is answered with an
//! `error` naming why, so a pane never waits on an index that will not come.

use tddy_rpc::Status;
use tddy_service::proto::code_navigation::code_query_answer::Result as Answered;
use tddy_service::proto::code_navigation::code_session_event::Event;
use tddy_service::proto::code_navigation::code_session_request::Frame;
use tddy_service::proto::code_navigation::{
    CodeIndexProgress, CodeIndexState, CodeQueryAnswer, CodeSessionRequest,
};
use tokio_stream::StreamExt;

use super::{misplaced_frame, send, Events};

/// What every query on a worktree without an index is answered.
pub(super) const NOT_A_RUST_WORKTREE: &str =
    "this worktree has no Cargo.toml at its root, so it is not a Rust worktree and has no code index";

/// Serves the session until the pane leaves: one `NONE` progress, then an `error` for each query.
pub(super) async fn serve(
    mut frames: tddy_rpc::Streaming<CodeSessionRequest>,
    events: &Events,
) -> Result<(), Status> {
    let none = CodeIndexProgress {
        state: CodeIndexState::None as i32,
        error: NOT_A_RUST_WORKTREE.to_string(),
        ..Default::default()
    };
    send(events, Event::Progress(none)).await?;
    loop {
        let frame = tokio::select! {
            frame = frames.next() => frame,
            () = events.closed() => return Ok(()),
        };
        let Some(frame) = frame else {
            return Ok(());
        };
        serve_frame(frame?, events).await?;
    }
}

/// One frame: a query is answered with why there is no index, a cancel is a no-op, and a frame with
/// no place in an open session is refused.
async fn serve_frame(frame: CodeSessionRequest, events: &Events) -> Result<(), Status> {
    match frame.frame {
        Some(Frame::Query(query)) => {
            let answer = CodeQueryAnswer {
                request_id: query.request_id,
                result: Some(Answered::Error(NOT_A_RUST_WORKTREE.to_string())),
            };
            send(events, Event::Answer(answer)).await
        }
        // Nothing is ever in flight, so there is nothing to cancel.
        Some(Frame::Cancel(_)) => Ok(()),
        misplaced @ (Some(Frame::Open(_)) | None) => Err(misplaced_frame(&misplaced)),
    }
}
