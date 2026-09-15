//! The port `ConnectionServiceImpl` holds instead of a concrete Telegram type.
//!
//! `connection_service.rs:141` holds `telegram: Option<Arc<TelegramDaemonHooks>>`, and its **only**
//! consumer is `svc_resolve_tddy_tools_path.rs:439`, passing it to
//! `telegram_session_subscriber::spawn_presenter_observer_task`.
//!
//! `TelegramDaemonHooks` is defined in `telegram_session_subscriber.rs` — one of the six modules
//! `#carve` 7/9 moves out — so that one field is the entire reason a 7,403-line cluster cannot
//! leave. Every other edge from the cluster points outward, at eight `tddy-session-lifecycle`
//! modules, none of which reaches back.
//!
//! This port is where the daemon's own wiring meets the connection service, so it lives here beside
//! the other symbols every daemon subsystem shares — `pub(crate)` does not cross a crate boundary,
//! which is the same reason this crate exists at all.

use std::sync::Arc;

/// Starts whatever watches a session's presenter stream, without naming what that is.
///
/// The daemon injects an implementation; `connection_service` holds the trait object and never
/// learns whether the other end is Telegram, something else, or nothing at all.
///
/// `None` is a first-class answer at the call site today — a daemon with no `telegram:` config block
/// has no hooks — so an implementation that does nothing is a valid one, and the absence of a
/// notifier is not an error to report.
pub trait PresenterObserverSpawner: Send + Sync {
    /// Begin observing the presenter stream of a session that has just started.
    ///
    /// `grpc_port` is the session's own port, which is what the observer connects to. Called once
    /// per started session, from the spawn path, and must not block it.
    fn spawn_presenter_observer(&self, session_id: &str, grpc_port: u16);
}

/// The implementation for a daemon with no observer configured.
///
/// Injecting this is what replaces `Option<Arc<TelegramDaemonHooks>>` being `None`, so the service
/// holds one shape rather than branching on absence.
pub struct NoPresenterObserver;

impl PresenterObserverSpawner for NoPresenterObserver {
    fn spawn_presenter_observer(&self, _session_id: &str, _grpc_port: u16) {}
}

/// The port as the connection service holds it.
pub type SharedPresenterObserver = Arc<dyn PresenterObserverSpawner>;
