//! The `session.agents` broadcast — a session's agent roster, published into the session room.
//!
//! Product contract: `docs/ft/daemon/session-agent-roster.md` § The roster stream.
//!
//! Beside [`crate::session_activity`], and for the same reason: publisher and receiver live in
//! different crates, so a topic each of them spelled for itself would fail as *silence* — every
//! receiver filters by topic, and a mismatch delivers nothing and reports nothing.
//!
//! The payload is [`crate::proto::session_agents_svc::SessionAgentRoster`], the same message
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

/// The coordinate `session_agents.SessionAgentService` is served at, named once.
///
/// Here, beside the two constants above, and for the same reason: every party that has to agree on
/// it lives in a different crate — the `ServiceEntry` the daemon mounts, the host-side bridge that
/// answers a call relayed out of a jail, the peer a routed call is forwarded to, and
/// [`IN_JAIL_RELAYABLE`] below. A name spelled out in four places is a name three of them can be
/// wrong about, and every one of those mistakes fails as `not_found` at runtime.
pub const SESSION_AGENT_SERVICE: &str = "session_agents.SessionAgentService";

/// The `(service, method)` pairs an in-jail agent may relay to its host, for family B.
///
/// `packages/tddy-sandbox-runner/src/runner.rs` holds the jail side of this: what an in-jail agent
/// may ask its host to dispatch. Exposed as data so that allowlist and the served coordinate cannot
/// drift — the runner reads this rather than repeating the strings. An allowlist that no longer
/// matches the served coordinate fails **closed**, silently, at runtime, which is the one failure
/// mode worth a shared constant.
///
/// In this crate rather than in `tddy-session-agents`, which serves the coordinate and re-exports
/// these two, because `tddy-sandbox-runner` is the other reader and it runs *inside every jail*:
/// `tddy-session-agents` depends on `livekit`, `tddy-livekit` and `tddy-daemon-livekit`, none of
/// which the runner links today. Reaching the allowlist through it would put a WebRTC stack in
/// every jail to share five string pairs. `tddy-service` is a dependency of both already, and it
/// owns `session_agents.proto` — the file that declares the coordinate these name.
///
/// The permitted operation *set* is not this constant's to change. It is exactly the five family-B
/// operations the jail allowed before `#unbundle` node 7 moved them; only the service name each
/// tuple carries moved with them.
pub const IN_JAIL_RELAYABLE: [(&str, &str); 5] = [
    (SESSION_AGENT_SERVICE, "StreamSessionAgents"),
    (SESSION_AGENT_SERVICE, "OpenAgentConversation"),
    (SESSION_AGENT_SERVICE, "PromptAgentConversation"),
    (SESSION_AGENT_SERVICE, "CancelAgentConversation"),
    (SESSION_AGENT_SERVICE, "ReportAgentConversationState"),
];
