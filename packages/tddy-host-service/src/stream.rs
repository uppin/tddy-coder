//! The two server-stream adapters `host.HostService` hands back.
//!
//! They differ in more than their item type, and the difference is the point. `StreamHostStats`
//! emits unconditionally on a timer, so its channel carries bare events. `StreamHostPrompts` is
//! **silent almost all the time** — a host raises a prompt only when an operator starts an add-key
//! flow — and it honours `daemon_instance_id`, so a feed served by a peer arrives as the
//! frames-or-status channel the forwarder hands back. Hence `Result` items on one and not the
//! other.

use std::pin::Pin;
use std::task::{Context, Poll};

use tddy_rpc::Status;
use tddy_service::proto::host::{HostPromptEvent, HostStatsEvent};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::Stream;

/// Stream adapter backed by an mpsc channel for [`HostStatsEvent`] server-streaming.
#[derive(Debug)]
pub struct MpscHostStatsStream {
    pub(crate) rx: UnboundedReceiver<HostStatsEvent>,
}

impl Stream for MpscHostStatsStream {
    type Item = Result<HostStatsEvent, Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(Some(Ok(event))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Unpin for MpscHostStatsStream {}

/// Stream adapter for [`HostPromptEvent`] server-streaming.
///
/// ⚠ Unlike [`MpscHostStatsStream`], the feed behind this one is **silent almost all the time**.
/// Per `packages/tddy-codegen/docs/server-streaming.md`, a handler whose stream can be silent must
/// `tokio::select!` on `tx.closed()` as well as breaking on a send error, or its task leaks one per
/// subscription forever. `stream_host_stats` escapes that only because it emits unconditionally.
pub struct MpscHostPromptStream {
    pub(crate) rx: UnboundedReceiver<Result<HostPromptEvent, Status>>,
}

impl Stream for MpscHostPromptStream {
    type Item = Result<HostPromptEvent, Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Unpin for MpscHostPromptStream {}
