//! The agents attached to a session, and the conversations held with them.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 7, serving
//! `session_agents.SessionAgentService` — family B, 9 methods. It sits in front of
//! `tddy-discovery`'s conversation runtime and roster, which node 5 moved there.
//!
//! # Five of these nine methods are a security boundary
//!
//! `packages/tddy-sandbox-runner/src/runner.rs` holds a `(service, method)` tuple allowlist of what
//! an in-jail agent may relay to its host, and it names `StreamSessionAgents`,
//! `OpenAgentConversation`, `PromptAgentConversation`, `CancelAgentConversation` and
//! `ReportAgentConversationState` — every one of them family B.
//!
//! Moving the coordinate without moving the allowlist makes every in-jail subagent conversation fail
//! **closed**. That is the safe direction, but silently and at runtime rather than at compile time,
//! which is why node 7's acceptance test drives a **real jail** rather than inspecting the list: a
//! test that read the tuples would pass while every conversation from inside a jail was broken.
//!
//! # What this crate does not decide
//!
//! It does not widen or narrow what the allowlist permits. The set of allowed operations is
//! identical; only the service name each tuple carries changes. Hiding a security change inside a
//! mechanical one is exactly what a stack like this makes easy and must not do.

pub mod agent_conversations;
pub mod ports;
pub mod service;
pub mod session_agent_clone;
pub mod session_agent_inference;
pub mod session_agent_roster;
pub mod session_agent_status;
pub mod status_reporting;

pub use agent_conversations::{AgentConversation, OpenAgentConversations, PromptRouting};
pub use ports::{
    AdmittedAgent, AgentAdmission, AgentCatalog, AgentConversationPeers, AgentSessions,
    RosterBroadcast, SessionAgentPorts, SessionDirResolver,
};
pub use service::{agent_conversation_frames, agent_stop_reason, SessionAgentServiceImpl};
pub use status_reporting::{note_agent_activity, republish_quietly};

/// The coordinate this crate serves, and the `(service, method)` pairs an in-jail agent may relay
/// to its host.
///
/// Defined in `tddy-service` beside `session_agents.proto`'s other cross-crate constants and
/// re-exported here, because the second reader of the allowlist is `tddy-sandbox-runner` — the
/// binary that runs *inside every jail*, and one that does not link this crate's `livekit`
/// dependencies. See [`tddy_service::session_agents::IN_JAIL_RELAYABLE`] for why.
pub use tddy_service::session_agents::{IN_JAIL_RELAYABLE, SESSION_AGENT_SERVICE as SERVICE_NAME};

#[cfg(test)]
mod tests {
    use super::*;

    /// The permitted operation set is unchanged by this move — only the service name each tuple
    /// carries. Widening or narrowing it here would hide a security change inside a mechanical one.
    #[test]
    fn relays_exactly_the_five_operations_the_jail_allowed_before() {
        let methods: Vec<&str> = IN_JAIL_RELAYABLE.iter().map(|(_, m)| *m).collect();

        assert_eq!(
            methods,
            vec![
                "StreamSessionAgents",
                "OpenAgentConversation",
                "PromptAgentConversation",
                "CancelAgentConversation",
                "ReportAgentConversationState",
            ]
        );
    }

    /// Every relayable tuple names the new service, so the runner reading this cannot be pointing at
    /// the old coordinate.
    #[test]
    fn every_relayable_tuple_names_the_new_service() {
        for (service, method) in IN_JAIL_RELAYABLE {
            assert_eq!(
                service, "session_agents.SessionAgentService",
                "{method} is still allowed under the old coordinate"
            );
        }
    }
}
