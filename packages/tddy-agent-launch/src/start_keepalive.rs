//! Keeping a streamed start's stream alive while it waits on slow provisioning.
//!
//! A forwarded `StreamStartSession` is ended by the relay after
//! `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30 s) without a frame. Provisioning a project can be quiet
//! for far longer — a clone waits up to two minutes for the operator to type an SSH passphrase — so
//! the host speaks up on a timer instead of the timeout being widened: a dead peer is still noticed
//! within 30 s.

use std::future::Future;
use std::time::Duration;

use tddy_session_files::attachment_progress::AttachmentProgressSink;

/// How often a waiting start emits a keep-alive. Well under the relay's 30 s idle timeout, so one
/// late tick does not cost the stream.
pub const START_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

/// What a keep-alive says the host is waiting on while a project is provisioned.
pub const PROVISIONING_ACTIVITY: &str = "provisioning";

/// What a keep-alive says the host is waiting on for the rest of a start: a worktree cut, an index,
/// an agent launch — anything after provisioning that can be quiet for longer than the relay allows.
pub const STARTING_ACTIVITY: &str = "starting";

/// Await `work`, reporting a [`PROVISIONING_ACTIVITY`] keep-alive to `sink` every `interval` until
/// it completes. See [`with_activity_keepalive`].
pub async fn with_keepalive<F: Future>(
    sink: &AttachmentProgressSink,
    interval: Duration,
    work: F,
) -> F::Output {
    with_activity_keepalive(sink, interval, PROVISIONING_ACTIVITY, work).await
}

/// Await `work`, reporting a keep-alive naming `activity` to `sink` every `interval` until it
/// completes.
///
/// The first keep-alive is one `interval` in, so work that finishes sooner emits none. Nothing is
/// emitted once `work` has returned: the ticking lives inside this call. A sink that reports
/// nowhere (the unary start) makes this a plain `work.await`.
pub async fn with_activity_keepalive<F: Future>(
    sink: &AttachmentProgressSink,
    interval: Duration,
    activity: &str,
    work: F,
) -> F::Output {
    tokio::pin!(work);
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
    loop {
        tokio::select! {
            // Work first, so a tick that is ready alongside completion is not reported.
            biased;
            output = &mut work => return output,
            _ = ticks.tick() => sink.keep_alive(activity),
        }
    }
}
