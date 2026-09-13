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

/// How long a pass of the roster stream must last to count as service rather than churn.
///
/// Here, beside [`SESSION_AGENTS_TOPIC`] and for the same reason: it is a constant two crates have
/// to agree on. The subscriber paces its reconnects by it (`StreamSessionAgents`, whose request and
/// snapshot are both defined in this package), and it is bounded from above by a constant in
/// `tddy-daemon` — a relay tears a forwarded stream down after its own idle deadline, and such a
/// teardown must read as *service*, because a keepalive path that goes quiet costs one reconnect
/// per deadline. Classifying that as churn would park a working cross-host subscription at the
/// subscriber's backoff ceiling. The relation is asserted where both constants are visible, in
/// `tddy-daemon`'s `livekit_peer_discovery`.
///
/// Comfortably longer than the two deadlines a fruitless pass can burn — opening the stream and
/// waiting for its first frame — so a pass that spent its whole life waiting never reads as
/// service.
pub const PASS_LONG_ENOUGH_TO_BE_SERVICE: std::time::Duration = std::time::Duration::from_secs(30);
