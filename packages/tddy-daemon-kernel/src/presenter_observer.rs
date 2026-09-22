//! The port a session's presenter observer delivers events through, without naming who receives
//! them.
//!
//! When a workflow session starts, `tddy-session-lifecycle` connects to the child's
//! `PresenterObserver` gRPC stream and hands every event to **two independent sinks**: the
//! session-notification bus, which lights the session's drawer indicator in `tddy-web`, and — on a
//! daemon with a `telegram:` block — the Telegram chat surface (elicitation keyboards, state lines).
//! The observer runs when *either* exists, so it cannot be a spawner owned by Telegram: inverting
//! the whole task onto the Telegram side would leave the indicator dark on every Telegram-less
//! daemon, which is most of them.
//!
//! So the loop — connect with retry, read the stream, publish to the bus — stays with the connection
//! service, and only the Telegram half is inverted. This trait is that half. The service holds an
//! optional [`SharedPresenterEventSink`]; the daemon injects Telegram's implementation when a bot is
//! configured.
//!
//! It lives here beside the other symbols every daemon subsystem shares, because `pub(crate)` does
//! not cross a crate boundary — which is the same reason this crate exists at all.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_service::gen::ServerMessage;

/// Receives each event from a session's presenter stream, without the observer learning what it is.
///
/// An error ends that session's observer loop, exactly as a failed stream read does: the sink is
/// telling the observer it can no longer follow the session.
#[async_trait]
pub trait PresenterEventSink: Send + Sync {
    /// Handle one presenter event of `session_id`, in stream order.
    async fn on_presenter_event(
        &self,
        session_id: &str,
        event: &ServerMessage,
    ) -> anyhow::Result<()>;
}

/// The port as the connection service holds it.
pub type SharedPresenterEventSink = Arc<dyn PresenterEventSink>;
