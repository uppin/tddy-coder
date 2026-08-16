//! The `session.activity` broadcast — the agent's own tool calls, published into the session room.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Beside [`crate::worktree_activity`], and for the same reason that module gives: publisher and
//! receiver live in different crates, so a topic each of them spelled for itself would fail as
//! *silence* — every receiver filters by topic, and a mismatch delivers nothing and reports
//! nothing.
//!
//! The payload is [`crate::proto::connection::AgentActivityRecord`], the same message
//! `StreamSessionActivity` returns. One schema for the record however it is delivered: a broadcast
//! that drifted from the stream would give two participants two different accounts of one call.

/// The data-channel topic agent activity is broadcast on inside a session room.
///
/// Deliberately not `tddy-rpc`: every RPC receiver in the system hard-filters on that topic, so a
/// record published there would be dropped by some peers and mistaken for a request by others.
/// Deliberately not [`crate::worktree_activity::WORKTREE_ACTIVITY_TOPIC`] either — the two carry
/// different schemas, and a receiver that wants only commits should not have to decode every tool
/// call to discover that.
pub const SESSION_ACTIVITY_TOPIC: &str = "session.activity";
