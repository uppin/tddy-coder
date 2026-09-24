use super::DaemonSessionHost;

use super::super::SplitStartFailure;

use crate::{
    connection_service::agent_roster, livekit_peer_discovery::local_instance_id_for_config,
};

use uuid::Uuid;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use super::super::AttachmentProgressSink;

use tddy_service::proto::session::StartSessionRequest;

impl DaemonSessionHost {
    /// Start a **split** session: the agent runs here, its worktree lives on `codebase_instance_id`.
    ///
    /// The codebase daemon creates a `workspace` session holding the worktree; this daemon spawns the
    /// agent with no repository on disk and wires it to that worktree through `mcp__tddy-tools__*`
    /// over LiveKit (`docs/ft/daemon/remote-managed-worktree.md`).
    ///
    /// Atomic by construction: everything that can be resolved locally is resolved *before* the peer
    /// is asked to create anything, and any failure after it has done so tears its session back down.
    /// A half-built split session would strand a worktree on a host with no session left to reclaim
    /// it.
    pub(crate) async fn start_split_claude_cli_session(
        &self,
        os_user: &str,
        codebase_instance_id: &str,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // These two ask for work that only exists on the daemon running the *agent*, which a split
        // session has no repository on. A recipe's tooling runs that host's own `transition`, and a
        // sandboxed spawn jails that host's filesystem; neither is a read another host could serve.
        // Refused rather than silently dropped, because a session that came up without its recipe
        // looks exactly like the session that was asked for.
        if !req.recipe.trim().is_empty() {
            return Err(Status::invalid_argument(
                    "a workflow recipe needs a repository on the daemon running the agent; it cannot be combined with codebase_daemon_instance_id",
                ));
        }
        // `specialized_agents` and `semantic_index` are *not* refused: neither depends on where the
        // codebase lives. An index indexes a worktree, and the one that counts is the codebase
        // host's, so that host builds it. An agent is placeable on any host — co-located with the
        // authoritative worktree it reads that worktree directly, anywhere else it reads a clone the
        // session worktree sync keeps current — so a split placement only decides which host the
        // roster and the clones end up on, which is the codebase host either way.
        //
        // Resolved here for the references naming *this* daemon, before the peer is asked for
        // anything: those are a request error only this host can see, and refusing one after the
        // codebase host had cut a worktree would mean tearing one down to report a typo. References
        // naming another host are resolved by the daemon that holds the roster, from that host's own
        // view of the common room.
        self.resolve_specialized_agent_defs(&req.specialized_agents)
            .await?;

        let slot = self.common_room_slot("StartSession")?.clone();

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_id = Uuid::now_v7().to_string();

        // The workspace session's id is chosen *here*, before the peer is asked for anything, and
        // travels in the request. Letting the peer name it would make the answer the only way to
        // learn the id — so a forward that errors or times out while the peer goes on building
        // would leave a worktree on that host with nothing pointing at it and no way to name it in
        // a teardown. This is what makes the failure atomic rather than merely usually atomic.
        let codebase_session_id = Uuid::now_v7().to_string();

        // Resolved before the peer is contacted: a room this daemon cannot mint a token for means
        // the agent could never reach its checkout, so nothing should be created for it. The room is
        // *this* session's and is hosted here — this daemon runs the agent, so it is the session's
        // facilitating daemon whether or not the repo turns out to live somewhere else.
        let livekit = crate::split_session::SplitLiveKitRoom::from_config(
            &self.config,
            tddy_daemon_livekit::session_room::session_room_name(&session_id),
        )?;

        let workspace_req = agent_roster::workspace_start_request(
            req,
            &local_instance_id_for_config(&self.config),
            &session_id,
            &codebase_session_id,
        )?;
        let forwarded =
            tddy_daemon_livekit::livekit_peer_discovery::forward_start_session_via_livekit_within(
                &slot,
                codebase_instance_id,
                &workspace_req,
                self.split_forward_deadline(),
            )
            .await;
        let workspace = match forwarded {
            Ok(workspace) => workspace,
            Err(status) => {
                // The peer may have created the session and its worktree, may have failed part-way
                // through, or may never have started — none of which this side can distinguish. The
                // teardown covers all three, because the id was ours to begin with.
                self.tear_down_codebase_session(
                    &slot,
                    codebase_instance_id,
                    &codebase_session_id,
                    &req.session_token,
                    SplitStartFailure::from_forward_error(&status),
                )
                .await;
                return Err(status);
            }
        };
        // A branch another session owns is reported, not created: the peer built nothing, so the
        // conflict travels back to the caller as it would for a co-located start.
        if workspace.branch_conflict.is_some() {
            return Ok(Response::new(workspace));
        }
        let created_session_id = workspace.session_id.trim();
        if created_session_id.is_empty() {
            return Err(Status::internal(format!(
                    "daemon {codebase_instance_id} answered StartSession with no session id; the worktree's placement cannot be recorded"
                )));
        }
        if created_session_id != codebase_session_id {
            // A peer that ignored `requested_session_id` cannot give the guarantee above: the next
            // forward it serves slowly would orphan its worktree. Refused rather than accepted with
            // a warning, and the session it did create is torn down under the id it reported.
            self.tear_down_codebase_session(
                &slot,
                codebase_instance_id,
                created_session_id,
                &req.session_token,
                SplitStartFailure::PeerAnswered,
            )
            .await;
            return Err(Status::internal(format!(
                    "daemon {codebase_instance_id} created workspace session {created_session_id:?} instead of the requested {codebase_session_id:?}; it does not honour requested_session_id, so a split session's worktree could not be reclaimed after a failed start"
                )));
        }
        // Nothing about the peer's LiveKit fields is checked here any more: a codebase daemon hosts
        // no room. It holds a checkout and answers `GetWorktreeSnapshot` and tool calls about it,
        // both of which this daemon reaches over the peer routing it already uses.
        let started = self
            .spawn_split_agent(
                os_user,
                &session_id,
                &sessions_base,
                codebase_instance_id,
                &codebase_session_id,
                Some(&livekit),
                req,
                progress,
            )
            .await;

        match started {
            Ok(response) => Ok(response),
            Err(status) => {
                // The agent spawn is this daemon's own work: the peer already answered, and
                // whatever it built is there to be reclaimed.
                self.tear_down_codebase_session(
                    &slot,
                    codebase_instance_id,
                    &codebase_session_id,
                    &req.session_token,
                    SplitStartFailure::PeerAnswered,
                )
                .await;
                Err(status)
            }
        }
    }
}
