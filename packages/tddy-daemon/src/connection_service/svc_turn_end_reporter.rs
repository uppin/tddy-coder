// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use prost::Message as _;

use tddy_service::proto::connection::ListSubagentsResponse;

use tddy_service::proto::connection::ListSubagentsRequest;

use crate::{
    connection_service::agent_roster, livekit_peer_discovery::local_instance_id_for_config,
};

use tddy_service::proto::connection::CancelAgentConversationRequest;

use tddy_rpc::Status;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Ask `daemon_instance_id` to cancel a conversation its own turn loop is running.
    pub(crate) async fn forward_cancel_agent_conversation(
        &self,
        session_token: &str,
        session_id: &str,
        daemon_instance_id: &str,
        conversation_id: &str,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("CancelAgentConversation")?;
        crate::livekit_peer_discovery::forward_to_peer(
            slot,
            daemon_instance_id,
            "connection.ConnectionService",
            "CancelAgentConversation",
            CancelAgentConversationRequest {
                // The detaching caller's own token: the peer authenticates a cancel exactly as it
                // authenticated the open, and this daemon holds no other credential to present.
                session_token: session_token.to_string(),
                session_id: session_id.to_string(),
                daemon_instance_id: daemon_instance_id.to_string(),
                conversation_id: conversation_id.to_string(),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    /// The roster entry a qualified `agent_id` attaches as.
    ///
    /// The id must be qualified: there is deliberately no "assume the local daemon" reading, which
    /// is the reading that silently picks the wrong host the moment two daemons offer a def of the
    /// same name.
    pub(crate) async fn roster_record_for_agent_id(
        &self,
        agent_id: &str,
    ) -> Result<tddy_core::SessionAgentRecord, Status> {
        let id = tddy_core::AgentId::parse(agent_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.roster_record_for(&id, agent_id).await
    }

    /// The roster entry `id` resolves to, reporting any refusal under `named_as` — the string the
    /// caller actually sent, so an operator is never sent looking for an id they never typed.
    pub(crate) async fn roster_record_for(
        &self,
        id: &tddy_core::AgentId,
        named_as: &str,
    ) -> Result<tddy_core::SessionAgentRecord, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        if id.daemon_instance_id != local_instance_id {
            return self.remote_roster_record_for(id, named_as).await;
        }
        let def = self
            .resolvable_agent_defs()
            .await?
            .into_iter()
            .find(|d| d.name == id.name)
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{named_as}' resolves to no def on daemon '{local_instance_id}' (not \
                     found under <tddyhome>/agents, and not an assistant in its registry)"
                ))
            })?;
        agent_roster::roster_record(&def, &local_instance_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))
    }

    /// The roster entry an id naming a **peer** resolves to, taken from that peer's own
    /// `ListSubagents`.
    ///
    /// Resolved there and nowhere else, deliberately: a local def of the same name is a *different*
    /// agent, and answering from it would run the wrong host's agent under the id the operator
    /// picked — the exact failure qualified ids exist to prevent (PRD § What attach does, step 1).
    ///
    /// The peer must be visible in the common room *before* it is asked. A daemon that resolves on
    /// no host is `INVALID_ARGUMENT` naming the id, decided from the eligible-daemon list rather
    /// than by waiting out a forward deadline against a participant that was never there — the
    /// caller mistyped an id, which is a bad request and not a slow one.
    pub(crate) async fn remote_roster_record_for(
        &self,
        id: &tddy_core::AgentId,
        named_as: &str,
    ) -> Result<tddy_core::SessionAgentRecord, Status> {
        let owning_daemon = id.daemon_instance_id.as_str();
        if !self
            .eligible_instance_ids()
            .iter()
            .any(|candidate| candidate == owning_daemon)
        {
            return Err(Status::invalid_argument(format!(
                "agent '{named_as}' is owned by daemon '{owning_daemon}', which is not in this \
                 daemon's common room; nothing was attached and no checkout was created"
            )));
        }

        let slot = self.common_room_slot("AttachSessionAgent")?;
        let answered = crate::livekit_peer_discovery::forward_to_peer(
            slot,
            owning_daemon,
            "connection.ConnectionService",
            "ListSubagents",
            ListSubagentsRequest {}.encode_to_vec(),
        )
        .await
        .map_err(|e| {
            Status::unavailable(format!(
                "daemon '{owning_daemon}' owns agent '{named_as}' but did not answer \
                 ListSubagents ({}); nothing was attached and no checkout was created",
                e.message()
            ))
        })?;
        let listed = ListSubagentsResponse::decode(answered.as_slice())
            .map_err(|e| Status::internal(format!("decode ListSubagentsResponse: {e}")))?;

        let row = listed
            .subagents
            .into_iter()
            .find(|s| s.name == id.name)
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{named_as}' resolves to no def on daemon '{owning_daemon}' (it \
                     answered with no subagent called '{}')",
                    id.name
                ))
            })?;

        // The peer stamps the id it minted, and it is taken verbatim rather than reassembled here:
        // an id the two sides spelled differently routes to a daemon the operator never picked.
        let agent_id = match row.agent_id.trim().is_empty() {
            true => agent_roster::qualified_agent_id(&row.name, owning_daemon)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
            false => row.agent_id,
        };
        Ok(tddy_core::SessionAgentRecord {
            agent_id,
            name: row.name,
            daemon_instance_id: owning_daemon.to_string(),
            label: Some(row.label).filter(|l| !l.is_empty()),
            model: row.model,
            replaces: row.replaces,
            tools: row.tools,
            // Filled in by the caller once the clone for (session, owning daemon) is claimed: the
            // record is resolved before this daemon knows which session it is being attached to.
            codebase_session_id: None,
        })
    }

    /// Build the `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` jail env pair for already-resolved
    /// specialized-agent defs (see [`Self::resolve_specialized_agent_defs`]). Empty input produces
    /// no env pairs.
    ///
    /// TODO(session-agent-roster): the in-jail runner derives `--allowedTools` /
    /// `--disallowedTools` from these seeded defs, so a def whose `replaces` was edited between the
    /// attach and the relaunch changes what the relaunched main agent may call — the one thing
    /// snapshotting `replaces` into the roster exists to prevent. Closing it means handing the
    /// runner the roster's replaced set outright instead of letting it re-derive one
    /// (docs/ft/daemon/session-agent-roster.md AC25).
    pub(crate) fn specialized_subagent_env(
        &self,
        defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
    ) -> Result<Vec<(String, String)>, Status> {
        if defs.is_empty() {
            return Ok(Vec::new());
        }
        let names = defs
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let defs_json = serde_json::to_string(defs).map_err(|e| {
            Status::internal(format!("failed to serialize specialized agent defs: {e}"))
        })?;
        Ok(vec![
            ("TDDY_SUBAGENT".to_string(), names),
            ("TDDY_SUBAGENTS_JSON".to_string(), defs_json),
        ])
    }

    /// Tell the jail which daemon facilitates it.
    ///
    /// Exported unconditionally, and not folded into [`Self::specialized_subagent_env`] which is
    /// skipped for a session that starts with no agents: the roster is mutated while the session
    /// runs, so a jail started empty still needs to be able to qualify what it is later told.
    ///
    /// Without it the in-jail `tddy-tools` cannot qualify its seeded agent ids — a seed resolved on
    /// this daemon is `explorer`, and the id the main agent must type is `explorer@{this daemon}`.
    /// The two other transports carry the id in their own environment
    /// (`TDDY_REMOTE_DAEMON_INSTANCE_ID`, the HTTP daemon's own); a sandbox-IPC jail is told nothing
    /// at all, and a bare id resolves against whichever daemon happens to answer.
    ///
    /// Named after the daemon's own `TDDY_DAEMON_INSTANCE_ID` startup override, so the value and the
    /// variable an operator would set to change it are spelled the same on both sides.
    pub(crate) fn jail_daemon_identity_env(&self) -> Vec<(String, String)> {
        vec![(
            "TDDY_DAEMON_INSTANCE_ID".to_string(),
            local_instance_id_for_config(&self.config),
        )]
    }

    /// The `TDDY_LSP_TOOLS` jail env pair — set when a language server is available for the
    /// session's worktree, so the in-jail `tddy-tools --mcp` exposes the `Lsp*` tools.
    pub(crate) fn lsp_tools_env(&self, worktree_root: &std::path::Path) -> Vec<(String, String)> {
        let available = tddy_core::toolcall::lsp::lsp_executor()
            .map(|ex| ex.is_available(worktree_root))
            .unwrap_or(false);
        if available {
            vec![("TDDY_LSP_TOOLS".to_string(), "rust".to_string())]
        } else {
            Vec::new()
        }
    }
}
