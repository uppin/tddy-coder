//! The `session.agents` broadcast — a session's agent roster, published into the session room.
//!
//! Product contract: `docs/ft/daemon/session-agent-roster.md` § The roster stream.
//!
//! Beside [`crate::session_activity`], and for the same reason: publisher and receiver live in
//! different crates, so a topic each of them spelled for itself would fail as *silence* — every
//! receiver filters by topic, and a mismatch delivers nothing and reports nothing.
//!
//! The payload is [`crate::proto::connection::SessionAgentRoster`], the same message
//! `ListSessionAgents` returns and `StreamSessionAgents` streams. One schema however it is
//! delivered: a broadcast that drifted from the stream would give two participants two different
//! accounts of which agents a session has, and a consumer rebuilding a registry from the wrong one
//! answers for agents that were detached.

/// The data-channel topic a session's roster is broadcast on inside its room.
///
/// Broadcast to the whole room with no `destination_identities`, exactly as
/// [`crate::worktree_activity::WORKTREE_ACTIVITY_TOPIC`] is: every frame is a whole snapshot that
/// every participant of the session is entitled to, and addressing it would mean the publisher
/// deciding who is interested — which it cannot know, since a browser tab or a newly admitted
/// owning daemon joins at any time.
pub const SESSION_AGENTS_TOPIC: &str = "session.agents";
