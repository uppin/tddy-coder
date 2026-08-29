//! The in-jail registry that follows the session's live agent roster.
//!
//! See docs/ft/daemon/session-agent-roster.md § Invoking an agent and § The roster stream.
//!
//! `tddy-tools --mcp` used to build its subagent registry from `TDDY_SUBAGENTS_JSON` — an env var
//! fixed when the jail was spawned. An agent attached at minute forty was therefore uncallable
//! until the session restarted, and one detached stayed callable forever. The registry now follows
//! `StreamSessionAgents`, and the env var is demoted to the **seed** that covers the window between
//! spawn and the first frame.
//!
//! Two properties are what this module exists for, and both are about *silent* disagreement rather
//! than staleness:
//!
//! - a frame **replaces** the registry rather than merging into it, so an agent the daemon stopped
//!   listing stops being callable here too;
//! - a registry that can no longer be kept current **refuses**, because one frozen at its last
//!   known roster answers for detached agents and refuses attached ones without saying so.
//!
//! With one bound on the second: a withdrawal never outlives the reachability of its replacement.
//! A roster that was current and went stale still withdraws the tools its agents took over — those
//! agents were real — but one that never received a frame at all withdraws nothing, because there
//! it would take a tool away and offer nothing in its place ([`RosterCurrency`]).
//!
//! The module is split three ways, by what each part is answerable for: [`registry`] holds what the
//! roster currently says and what it permits, [`seed`] holds what the spawn environment claimed
//! before the first frame, and [`stream`] holds the subscription that replaces the one with the
//! other.

mod conversation;
mod link;
mod registry;
mod seed;
mod stream;

pub use conversation::{
    AgentConversationLink, RemoteAgentSession, RemoteConversationHandle, NO_TRANSPORT,
};
pub use registry::{
    AddressableAgent, CatalogVisibility, ConversationState, LiveAgentRoster, RosterError, Takeover,
    WithdrawnExecTools,
};
pub use seed::session_agent_roster;
pub use stream::{
    follow_session_agent_roster, ReconnectPacing, RosterStreamOutcome,
    PASS_LONG_ENOUGH_TO_BE_SERVICE,
};
