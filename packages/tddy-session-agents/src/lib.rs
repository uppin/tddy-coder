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

pub mod agent_clone_lookup;
pub mod agent_clone_worktree;
pub mod agent_conversations;
pub mod agent_records;
pub mod agent_roster_state;
pub mod clone_readiness;
pub mod conversation_cancel_forward;
pub mod conversation_open_forward;
pub mod departed_daemon;
pub mod exec_tool_caller;
pub mod hosted_clone_start;
pub mod opened_session_room;
pub mod ports;
pub mod roster_broadcast;
pub mod service;
pub mod session_agent_clone;
pub mod session_agent_inference;
pub mod session_agent_roster;
pub mod session_agent_status;
pub mod session_room_participants;
pub mod spawn_agent_def;
pub mod status_reporting;

pub use agent_conversations::{AgentConversation, OpenAgentConversations, PromptRouting};
pub use agent_roster_state::AgentRosterState;
pub use ports::{
    AdmittedAgent, AgentAdmission, AgentCatalog, AgentConversationPeers, AgentSessions,
    RosterBroadcast, SessionAgentPorts, SessionDirResolver,
};
pub use service::{
    agent_conversation_frames, agent_message_role, agent_stop_reason, agent_turn_frames,
    SessionAgentServiceImpl,
};
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

    /// What a jailed process may reach on its host, as a **closed** set: a widening or a narrowing
    /// has to be made here, in a diff of its own, rather than hidden inside a mechanical change.
    ///
    /// `ResumeAgentConversation` is the one addition since `#unbundle` node 7 moved this list onto
    /// this coordinate. It reaches the same code path as `PromptAgentConversation`, with the same
    /// authentication, and so opens no weaker route than one already open — but it is not confined
    /// to the caller's own session's conversations, because nothing binds a conversation id to the
    /// session that opened it (see
    /// `docs/dev/todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`;
    /// the gap predates this entry). Unlike prompt, resume is a destructive write:
    /// `from_message_id` truncates a transcript and `correction` injects into it. It is here
    /// because the in-jail `tddy-tools`
    /// advertises `subagent_resume`, and an operation advertised in a jail and refused by the
    /// relay is the "tool that is offered and cannot be called" defect this coordinate's own
    /// history is full of.
    #[test]
    fn relays_exactly_the_six_operations_the_jail_is_allowed() {
        let methods: Vec<&str> = IN_JAIL_RELAYABLE.iter().map(|(_, m)| *m).collect();

        assert_eq!(
            methods,
            vec![
                "StreamSessionAgents",
                "OpenAgentConversation",
                "PromptAgentConversation",
                "ResumeAgentConversation",
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
