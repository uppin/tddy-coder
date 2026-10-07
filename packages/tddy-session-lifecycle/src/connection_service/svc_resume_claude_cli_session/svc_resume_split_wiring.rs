use tddy_rpc::Request;

use tddy_rpc::Status;
use tddy_service::proto::session_agents_svc::ListSessionAgentsRequest;

use std::path::Path;

use crate::connection_service::split_ports::SplitSessions;

impl SplitSessions {
    /// Rebuild the remote-tool wiring for a split session being resumed, or `None` for a co-located
    /// one.
    ///
    /// Nothing about a split session's tool transport survives a stop: the env was injected into a
    /// process that has exited, and the join token it carried is scoped to a TTL that may have
    /// elapsed. Both are minted afresh here from the persisted pairing, which is the only part that
    /// is durable.
    ///
    /// The roster is re-read too, from the daemon that holds it — this session's own
    /// `.session.yaml` has none, because a split session's roster lives beside its codebase, and
    /// the agent attached at minute forty is recorded only there. Reading it is what makes the
    /// relaunch honour a withdrawal (PRD AC25): the flags Claude is spawned with are fixed for the
    /// life of the process, so a relaunch that assumed an empty roster would hand the main agent
    /// back, pre-approved, exactly the tools the operator took away from it.
    pub(crate) async fn resume_split_wiring(
        &self,
        meta: &tddy_core::SessionMetadata,
        sessions_base: &Path,
        session_dir: &Path,
        session_id: &str,
        session_token: &str,
    ) -> Result<Option<crate::split_session::SplitAgentWiring>, Status> {
        let Some((codebase_daemon, codebase_session)) =
            crate::connection_service::peer_session_answer::split_pairing(meta)
        else {
            return Ok(None);
        };

        // A **jailed-codebase** session is paired with a `workspace` session on this very daemon,
        // so none of the remote re-wiring below applies: there is no room to rejoin and no peer to
        // read from. What it does need is its jail back — the registry is in-process, so a daemon
        // restart emptied it while the metadata saying the checkout is sandboxed survived.
        if codebase_daemon
            == crate::livekit_peer_discovery::local_instance_id_for_config(&self.config)
        {
            return self
                .resume_colocated_jail_wiring(
                    sessions_base,
                    session_dir,
                    session_id,
                    codebase_session,
                    session_token,
                )
                .await
                .map(Some);
        }

        let roster = self
            .split_roster_from_codebase_host(session_token, codebase_session, codebase_daemon)
            .await?;
        let withdrawals = crate::split_session::wire_roster_withdrawals(&roster.agents);

        // Split placement is `claude-cli` only (PRD § Why claude-cli only), so the allow-list is
        // that backend's. Re-fetched on resume rather than trusted from the directory the previous
        // process left behind: the repository moved on while the session was stopped, and a resumed
        // agent reading a snapshot from before the stop is reading rules that may have been
        // retracted.
        let agent = crate::context_files::context_agent_for_session_type("claude-cli");
        let context = self
            .split_context_from_codebase_host(
                session_token,
                codebase_session,
                codebase_daemon,
                agent,
                "resume",
            )
            .await?;

        let wiring = crate::split_session::prepare_split_agent_wiring(
            &self.config,
            self.session_tokens()?,
            session_dir,
            &self.resolve_tddy_tools_path()?.to_string_lossy(),
            &crate::split_session::SplitSpawnTarget {
                session_id,
                codebase_instance_id: codebase_daemon,
                codebase_session_id: codebase_session,
                session_token,
            },
            &withdrawals,
            tddy_core::backend::context_globs_for_agent(agent),
            &context,
        )?;
        // The roster just read goes with the agent too, so its first `tools/list` is right before
        // any stream frame has arrived.
        let mut wiring = wiring;
        wiring
            .env
            .extend(crate::split_session::roster_seed_env_pairs(&roster));
        log::info!(
            "ResumeSession: re-wired split session {session_id} to workspace session {codebase_session} on daemon {codebase_daemon}"
        );
        Ok(Some(wiring))
    }

    /// The roster a split agent must launch with — its tool withdrawals and its roster seed — read
    /// from the daemon that holds it.
    ///
    /// Routed through this daemon's own handler, exactly as the in-jail `tddy-tools` registry's
    /// reads are routed. A failure is a **refusal**, never an empty roster: "the codebase host is
    /// unreachable" and "nothing is attached" produce the same value, and reading the second from
    /// the first is how a relaunch silently restores a withdrawn tool. A split session whose
    /// codebase host cannot be reached has no working tool call anyway.
    ///
    /// The peer's status is re-worded rather than propagated, keeping its code: the transport's own
    /// refusal ("the common room is not connected") names neither the host nor the session, and this
    /// is the one path where the operator has to know *which* pairing they cannot resume.
    pub(crate) async fn split_roster_from_codebase_host(
        &self,
        session_token: &str,
        codebase_session: &str,
        codebase_daemon: &str,
    ) -> Result<tddy_service::proto::session_agents_svc::SessionAgentRoster, Status> {
        let roster = self
            .host
            .session_agents()
            .list_session_agents(Request::direct(ListSessionAgentsRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session.to_string(),
                daemon_instance_id: codebase_daemon.to_string(),
            }))
            .await
            .map_err(|status| Status {
                code: status.code(),
                message: format!(
                    "cannot resume a split session without the agent roster held beside its \
                     codebase: reading session {codebase_session} on daemon {codebase_daemon} \
                     failed: {}",
                    status.message()
                ),
            })?
            .into_inner();
        Ok(roster)
    }
}
