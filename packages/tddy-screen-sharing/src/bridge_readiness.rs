//! Waiting for a spawned bridge to say whether its desktop came up.
//!
//! A bridge writes one readiness line to its stdout (`tddy_screenshare::readiness`) once it has
//! signed in and knows the desktop's size, or once it knows it never will. Until that line
//! arrives, a start has nothing true to tell the browser: not whether the password was right, and
//! not how large the picture it is about to mount is.
//!
//! A module of its own rather than more of `screen_sharing_service.rs`, which is due to be split
//! (`#host-desktops` node 1); the spawn path calls in here, wherever that path ends up living.

use std::time::Duration;

use tddy_rpc::Status;
use tokio::io::AsyncRead;

/// How long a start waits for a bridge's readiness line when nothing else is configured.
///
/// Long enough for a slow desktop to finish its sign-in and send a first framebuffer update; short
/// enough that a bridge which hangs is reported as a failure while the operator is still looking
/// at the dialog that started it.
pub const DEFAULT_BRIDGE_READY_TIMEOUT: Duration = Duration::from_secs(30);

/// A bridge that reported its desktop ready, and the size of that desktop's framebuffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeStarted {
    pub width: u32,
    pub height: u32,
}

/// Read a bridge's readiness line from its `stdout`, waiting at most `within`.
///
/// What each outcome becomes:
/// - `Ready { width, height }` → [`BridgeStarted`] with that size.
/// - `Failed` of kind `Authentication` → `Unauthenticated`, carrying the desktop's reason.
/// - any other `Failed` → `Unavailable`, carrying the bridge's reason.
/// - stdout closing before a line arrives (the bridge exited) → `Unavailable`.
/// - no line within `within` → `DeadlineExceeded`.
///
/// Terminating the child on a failure is the caller's job: this reads a stream and owns no
/// process.
pub async fn await_bridge_readiness<R>(stdout: R, within: Duration) -> Result<BridgeStarted, Status>
where
    R: AsyncRead + Unpin + Send,
{
    // TODO(bridge-ready): implement
    let _ = (stdout, within);
    todo!()
}
