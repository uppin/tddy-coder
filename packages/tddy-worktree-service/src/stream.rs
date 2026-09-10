//! The two server-stream adapters `worktree.WorktreeService` hands back, and the framing one of
//! them repeats.
//!
//! They carry different item types for a reason. `StreamWorktreeStats` produces events the service
//! itself built, so a failure there has already been answered as a `Status` before the stream
//! exists — bare events suffice. `StreamReadWorktreeFile` reads a file, and a read that fails
//! part-way has to reach the caller *as an error* rather than as a short file, so its items are
//! `Result`s.

use std::pin::Pin;
use std::task::{Context, Poll};

use tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;
use tddy_rpc::Status;
use tddy_service::proto::worktree::{WorktreeFileChunk, WorktreeStatsEvent};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::Stream;

/// Stream adapter backed by an mpsc channel for [`WorktreeStatsEvent`] server-streaming. The first
/// event carries a full snapshot; each subsequent event carries one worktree's updated size row.
#[derive(Debug)]
pub struct MpscWorktreeStatsStream {
    rx: UnboundedReceiver<WorktreeStatsEvent>,
}

impl From<UnboundedReceiver<WorktreeStatsEvent>> for MpscWorktreeStatsStream {
    fn from(rx: UnboundedReceiver<WorktreeStatsEvent>) -> Self {
        Self { rx }
    }
}

impl Stream for MpscWorktreeStatsStream {
    type Item = Result<WorktreeStatsEvent, Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(Some(Ok(event))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Unpin for MpscWorktreeStatsStream {}

/// Stream adapter backed by an unbounded mpsc channel carrying `Result<T, Status>` items — for a
/// server-streaming RPC whose frames may carry a mid-stream status.
pub struct MpscResultStream<T> {
    rx: UnboundedReceiver<Result<T, Status>>,
}

impl<T> From<UnboundedReceiver<Result<T, Status>>> for MpscResultStream<T> {
    fn from(rx: UnboundedReceiver<Result<T, Status>>) -> Self {
        Self { rx }
    }
}

impl<T> Stream for MpscResultStream<T> {
    type Item = Result<T, Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl<T> Unpin for MpscResultStream<T> {}

/// Opaque by design: a stream's pending items are not inspectable without consuming them, so this
/// only names the adapter — enough for a `Result::expect_err` message on a handler that returns it.
impl<T> std::fmt::Debug for MpscResultStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MpscResultStream")
    }
}

/// Split a worktree file's bytes into ordered [`HOST_DOCUMENT_FRAME_BYTES`] frames, stamping
/// `total_byte_size` on every one.
///
/// A zero-byte file still yields exactly **one** (empty) frame, so "the file is empty" stays
/// distinguishable from "the stream produced nothing" — AC18. The size is repeated on every frame
/// rather than sent as a header: a reader knows the total from the first frame with no header frame
/// to special-case, and a one-frame file is not a different shape from a hundred-frame one.
///
/// The bytes are already in memory by the time this runs, because the reader that produced them is
/// also the thing that applies the cap: over-cap is refused before any frame exists, so nothing here
/// can be a partial file.
#[must_use]
pub fn worktree_file_frames(bytes: &[u8]) -> Vec<WorktreeFileChunk> {
    let total_byte_size = bytes.len() as u64;
    let mut frames: Vec<WorktreeFileChunk> = bytes
        .chunks(HOST_DOCUMENT_FRAME_BYTES)
        .map(|chunk| WorktreeFileChunk {
            data: chunk.to_vec(),
            total_byte_size,
        })
        .collect();
    if frames.is_empty() {
        frames.push(WorktreeFileChunk {
            data: Vec::new(),
            total_byte_size,
        });
    }
    frames
}
