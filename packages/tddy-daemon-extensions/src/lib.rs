//! Extension ports: how an optional integration hooks into the daemon without the daemon depending
//! on it.
//!
//! The daemon's core crates depend on this crate; an integration (LiveKit is the first) implements
//! its ports; and only the crates that build a binary bring the two together. A daemon with nothing
//! wired gets each port's named null object, which is a decision written down rather than a `None`
//! that could equally mean "wiring forgotten".
//!
//! - [`session_reach`] — a session was reached on this host; an integration may extend that reach
//!   beyond it.
//! - [`peer_forwarder`] — carry an RPC to a peer daemon; the kernel's `CommonRoom` keeps the policy.
//! - [`integration_status`] — the one place an integration's live state is recorded, so "is it
//!   configured, is it up" is answered once instead of re-derived per caller.

pub mod integration_status;
pub mod peer_forwarder;
pub mod session_reach;
pub mod session_rooms;

pub use integration_status::{
    IntegrationReporter, IntegrationState, IntegrationStatus, IntegrationStatusBoard,
};
pub use peer_forwarder::{NoPeerForwarding, PeerForwarder};
pub use session_rooms::{NoSessionRooms, SessionRooms};
pub use session_reach::{NoSessionReach, ReachableSessions, ReachedSession, SessionReach};
