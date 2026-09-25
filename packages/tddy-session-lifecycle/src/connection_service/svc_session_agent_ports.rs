//! What this daemon hands `tddy-session-agents` so that crate can serve
//! `session_agents.SessionAgentService`, and the routing it keeps for itself.
//!
//! The nine roster and conversation methods are `tddy-session-agents`'; what stays here is the
//! capabilities only a daemon has — which directory a session token may reach, which def an agent
//! id resolves to on this host, whether a checkout could be claimed on the peer that owns an agent,
//! how a roster snapshot reaches a session room, how a turn loop is opened against a jail or a
//! clone, and how a conversation is forwarded to the daemon running it.
//!
//! Seven of the nine also **route** on the `daemon_instance_id` the *request* names: a roster lives
//! on the daemon facilitating its session, so a call served anywhere else answers about the wrong
//! host. That decision needs the eligible-daemon roster, the common room slot and the LiveKit
//! forwarding clients, none of which `tddy-session-agents` may reach for — which is why
//! [`PeerRoutedSessionAgents`] wraps the crate's implementation rather than the crate growing a
//! transport.
//!
//! The forward that follows the **agent's** owning daemon is a different decision and is made
//! inside the crate: it can only be taken once the roster entry naming the owner has been read.
//! Delivering it is [`ConversationsForwardedOverTheCommonRoom`]'s.

use std::sync::Arc;

use tddy_session_agents::ports::SessionAgentPorts;
use tddy_session_agents::SessionAgentServiceImpl;

use super::DaemonSessionHost;
use crate::livekit_peer_discovery::local_instance_id_for_config;

/// The coordinate this daemon serves family B at, and the one a forwarded family-B call is
/// addressed at on a peer — the same name, read from the crate that owns it so a forward cannot be
/// addressed at a name nothing answers.
const SESSION_AGENT_SERVICE: &str = tddy_session_agents::SERVICE_NAME;

impl DaemonSessionHost {
    /// The `session_agents.SessionAgentService` entry this daemon registers.
    ///
    /// Public because it is wiring: the host that assembles the roster registers it
    /// (`runtime::build`), and an acceptance test that asks what this coordinate does with a
    /// request has to address the entry the host mounts rather than a re-assembled lookalike.
    #[must_use]
    pub fn session_agents_entry(&self) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: SESSION_AGENT_SERVICE,
            service: Arc::new(tddy_service::SessionAgentServiceServer::new(
                self.session_agents_service(),
            )) as Arc<dyn tddy_rpc::RpcService>,
        }
    }

    /// This daemon's family-B surface: the crate's nine handlers, with the seven routed ones
    /// answered by the daemon that holds the roster.
    ///
    /// Rebuilt per call rather than held, and that is safe only because every piece of *state* it
    /// names is an `Arc` on this daemon — the roster store, the clone store and the open
    /// conversations. A map created here instead would have a prompt answer `NOT_FOUND` for a
    /// conversation an open on the same daemon had just created.
    #[must_use]
    pub fn session_agents_service(
        &self,
    ) -> svc_peer_routed_session_agents::PeerRoutedSessionAgents {
        svc_peer_routed_session_agents::PeerRoutedSessionAgents {
            connection: self.clone(),
            local: SessionAgentServiceImpl::new(self.session_agent_ports()),
        }
    }

    /// The host capabilities the nine handlers need, each read off this daemon.
    fn session_agent_ports(&self) -> SessionAgentPorts {
        let for_dirs = self.clone();
        SessionAgentPorts {
            // `roster_session_dir` authenticates **before** it resolves, which is load-bearing
            // rather than tidy: attaching an agent owned by another daemon contacts that peer and
            // provisions a checkout on it, so a check that ran afterwards would let an
            // unauthenticated caller build a clone on another host (PRD AC12).
            session_dirs: Arc::new(move |session_token: &str, session_id: &str| {
                for_dirs.roster_session_dir(session_token, session_id)
            }),
            local_instance_id: local_instance_id_for_config(&self.config),
            roster_keepalive: self.roster_keepalive_interval,
            rosters: Arc::clone(&self.session_agent_rosters),
            clones: Arc::clone(&self.session_agent_clones),
            conversations: Arc::clone(&self.agent_conversations),
            admission: Arc::new(
                svc_session_agent_port_adapters::ClonesClaimedOnOwningPeers {
                    connection: self.clone(),
                },
            ),
            catalog: Arc::new(
                svc_session_agent_port_adapters::DefsResolvableFromThisDaemon {
                    connection: self.clone(),
                },
            ),
            broadcast: Arc::new(svc_session_agent_port_adapters::TheSessionsOwnRoom {
                connection: self.clone(),
            }),
            sessions: Arc::new(
                svc_session_agent_port_adapters::TurnLoopsThisDaemonCanOpen {
                    connection: self.clone(),
                },
            ),
            peers: Arc::new(
                svc_session_agent_port_adapters::ConversationsForwardedOverTheCommonRoom {
                    connection: self.clone(),
                },
            ),
        }
    }
}

mod svc_session_agent_port_adapters;

mod svc_peer_routed_session_agents;
pub use svc_peer_routed_session_agents::*;
