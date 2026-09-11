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

pub mod session_agent_clone;
pub mod session_agent_inference;
pub mod session_agent_roster;
pub mod session_agent_status;

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
///
/// # Not yet constructible, and not from a `LiveAgentRoster` at all
///
/// [`LiveAgentRoster`] is the roster as a **client** process sees it — seeded from
/// `TDDY_SUBAGENTS_JSON` at spawn and replaced by every frame the daemon publishes. It is what
/// in-jail `tddy-tools` and `tddy-sandbox-app` read, and it is a *subscriber* to this service, not
/// its state. The authoritative store this service answers from is
/// [`session_agent_roster::SessionAgentRosterStore`], beside
/// [`session_agent_clone::SessionAgentCloneStore`] and
/// [`session_agent_status::SessionAgentActivityStore`] — all three in this crate. Handing the
/// service the client's mirror would have it answer `AttachSessionAgent` by writing into a copy
/// nobody persists.
///
/// The nine handlers in `tddy-daemon`'s `connection_service::rpc_service` read a good deal more of
/// the host than any roster is. All nine resolve a session directory from the caller's token; seven
/// classify a peer route before they look a session up, and three of those forward over this
/// daemon's common-room handle; two read the daemon's own instance id out of its config to tell a
/// local agent from an owned one; `StreamSessionAgents` paces a quiet roster from a configured
/// keepalive; `OpenAgentConversation` and `PromptAgentConversation` spawn and drive node 5's
/// conversation runtime and hold the open conversations in a shared map; `PromptAgentConversation`
/// additionally reports a turn's end; `AttachSessionAgent` resolves a remote agent id against the
/// agent catalog and claims — and on failure unwinds — a checkout on a peer.
///
/// So this constructor takes the wrong argument, not merely too few: what it wants is a ports
/// struct, the shape `tddy_session_files::build_session_files_entry` already takes for the same
/// reason. Panicking is deliberate until it has one. A service mounted on the daemon's local Unix
/// socket answering `unimplemented` to all nine would be a silent capability removal on a
/// privileged interface — and five of the nine are what the sandbox relay allowlist below permits
/// an in-jail agent to reach, so the removal would land inside a jail — whereas a panic at the
/// wiring site cannot be mistaken for a working mount.
///
/// TODO(session-agent-services): take `SessionAgentPorts` (session-token-to-session-directory
/// resolver, the peer-route classifier and this daemon's common-room slot, the local instance id,
/// the roster keepalive, the agent catalog, the three stores above, node 5's conversation runtime
/// and the open-conversation map, the turn-end reporter and the clone-claim pair) and move the nine
/// handlers out of `tddy-daemon`'s `connection_service::rpc_service` behind it, leaving the
/// daemon's routing preamble in the daemon as node 6 left its own.
pub fn build_session_agents_entry(_roster: Arc<LiveAgentRoster>) -> tddy_rpc::ServiceEntry {
    unimplemented!(
        "build_session_agents_entry needs the ports the nine handlers read the host through"
    )
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

    use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};

    /// The seed a jail is spawned with — one def, as `TDDY_SUBAGENTS_JSON` carries it.
    fn a_seed_def(name: &str) -> SpecializedAgentDef {
        SpecializedAgentDef {
            name: name.to_string(),
            label: None,
            model: "qwen2.5-coder:7b".to_string(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            system_prompt: None,
            system_prompt_path: None,
            tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
            max_turns: 10,
            replaces: Vec::new(),
        }
    }

    /// A roster holding the one agent a session was seeded with, addressed under the daemon that
    /// resolved its def.
    fn a_roster_holding_one_seeded_agent() -> Arc<LiveAgentRoster> {
        Arc::new(LiveAgentRoster::seeded_from(
            "1780828020298-roster",
            vec![a_seed_def("explorer")],
            "ws-01",
        ))
    }

    #[test]
    fn names_the_service_family_b_moves_to() {
        // Given
        let roster = a_roster_holding_one_seeded_agent();

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
