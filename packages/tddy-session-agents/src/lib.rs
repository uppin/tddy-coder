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
pub use service::{
    agent_conversation_frames, agent_stop_reason, build_session_agents_entry,
    roster_at_the_new_coordinate, SessionAgentServiceImpl,
};
pub use status_reporting::{note_agent_activity, republish_quietly};

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

    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use tddy_core::SessionAgentRecord;
    use tddy_discovery::subagent::SubagentSession;
    use tddy_rpc::Status;
    use tddy_service::proto::connection::SessionAgentRoster;
    use tddy_service::proto::session_agents_svc::{
        AgentConversationChunk, OpenAgentConversationRequest, PromptAgentConversationRequest,
    };

    use crate::session_agent_clone::SessionAgentCloneStore;
    use crate::session_agent_roster::SessionAgentRosterStore;
    use crate::session_agent_status::SessionAgentActivityStore;

    /// The one operator this host is configured for, and the one session of theirs it facilitates.
    /// A token it did not mint reaches nothing, which is the refusal every handler starts from.
    fn sessions_of(os_user: &'static str) -> SessionDirResolver {
        Arc::new(
            move |session_token: &str, session_id: &str| match session_token {
                "web-token-for-ada" => Ok(PathBuf::from(format!(
                    "/var/lib/tddy/users/{os_user}/sessions/{session_id}"
                ))),
                _ => Err(Status::unauthenticated("invalid or expired session")),
            },
        )
    }

    /// A host with no `<tddyhome>/agents` defs and no registry assistants, so no id resolves on it.
    struct NoAgentDefsOnThisHost;

    #[async_trait]
    impl AgentCatalog for NoAgentDefsOnThisHost {
        async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> {
            Err(Status::invalid_argument(format!(
                "agent '{agent_id}' resolves to no def on this daemon"
            )))
        }
    }

    /// A single-daemon deployment: every agent it can resolve is its own, so an admission claims no
    /// checkout anywhere and there is nothing to hand back.
    struct EveryAgentIsLocalHere;

    #[async_trait]
    impl AgentAdmission for EveryAgentIsLocalHere {
        async fn admit(
            &self,
            _session_id: &str,
            _session_dir: &Path,
            record: &SessionAgentRecord,
            _session_token: &str,
        ) -> Result<AdmittedAgent, Status> {
            Ok(AdmittedAgent {
                daemon_instance_id: record.daemon_instance_id.clone(),
                codebase_session_id: None,
                commissioned: false,
            })
        }

        async fn withdraw(
            &self,
            _session_id: &str,
            _admitted: &AdmittedAgent,
            _session_token: &str,
        ) {
        }

        async fn tear_down(
            &self,
            _session_id: &str,
            daemon_instance_id: &str,
            codebase_session_id: &str,
            _session_token: &str,
        ) -> Result<(), Status> {
            Err(Status::failed_precondition(format!(
                "this daemon commissioned no checkout {codebase_session_id} on {daemon_instance_id}"
            )))
        }
    }

    /// A host with no LiveKit configuration: it hosts no session rooms, so a snapshot reaches its
    /// `StreamSessionAgents` subscribers and nothing else.
    struct NoSessionRoomToBroadcastInto;

    #[async_trait]
    impl RosterBroadcast for NoSessionRoomToBroadcastInto {
        async fn broadcast(&self, _session_id: &str, _roster: &SessionAgentRoster) {}
    }

    /// A host that facilitates its own sessions and holds no peer's clone, so every conversation it
    /// opens is decided by the roster and no owner has departed.
    struct FacilitatesItsOwnSessionsOnly;

    #[async_trait]
    impl AgentSessions for FacilitatesItsOwnSessionsOnly {
        fn hosts_a_clone_for(&self, _session_id: &str) -> bool {
            false
        }

        async fn open_owned(
            &self,
            _session_id: &str,
            _agent_id: &str,
        ) -> Result<Option<Box<dyn SubagentSession>>, Status> {
            Ok(None)
        }

        async fn open_local(
            &self,
            _session_id: &str,
            _session_dir: &Path,
            record: &SessionAgentRecord,
            _session_token: &str,
        ) -> Result<Box<dyn SubagentSession>, Status> {
            Err(Status::invalid_argument(format!(
                "agent '{}' resolves to no def on this daemon any more",
                record.agent_id
            )))
        }

        fn refuse_unready_clone(
            &self,
            _session_id: &str,
            _record: &SessionAgentRecord,
        ) -> Result<(), Status> {
            Ok(())
        }

        async fn refuse_departed_owner(&self, _daemon_instance_id: &str) -> Result<(), Status> {
            Ok(())
        }
    }

    /// A host alone in its common room: there is no peer a conversation could be forwarded to.
    struct NoPeersInTheCommonRoom;

    #[async_trait]
    impl AgentConversationPeers for NoPeersInTheCommonRoom {
        async fn open(
            &self,
            _request: &OpenAgentConversationRequest,
            owner: &str,
            _conversation_id: &str,
        ) -> Result<(), Status> {
            Err(Status::unavailable(format!(
                "daemon '{owner}' is not in this daemon's common room"
            )))
        }

        async fn prompt(
            &self,
            _request: &PromptAgentConversationRequest,
            owner: &str,
        ) -> Result<
            tokio::sync::mpsc::UnboundedReceiver<Result<AgentConversationChunk, Status>>,
            Status,
        > {
            Err(Status::unavailable(format!(
                "daemon '{owner}' is not in this daemon's common room"
            )))
        }

        async fn cancel(
            &self,
            _session_token: &str,
            _session_id: &str,
            owner: &str,
            _conversation_id: &str,
        ) -> Result<(), Status> {
            Err(Status::unavailable(format!(
                "daemon '{owner}' is not in this daemon's common room"
            )))
        }
    }

    /// What a single-daemon host facilitating one operator's sessions hands this crate: its token
    /// mapping, its own instance id, its keepalive cadence, the three stores it shares with
    /// `ListSessions`, and the five capabilities only it has.
    fn ports_of_a_host_facilitating_one_operator() -> SessionAgentPorts {
        let clones = Arc::new(SessionAgentCloneStore::new());
        SessionAgentPorts {
            session_dirs: sessions_of("ada"),
            local_instance_id: "ws-01".to_string(),
            roster_keepalive: Duration::from_secs(20),
            rosters: Arc::new(SessionAgentRosterStore::new(
                Arc::clone(&clones),
                Arc::new(SessionAgentActivityStore::new()),
            )),
            clones,
            conversations: Arc::new(OpenAgentConversations::new()),
            admission: Arc::new(EveryAgentIsLocalHere),
            catalog: Arc::new(NoAgentDefsOnThisHost),
            broadcast: Arc::new(NoSessionRoomToBroadcastInto),
            sessions: Arc::new(FacilitatesItsOwnSessionsOnly),
            peers: Arc::new(NoPeersInTheCommonRoom),
        }
    }

    #[test]
    fn names_the_service_family_b_moves_to() {
        // Given
        let ports = ports_of_a_host_facilitating_one_operator();

        // When
        let entry = build_session_agents_entry(ports);

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
