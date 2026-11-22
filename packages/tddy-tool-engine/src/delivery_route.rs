//! How an orchestrator best reaches one of its sessions (`#orchestra` 7/9), as `ListProjectSessions`
//! reports it per session:
//!
//! - **native** — the session's agent has native peer messaging (`coding_agent.peer_name`) and runs
//!   on this host, as the orchestrator does: message it by its peer name, agent to agent;
//! - **terminal** — the session runs on a target (Coder, SSH): type into its TUI with
//!   `SendSessionMessage`, which reaches the remote agent where no `tddy-tools` runs;
//! - **inbox** — any other session: `PostSessionMessage`, read from its inbox, which wakes it.
//!
//! The route is a function of the agent's spec and where the session runs, never of the agent's
//! name.

use serde::{Deserialize, Serialize};
use tddy_coding_agent_specs::CodingAgentSpec;

/// One session's best route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeliveryRoute {
    Native { peer_name: String },
    Terminal,
    Inbox,
}

/// What a session's route is decided from.
#[derive(Debug, Clone)]
pub struct DeliveryInputs<'a> {
    /// The session's native peer name, as it was launched under (`#orchestra` 2/9's
    /// `native_peer_name_of`); the daemon computes it, this module only routes by it.
    pub peer_name: &'a str,
    /// The session's agent; `None` for a session with no coding agent (a workspace).
    pub agent: Option<&'a CodingAgentSpec>,
    /// Whether its agent runs on a target rather than on this host.
    pub on_target: bool,
    /// Whether the orchestrator asking has native peer messaging itself.
    pub orchestrator_is_native_peer: bool,
}

/// The route an orchestrator best reaches the session described by `inputs` by.
pub fn delivery_route_for(inputs: &DeliveryInputs<'_>) -> DeliveryRoute {
    if inputs.on_target {
        return DeliveryRoute::Terminal;
    }
    let child_has_peer_name = inputs
        .agent
        .is_some_and(|agent| agent.peer_name().is_some());
    if child_has_peer_name && inputs.orchestrator_is_native_peer {
        return DeliveryRoute::Native {
            peer_name: inputs.peer_name.to_string(),
        };
    }
    DeliveryRoute::Inbox
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_coding_agent_specs::embedded_spec;

    const PEER_NAME: &str = "the-childs-peer-name";

    fn route_of(agent: &str, on_target: bool, orchestrator_is_native_peer: bool) -> DeliveryRoute {
        let spec = embedded_spec(agent).expect("built in");
        delivery_route_for(&DeliveryInputs {
            peer_name: PEER_NAME,
            agent: Some(&spec),
            on_target,
            orchestrator_is_native_peer,
        })
    }

    #[test]
    fn a_local_claude_child_of_a_claude_orchestrator_is_reached_natively_by_its_peer_name() {
        assert_eq!(
            route_of("claude", false, true),
            DeliveryRoute::Native {
                peer_name: PEER_NAME.to_string()
            }
        );
    }

    #[test]
    fn a_child_on_a_target_is_reached_through_its_terminal_even_when_native_is_possible() {
        assert_eq!(route_of("claude", true, true), DeliveryRoute::Terminal);
    }

    #[test]
    fn a_local_cursor_child_is_reached_through_its_inbox() {
        assert_eq!(route_of("cursor", false, true), DeliveryRoute::Inbox);
    }

    #[test]
    fn a_claude_child_of_an_orchestrator_without_native_messaging_is_reached_through_its_inbox() {
        assert_eq!(route_of("claude", false, false), DeliveryRoute::Inbox);
    }

    #[test]
    fn a_session_without_an_agent_is_reached_through_its_inbox() {
        assert_eq!(
            delivery_route_for(&DeliveryInputs {
                peer_name: PEER_NAME,
                agent: None,
                on_target: false,
                orchestrator_is_native_peer: true,
            }),
            DeliveryRoute::Inbox
        );
    }
}
