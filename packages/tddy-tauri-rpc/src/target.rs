//! What a page's connection asked to reach, and how the host application turns that into a roster.
//!
//! Over LiveKit a session is a *participant* you target inside a *room*. Over IPC there was no
//! equivalent, because there was nothing to address: a page got one connection and that connection
//! reached one roster, the daemon's. Session-scoped work on the desktop therefore fell back to the
//! daemon.
//!
//! [`ConnectionTarget`] is the addressing this crate gained. It names what a page wants in the
//! host's own vocabulary — no rooms, no participants, no identity strings of LiveKit's shape.

use std::sync::Arc;

use tddy_rpc::RpcService;

/// What a connection asked to reach.
///
/// Deliberately a **closed** enum rather than a string. An open target would invite the LiveKit
/// identity strings this stack exists to remove (`daemon-{instance}-{session}`) to leak across the
/// IPC boundary, and once one had, nothing would notice. A new kind of peer is a new variant and a
/// deliberate decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConnectionTarget {
    /// The daemon's own RPC roster — what every page has always reached.
    Daemon,
    /// One session's RPC, addressed by session id.
    Session { session_id: String },
}

/// Resolves a connection's target to the roster that serves it.
///
/// This crate stays a generic webview-RPC host: it knows a target is a value it was handed and that
/// somebody can turn it into a service, and nothing more. Only the host application knows that a
/// session id maps to the daemon's session-scoped roster — which is what keeps this crate reusable
/// and the layering honest.
pub trait RosterResolver: Send + Sync + 'static {
    /// The roster serving `target`, or `None` when nothing does.
    ///
    /// `None` is refused **at connect**, with a reason, rather than by accepting the connection and
    /// silently answering nothing: a page that is told can fail, and a page that is not waits
    /// forever.
    fn roster_for(&self, target: &ConnectionTarget) -> Option<Arc<dyn RpcService>>;
}

/// Why a connection could not be opened.
#[derive(Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// No roster serves the requested target — an unknown session, or one that has ended.
    NoSuchTarget { target: ConnectionTarget },
    /// A connection is already registered under this client epoch.
    ///
    /// Epochs are minted per transport on the page side, so a collision means the page reused one.
    /// Accepting it would route two connections' answers to one sink.
    EpochInUse { client_epoch: u32 },
}
