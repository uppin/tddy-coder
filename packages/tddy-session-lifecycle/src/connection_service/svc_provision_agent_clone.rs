// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use crate::tool_engine;
use prost::Message as _;
use tddy_service::proto::exec_tools::ExecuteToolResponse;

use tddy_service::proto::exec_tools::ExecuteToolRequest;

use std::{path::Path, sync::Arc};

use std::path::PathBuf;

use tddy_service::proto::session_agents_svc::SessionAgentRoster;

use super::peer_has_no_such_session;

use tddy_service::proto::session::DeleteSessionRequest;

use crate::{
    connection_service::{agent_roster, hooks_and_urls, seed_codebase},
    livekit_peer_discovery::local_instance_id_for_config,
};

use tddy_service::proto::session::StartSessionRequest;

use tddy_rpc::Status;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Ask `daemon_instance_id` for the checkout this session's agents on it will read.
    ///
    /// The same `workspace`-session primitive a split placement uses, and for the same reasons: the
    /// peer knows how to provision a project by clone when it does not have one, how to cut a
    /// worktree for it, and how to report and delete it — and an operator sees the result as an
    /// ordinary session rather than as a directory the daemon made up.
    ///
    /// The placement carries this daemon's Connect-HTTP root (`facilitating_daemon_url`) so a peer
    /// that has never seen the project clones it from this daemon's `remote_git.RemoteGitService`
    /// rather than from its own peer fan-out — `git clone {facilitating_instance_id}:{project_id}`
    /// with `GIT_SSH_COMMAND=tddy-remote-git-repo`, the transport `tddy-session-sync` already uses
    /// (`docs/ft/daemon/remote-git-repo.md`). That closes the two cases the fan-out fails: a peer in
    /// this daemon's room that keeps no room of its own has nobody to ask, and a project whose
    /// `git_url` names a forge the peer cannot reach has nothing to clone (PRD AC37).
    pub(crate) async fn provision_agent_clone(
        &self,
        session_id: &str,
        codebase: &seed_codebase::SeedCodebase,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("AttachSessionAgent")?.clone();
        // The room-admission handshake (PRD § "What attach does" step 3): the facilitating daemon
        // records the owning daemon in the per-session admission registry and mints the scoped,
        // short-TTL token it forwards along with the StartSession. The owning daemon joins
        // `session-{session_id}` with this token and nothing else, then runs the re-admit loop
        // against `session_admission.SessionAdmissionService/AdmitOwningDaemon` before it expires.
        // `None` (LiveKit not configured) falls back to the owning daemon self-minting — a recorded
        // deviation, never a silent one.
        let first_admission = self.mint_first_admission_token(session_id, daemon_instance_id);
        let (first_admission_token, first_admission_url, first_admission_room, _ttl) =
            match first_admission {
                Some((token, url, room, ttl)) => (token, url, room, ttl),
                None => {
                    log::warn!(
                        "session {session_id}: facilitating daemon could not mint an admission \
                         token for owning daemon {daemon_instance_id}; the owning daemon will fall \
                         back to self-minting a room token (PRD § Deviations — no handshake)"
                    );
                    (String::new(), String::new(), String::new(), 0u64)
                }
            };
        let request = StartSessionRequest {
            session_token: session_token.to_string(),
            session_type: "workspace".to_string(),
            project_id: codebase.project_id.clone(),
            // Empty: the peer holds this checkout itself and must not route the request onward.
            daemon_instance_id: String::new(),
            codebase_daemon_instance_id: String::new(),
            requested_session_id: codebase_session_id.to_string(),
            agent_clone: Some(tddy_service::proto::session::AgentClonePlacement {
                session_id: session_id.to_string(),
                facilitating_daemon_instance_id: local_instance_id_for_config(&self.config),
                facilitating_daemon_url: hooks_and_urls::advertise_daemon_url(&self.config),
                first_admission_token,
                first_admission_url,
                first_admission_room,
            }),
            ..StartSessionRequest::default()
        };
        // The split forward's deadline, for the split forward's reason: giving up after the ordinary
        // 30 s would mean erroring while the peer is still cloning, and a peer that carried on would
        // leave a checkout on a host nobody is watching.
        let answered =
            tddy_daemon_livekit::livekit_peer_discovery::forward_start_session_via_livekit_within(
                &slot,
                daemon_instance_id,
                &request,
                self.split_forward_deadline(),
            )
            .await?;
        let created = answered.session_id.trim();
        if created != codebase_session_id {
            // A peer that ignored `requested_session_id` cannot give the guarantee above, so what it
            // did create is torn down under the id it reported and the clone fails.
            self.delete_clone_on_peer(daemon_instance_id, created, session_token)
                .await;
            return Err(Status::internal(format!(
                "daemon {daemon_instance_id} created workspace session {created:?} instead of the \
                 requested {codebase_session_id:?}; it does not honour requested_session_id, so \
                 this clone could not be reclaimed after a failed attach"
            )));
        }
        // Nothing is marked READY here. The peer joins the session room and restores the checkout
        // from the session's WIP ref after it has answered, and only it can say when that is done —
        // so it reports (`ReportAgentCloneState`) and this daemon waits to be told. Marking readiness
        // from the side that cannot see the checkout is how a prompt gets served from an empty tree.
        log::info!(
            "AttachSessionAgent: daemon {daemon_instance_id} is building session {session_id}'s \
             clone as workspace session {codebase_session_id}"
        );
        Ok(())
    }

    /// Delete a clone's `workspace` session on the daemon holding it.
    ///
    /// Used only to unwind a failed provisioning, where the caller already has the more useful error
    /// to return — so a teardown failure names the orphan in the log rather than replacing it.
    pub(crate) async fn delete_clone_on_peer(
        &self,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) {
        let Ok(slot) = self.common_room_slot("AttachSessionAgent") else {
            log::error!(
                "could not reach the common room to delete workspace session \
                 {codebase_session_id} on daemon {daemon_instance_id}; its checkout is orphaned there"
            );
            return;
        };
        match tddy_daemon_livekit::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            daemon_instance_id,
            &DeleteSessionRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session_id.to_string(),
            },
        )
        .await
        {
            Ok(_) => log::info!(
                "deleted workspace session {codebase_session_id} on daemon {daemon_instance_id}"
            ),
            Err(e) if peer_has_no_such_session(&e) => log::info!(
                "daemon {daemon_instance_id} no longer has workspace session \
                 {codebase_session_id} ({e}); it was already torn down"
            ),
            Err(e) => log::error!(
                "could not delete workspace session {codebase_session_id} on daemon \
                 {daemon_instance_id} ({e}); its checkout is orphaned there"
            ),
        }
    }

    /// Tear down the clone `daemon_instance_id` holds for this session.
    ///
    /// Follows the discipline `docs/ft/daemon/remote-managed-worktree.md` § Teardown established:
    /// the peer answering **"no such session" is success** — the clone is an ordinary listable
    /// session an operator may have deleted directly, and treating that as an error would make the
    /// agent permanently undetachable, with a message naming a checkout that no longer exists. Only
    /// *unreachable or failed* refuses, naming the checkout left behind.
    ///
    /// The refusals say nothing about what the caller has already changed locally, because the two
    /// callers differ: `DetachSessionAgent` has removed and persisted the entry before it gets here,
    /// while `DeleteSession` has deleted nothing yet and is refused outright. Each adds that half
    /// itself.
    pub(crate) async fn tear_down_agent_clone(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("DetachSessionAgent")?;
        // Being *configured* for a common room is not being in one, and the two failures are
        // indistinguishable from their status codes alone: a forward attempted with no room fails
        // locally with `failed_precondition`, exactly as a peer that does not have the session does.
        // Without this check a momentary disconnect would read as "already torn down".
        if slot.read().await.is_none() {
            return Err(Status::failed_precondition(format!(
                "cannot reach the common room to delete session {session_id}'s clone \
                 {codebase_session_id} on daemon {daemon_instance_id}, so its checkout is still \
                 there; delete workspace session {codebase_session_id} on {daemon_instance_id} once \
                 the daemons can see each other"
            )));
        }
        match tddy_daemon_livekit::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            daemon_instance_id,
            &DeleteSessionRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session_id.to_string(),
            },
        )
        .await
        {
            Ok(_) => log::info!(
                "deleted session {session_id}'s clone {codebase_session_id} on daemon \
                 {daemon_instance_id}"
            ),
            Err(e) if peer_has_no_such_session(&e) => log::info!(
                "daemon {daemon_instance_id} no longer has session {session_id}'s clone \
                 {codebase_session_id} ({e}); it was already torn down, so the detach continues"
            ),
            Err(e) => {
                return Err(Status::internal(format!(
                    "could not delete session {session_id}'s clone {codebase_session_id} on daemon \
                     {daemon_instance_id} ({e}); its checkout is left behind there — delete \
                     workspace session {codebase_session_id} on {daemon_instance_id} directly"
                )))
            }
        }
        self.session_agent_clones
            .forget(session_id, daemon_instance_id);
        // The admission this clone's owning daemon held is gone with it: a later `AdmitOwningDaemon`
        // call from that daemon (its mirror's re-admit loop, or a stale retry) must now refuse, and
        // the only way it can refuse is if the registry no longer lists the daemon as admitted. This
        // is the revocation half of the handshake (PRD § "What attach does" step 3) — the detach
        // path. The session-delete path revokes every admitted daemon at once via
        // `revoke_all_for_session`.
        let revoked = self
            .session_admissions
            .revoke(session_id, daemon_instance_id);
        if revoked {
            log::info!(
                "revoked admission for daemon {daemon_instance_id} to session {session_id} \
                 (last agent detached)"
            );
        }
        Ok(())
    }

    /// Delete every clone a session created, on every host that built one.
    ///
    /// Called by `DeleteSession`. One failure fails the deletion naming the orphan, for the same
    /// reason a split session's paired workspace does: a delete that succeeded locally while a
    /// checkout survived on another host is exactly the silent leak the pairing exists to prevent.
    pub(crate) async fn tear_down_every_agent_clone(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        for (daemon_instance_id, clone) in self.session_agent_clones.for_session(session_id) {
            self.tear_down_agent_clone(
                session_id,
                &daemon_instance_id,
                &clone.codebase_session_id,
                session_token,
            )
            .await?;
        }
        Ok(())
    }

    /// Push the session's current roster to its `StreamSessionAgents` subscribers and to its room.
    ///
    /// Best-effort on the room and loud on the stream: a subscriber that cannot be reached is a
    /// consumer running on a stale roster, which is the failure this whole feature exists to
    /// prevent, while a room that is not open is the ordinary state of a session whose daemon has
    /// no LiveKit configuration.
    pub(crate) async fn publish_roster_change(&self, session_id: &str, session_dir: &Path) {
        if let Err(e) = self
            .session_agent_rosters
            .republish(session_id, session_dir)
        {
            log::warn!(
                "could not republish session {session_id}'s roster to its subscribers: {}",
                e.message()
            );
            return;
        }
        let Ok(roster) = self.session_agent_rosters.snapshot(session_id, session_dir) else {
            return;
        };
        self.broadcast_roster(session_id, &roster).await;
    }

    /// Broadcast one roster snapshot on the session room's `session.agents` topic.
    ///
    /// Published once, to the whole room, with no `destination_identities` — the same broadcast
    /// discipline `worktree.activity` follows. Every frame is a whole snapshot that every
    /// participant of the session is entitled to, and addressing it would mean the publisher
    /// deciding who is interested, which it cannot know: a browser tab or a newly admitted owning
    /// daemon joins at any time.
    pub(crate) async fn broadcast_roster(&self, session_id: &str, roster: &SessionAgentRoster) {
        let Some(publisher) = self.session_rooms.agents_publisher(session_id) else {
            return;
        };
        if let Err(e) = publisher.publish(&roster.encode_to_vec()).await {
            log::warn!(
                "could not broadcast session {session_id}'s roster on \
                 {}: {e}",
                crate::session_room::SESSION_AGENTS_TOPIC
            );
        }
    }

    /// The identities currently joined to a session's room, as the LiveKit server reports them.
    ///
    /// Read from the server API rather than from this daemon's own connection: the question is who
    /// is in the room, and only the server can answer it for participants this daemon did not admit.
    pub async fn session_room_participant_identities(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, Status> {
        let room_name = tddy_daemon_livekit::session_room::session_room_name(session_id);
        let rooms = self.room_roster.list_rooms().await.map_err(Status::from)?;
        let room = rooms
            .into_iter()
            .find(|room| room.name == room_name)
            .ok_or_else(|| {
                Status::not_found(format!(
                    "the LiveKit server has no room called {room_name}; session {session_id} is \
                     not being facilitated in one"
                ))
            })?;
        let mut identities: Vec<String> = room
            .participants
            .into_iter()
            .map(|participant| participant.identity)
            .collect();
        identities.sort();
        Ok(identities)
    }

    /// Where the checkout serving `agent_id` is, on the daemon that owns it.
    ///
    /// Reported by that daemon rather than derived here: this daemon does not have the filesystem
    /// the path names, and a path it computed would describe a directory nobody created.
    pub async fn agent_clone_worktree_path(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<PathBuf, Status> {
        let clone = self.agent_clone_for(session_id, agent_id)?;
        clone.worktree_path.ok_or_else(|| {
            Status::failed_precondition(format!(
                "the daemon owning '{agent_id}' has not reported where session {session_id}'s \
                 clone landed (its state is {:?})",
                clone.state
            ))
        })
    }

    /// Every reconcile the daemon owning `agent_id` has reported for this session's clone.
    ///
    /// A reconcile is never silent: a mirror that repairs itself without saying so hides a real
    /// fault, which here is a second writer nobody knows about.
    pub async fn agent_clone_divergences(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>, Status> {
        Ok(self.agent_clone_for(session_id, agent_id)?.divergences)
    }

    /// The clone serving `agent_id`, found through the roster entry that names its owning daemon.
    pub(crate) fn agent_clone_for(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<crate::session_agent_clone::AgentClone, Status> {
        let session_dir = self.session_dir_for(session_id)?;
        let record = self
            .session_agent_rosters
            .entry(session_id, &session_dir, agent_id)?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "agent '{agent_id}' is not attached to session '{session_id}'"
                ))
            })?;
        self.session_agent_clones
            .get(session_id, &record.daemon_instance_id)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "agent '{agent_id}' is served locally by daemon \
                     '{}', which works the session's own worktree and has no clone",
                    record.daemon_instance_id
                ))
            })
    }

    /// The clone this daemon hosts for `session_id`, when it holds one.
    ///
    /// What makes an exec tool addressed at this daemon for another daemon's session resolvable at
    /// all: the session lives elsewhere, so the ordinary "resolve the worktree from my own sessions
    /// base" would find nothing.
    pub(crate) fn hosted_clone_for(
        &self,
        session_id: &str,
    ) -> Option<Arc<crate::session_agent_clone::HostedClone>> {
        self.hosted_agent_clones.get(session_id)
    }

    /// Serve one exec tool for a session whose checkout this daemon holds as an agent clone.
    ///
    /// This is where the read/write split actually happens, so the agent's own turn loop and an
    /// exec-tool RPC addressed here take exactly one path. A read is answered from the clone with no
    /// round trip — which is the entire reason for placing an agent on this host — and a mutation is
    /// proxied to the facilitating daemon's authoritative worktree.
    ///
    /// Which tree is worked is settled by the clone link rather than by the caller: a hosted clone is
    /// a checkout this daemon built for exactly one session on exactly one peer, at the request of a
    /// `StartSession` it already authenticated, so the request cannot select any other tree and the
    /// OS user the tools run as was settled then. *Who may drive them* is settled by the caller's
    /// session token, which every path into here — the RPC handlers and this daemon's own agent turn
    /// loop — establishes before this is reached: the mutating half proxies to the facilitating
    /// daemon under the clone's stored credential, so an unauthenticated caller reaching here would
    /// be writing into another host's authoritative worktree under a credential it never held.
    ///
    /// TODO(session-agent-roster): narrow that to a session-scoped tool token — audience = this
    /// clone's session, exec-tool methods only — which is the same credential the split placement's
    /// trust model already wants and `docs/dev/TODO.md` already records.
    pub(crate) async fn run_hosted_clone_tool(
        &self,
        req: &ExecuteToolRequest,
        clone: &crate::session_agent_clone::HostedClone,
    ) -> ExecuteToolResponse {
        if !agent_roster::agent_tool_reads_the_clone(&req.tool_name) {
            return match clone
                .execute_tool_on_facilitator(&req.tool_name, &req.args_json)
                .await
            {
                Ok(result_json) => ExecuteToolResponse {
                    result_json,
                    is_error: false,
                    error_message: String::new(),
                    job_id: String::new(),
                    job_running: false,
                },
                // Carried in the response rather than raised, exactly as a locally-run tool failure
                // is: the agent asked for a tool and the tool did not happen, which is a tool result
                // and not a transport failure.
                Err(status) => ExecuteToolResponse {
                    result_json: serde_json::json!({ "error": status.message() }).to_string(),
                    is_error: true,
                    error_message: status.message().to_string(),
                    job_id: String::new(),
                    job_running: false,
                },
            };
        }
        let outcome = tool_engine::execute_tool(
            &clone.worktree_path,
            &req.tool_name,
            &req.args_json,
            &self.task_registry,
            &req.session_id,
        )
        .await;
        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }
}
