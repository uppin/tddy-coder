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

use std::sync::Arc;

use tddy_discovery::roster::LiveAgentRoster;

/// Why a roster or conversation operation could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum SessionAgentError {
    #[error("no session {session_id} on this host")]
    NoSuchSession { session_id: String },
    #[error("the roster for {session_id} is stale, so it cannot be answered from")]
    RosterStale { session_id: String },
    #[error("no agent is addressable for {session_id}")]
    NoAddressableAgent { session_id: String },
}

/// The `session_agents.SessionAgentService` entry the daemon's wiring layer registers.
pub fn build_session_agents_entry(_roster: Arc<LiveAgentRoster>) -> tddy_rpc::ServiceEntry {
    // TODO(session-agent-services): implement
    unimplemented!("build_session_agents_entry")
}

/// The `(service, method)` pairs an in-jail agent may relay to its host, for family B.
///
/// Exposed as data so `tddy-sandbox-runner`'s allowlist and this service cannot drift: the runner
/// reads this rather than repeating the strings. That is the mitigation for the failure mode the
/// node's changeset names — an allowlist that no longer matches the served coordinate fails closed,
/// silently.
pub const IN_JAIL_RELAYABLE: [(&str, &str); 5] = [
    ("session_agents.SessionAgentService", "StreamSessionAgents"),
    (
        "session_agents.SessionAgentService",
        "OpenAgentConversation",
    ),
    (
        "session_agents.SessionAgentService",
        "PromptAgentConversation",
    ),
    (
        "session_agents.SessionAgentService",
        "CancelAgentConversation",
    ),
    (
        "session_agents.SessionAgentService",
        "ReportAgentConversationState",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_family_b_moves_to() {
        // Given
        let roster = Arc::new(LiveAgentRoster::default());

        // When
        let entry = build_session_agents_entry(roster);

        // Then
        assert_eq!(entry.name, "session_agents.SessionAgentService");
    }

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

    /// A stale roster is not an empty one. Answering "no agents" from a roster whose stream dropped
    /// is how a picker offers nothing while an agent is attached.
    #[test]
    fn tells_a_stale_roster_apart_from_one_with_no_agents() {
        let stale = SessionAgentError::RosterStale {
            session_id: "session-a".to_string(),
        };
        let none = SessionAgentError::NoAddressableAgent {
            session_id: "session-a".to_string(),
        };

        assert!(stale.to_string().contains("stale"));
        assert!(none.to_string().contains("no agent is addressable"));
    }
}
