use std::path::Path;

use crate::{
    connection_service::{agent_roster, seed_codebase, seeded_clone_guard},
    livekit_peer_discovery::local_instance_id_for_config,
    workspace_session,
};

use tddy_core::session_lifecycle::unified_session_dir_path;

use uuid::Uuid;

use tddy_rpc::Status;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// [`Self::ensure_session_room`] for the attach path, so an owning daemon has something to be
    /// admitted to.
    ///
    /// A peer told to join a room nobody opened waits out its deadline against a participant that
    /// never arrives, so this is done *before* the peer is asked for anything.
    pub(crate) async fn ensure_session_room_for_agents(
        &self,
        session_id: &str,
        codebase: &seed_codebase::SeedCodebase,
    ) -> Result<(), Status> {
        // Asked before the checkout is, because the two questions are independent and only one of
        // them is a precondition. A session already hosting its room needs nothing from this call,
        // including a local checkout — under split placement it has none, and demanding one here
        // would refuse an attach to a room that is open and serving.
        if self.session_rooms.hosts(session_id) {
            return Ok(());
        }
        let worktree_root = codebase.worktree_root.clone().ok_or_else(|| {
            Status::failed_precondition(format!(
                "session '{session_id}' has no checkout on this daemon, so its room cannot be \
                 opened here; a remote agent reads a mirror of that checkout and there is nothing \
                 to mirror"
            ))
        })?;
        match self
            .ensure_session_room(session_id, codebase.session_dir.as_path(), &worktree_root)
            .await?
        {
            Some(room) => {
                log::info!(
                    "AttachSessionAgent: opened {} as {} so an owning daemon can be admitted to it",
                    room.room,
                    room.server_identity
                );
                Ok(())
            }
            // A daemon with no LiveKit credentials hosts no rooms, which is fine for a local agent
            // and impossible for a remote one: there would be no room to sync the clone from and no
            // route to the owning daemon.
            None => Err(Status::failed_precondition(format!(
                "session '{session_id}' cannot take an agent from another daemon: this daemon has \
                 no LiveKit configuration, so it hosts no session room for that daemon to join"
            ))),
        }
    }

    /// Claim the clone serving `daemon_instance_id`'s agents on this session, and start building it
    /// if this is the first agent that daemon owns here.
    ///
    /// Returns the clone's `workspace` session id, which the roster entry records. The id is minted
    /// **before** the peer is contacted and travels in the request as `requested_session_id`, so a
    /// forward that never answers still leaves this daemon able to name — and therefore tear down —
    /// whatever the peer built.
    pub(crate) async fn claim_agent_clone(
        &self,
        session_id: &str,
        codebase: &seed_codebase::SeedCodebase,
        daemon_instance_id: &str,
        session_token: &str,
    ) -> Result<seed_codebase::ClaimedAgentClone, Status> {
        // Before the claim: a room this daemon could not open is a clone that could never sync, and
        // a claim recorded for it would leave the roster naming a checkout nobody will build.
        self.ensure_session_room_for_agents(session_id, codebase)
            .await?;

        let (codebase_session_id, provision) =
            self.session_agent_clones
                .claim(session_id, daemon_instance_id, || {
                    Uuid::now_v7().to_string()
                });
        if !provision {
            return Ok(seed_codebase::ClaimedAgentClone {
                codebase_session_id,
                commissioned: false,
            });
        }

        // Spawned rather than awaited: the peer resolves the project — cloning it if it does not
        // have it — and cuts a worktree, which its own `spawn_worker_request_timeout` bounds at five
        // minutes. Holding the attach open for that would make a 90-second `git clone` look like a
        // hung RPC, so the entry is published PROVISIONING and republished when the peer reports.
        let service = self.clone();
        let session_id = session_id.to_string();
        let daemon_instance_id = daemon_instance_id.to_string();
        let codebase = codebase.clone();
        let clone_id = codebase_session_id.clone();
        let session_token = session_token.to_string();
        tokio::spawn(async move {
            if let Err(status) = service
                .provision_agent_clone(
                    &session_id,
                    &codebase,
                    &daemon_instance_id,
                    &clone_id,
                    &session_token,
                )
                .await
            {
                log::error!(
                    "AttachSessionAgent: could not build session {session_id}'s clone \
                     {clone_id} on daemon {daemon_instance_id}: {}",
                    status.message()
                );
                service.session_agent_clones.fail(
                    &session_id,
                    &daemon_instance_id,
                    status.message(),
                );
            }
            // The attach that commissioned this clone may have failed while the peer was still
            // cutting the checkout. Its unwind cannot delete a session the peer had not created yet,
            // so the side that knows the checkout now exists finishes the job: no claim under this
            // id means nothing will ever read it.
            let still_claimed = service
                .session_agent_clones
                .get(&session_id, &daemon_instance_id)
                .is_some_and(|clone| clone.codebase_session_id == clone_id);
            if !still_claimed {
                log::info!(
                    "AttachSessionAgent: session {session_id} no longer claims clone {clone_id}; \
                     deleting it on daemon {daemon_instance_id}"
                );
                service
                    .delete_clone_on_peer(&daemon_instance_id, &clone_id, &session_token)
                    .await;
                return;
            }
            // Either way the roster now says something new about the clone, and a subscriber that
            // only heard about `rev` changes would show `provisioning` until an attach that may
            // never come.
            service
                .publish_roster_change(&session_id, &codebase.session_dir)
                .await;
        });
        Ok(seed_codebase::ClaimedAgentClone {
            codebase_session_id,
            commissioned: true,
        })
    }

    /// Give back a clone this attach commissioned, because the attach did not complete.
    ///
    /// Only the call that *commissioned* the checkout may take it away: a second agent on the same
    /// host shares the first one's clone, and unwinding that one would delete a checkout the roster
    /// still names (PRD § One clone per (session, remote daemon)).
    ///
    /// The claim is dropped before the peer is asked, so a checkout the peer creates after this ran
    /// is deleted by the provisioning task that created it rather than left orphaned.
    pub(crate) async fn unwind_agent_clone_claim(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        claimed: &seed_codebase::ClaimedAgentClone,
        session_token: &str,
    ) {
        if !claimed.commissioned {
            return;
        }
        self.session_agent_clones
            .forget(session_id, daemon_instance_id);
        self.delete_clone_on_peer(
            daemon_instance_id,
            &claimed.codebase_session_id,
            session_token,
        )
        .await;
    }

    /// Build the semantic index for a `workspace` session's worktree.
    ///
    /// The index indexes a worktree, and the worktree that counts is this daemon's: for the codebase
    /// half of a split session, the agent runs on another host with no repository on disk, so this is
    /// the only host that has anything to index (docs/ft/coder/semantic-index.md).
    ///
    /// Blocking, and a failure fails the start — no unindexed fallback, exactly as on the co-located
    /// paths: a session that came up without the index it asked for looks like the session that was
    /// asked for.
    ///
    /// TODO(seeded-agents-on-any-placement): export `TDDY_SEMANTIC_INDEX_DB` on this daemon's
    /// exec-tool surface, so a split session's `mcp__tddy-tools__SemanticSearch` reaches the index
    /// this built rather than reporting it unset. `tool_engine::execute_tool` takes no env pairs on
    /// the daemon's own surface, and the query side is unwired regardless
    /// (`packages/tddy-tool-engine/src/lib.rs` — "index query not yet wired").
    pub(crate) async fn index_workspace_worktree(
        &self,
        sessions_base: &Path,
        session_id: &str,
    ) -> Result<(), Status> {
        let worktree_path =
            workspace_session::resolve_worktree_root_for_session(sessions_base, session_id)?;
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        let embedder =
            tddy_semantic_index::production_embedder(&self.tddy_data_dir).map_err(|e| {
                Status::failed_precondition(format!(
                    "semantic index requested but no embedder is available: {e}"
                ))
            })?;
        tddy_semantic_index::semantic_index::run_semantic_index_blocking(
            &worktree_path,
            &session_dir,
            embedder,
            &self.task_registry,
            session_id,
        )
        .await
        .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
        log::info!(
            "StartSession: indexed workspace session {session_id}'s worktree at {}",
            worktree_path.display()
        );
        Ok(())
    }

    /// Build the jail a sandboxed `workspace` session runs its tools in, and register it under the
    /// session id every later dispatch looks it up by
    /// (`docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox).
    ///
    /// A host with no sandbox backend, or a jail that will not come up, is an error — never a start
    /// that succeeds unconfined.
    pub(crate) async fn provision_workspace_tool_sandbox(
        &self,
        sessions_base: &Path,
        session_id: &str,
    ) -> Result<(), Status> {
        let spec = crate::workspace_tool_sandbox::WorkspaceSandboxSpec {
            session_id: session_id.to_string(),
            session_dir: unified_session_dir_path(sessions_base, session_id),
            worktree_path: workspace_session::resolve_worktree_root_for_session(
                sessions_base,
                session_id,
            )?,
        };
        let jail = self
            .workspace_sandbox_provisioner
            .provision(&spec)
            .await
            .map_err(crate::sandbox_session::sandbox_error_to_status)?;
        self.workspace_sandboxes
            .insert(session_id.to_string(), jail)
            .await;
        log::info!(
            "StartSession: workspace session {session_id} runs its tools in a jail holding {}",
            spec.worktree_path.display()
        );
        Ok(())
    }

    /// Record a session's seeded roster, giving every agent that is not co-located with this
    /// daemon's worktree the clone it reads.
    ///
    /// The seed's counterpart to [`Self::attach_session_agent`], step for step and by the same
    /// calls: a withdrawal this session could not enforce is refused, a clone is claimed for an
    /// agent owned by a peer — which is also what opens the session's room — and only then is the
    /// entry written. Called **before** the agent is spawned, because the spawn fixes its tool
    /// allowlist at launch: a roster written afterwards would leave the seed's `replaces`
    /// unenforced until the first resume (docs/ft/daemon/session-agent-roster.md
    /// § Seeding at start).
    ///
    /// Atomic across the whole seed, not per entry: a refusal on the third agent takes the first
    /// two back out — entry, clone and room membership — so the caller never has to reason about a
    /// half-seeded roster it cannot see. The order within the unwind is the detach path's, for the
    /// same reason: the entry goes first, so nothing is left naming a checkout that is being
    /// deleted.
    ///
    /// On success the artifacts are handed back rather than dropped: a start can still fail at a
    /// step *after* the seed, and only this list says what to take away. The caller owns them from
    /// here — see [`Self::unwind_seeded_roster`].
    pub(crate) async fn seed_session_agent_roster(
        &self,
        session_id: &str,
        codebase: &seed_codebase::SeedCodebase,
        session_token: &str,
        records: Vec<tddy_core::SessionAgentRecord>,
    ) -> Result<Vec<seeded_clone_guard::SeededAgent>, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        let mut seeded: Vec<seeded_clone_guard::SeededAgent> = Vec::with_capacity(records.len());
        for mut record in records {
            if let Err(status) =
                agent_roster::refuse_unenforceable_withdrawal(session_id, codebase, &record)
            {
                self.unwind_seeded_roster(session_id, codebase, session_token, seeded)
                    .await;
                return Err(status);
            }
            let seeded_daemon = record.daemon_instance_id.clone();
            let mut clone = None;
            if record.daemon_instance_id != local_instance_id {
                match self
                    .claim_agent_clone(
                        session_id,
                        codebase,
                        &record.daemon_instance_id,
                        session_token,
                    )
                    .await
                {
                    Ok(claimed) => {
                        record.codebase_session_id = Some(claimed.codebase_session_id.clone());
                        clone = Some(claimed);
                    }
                    Err(status) => {
                        self.unwind_seeded_roster(session_id, codebase, session_token, seeded)
                            .await;
                        return Err(status);
                    }
                }
            }
            let agent_id = record.agent_id.clone();
            let written =
                self.session_agent_rosters
                    .attach(session_id, &codebase.session_dir, record);
            // Recorded as seeded before the write is inspected: the clone claimed a moment ago is
            // the half of this entry a peer has already been told to build, so a failed write must
            // still be able to take it away.
            seeded.push(seeded_clone_guard::SeededAgent {
                agent_id: agent_id.clone(),
                daemon_instance_id: seeded_daemon,
                clone,
            });
            match written {
                Ok(roster) => log::info!(
                    "StartSession: session {session_id} seeded agent '{agent_id}' at rev {}",
                    roster.rev
                ),
                Err(status) => {
                    self.unwind_seeded_roster(session_id, codebase, session_token, seeded)
                        .await;
                    return Err(status);
                }
            }
        }
        Ok(seeded)
    }

    /// Claim the clones a co-located start's seeded roster needs, before its agent is spawned.
    ///
    /// The co-located twin of [`Self::seed_session_agent_roster`], and deliberately not the same
    /// call: a co-located start persists its roster *inline* in the `.session.yaml` it writes once
    /// the agent has a pid, so there is no entry to write here. What cannot wait for that file is
    /// the clone — a peer builds it over the session's room, and the agent it serves is about to
    /// launch — which is why the facts it needs are passed in rather than read back off disk.
    ///
    /// Each record is stamped with the checkout its agent will read, so the roster the metadata
    /// write persists names it. A record of this daemon's own agent reads the authoritative
    /// worktree and names no clone, exactly as on every other path.
    pub(crate) async fn claim_co_located_seed_clones(
        &self,
        session_id: &str,
        codebase: &seed_codebase::SeedCodebase,
        session_token: &str,
        records: &mut [tddy_core::SessionAgentRecord],
    ) -> Result<seeded_clone_guard::SeededCloneGuard, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        // Built before the first claim so an early return releases what the loop got through: the
        // guard is the only thing that knows a peer was asked to build a checkout.
        let mut guard =
            seeded_clone_guard::SeededCloneGuard::claiming(self.clone(), session_id, session_token);
        for record in records.iter_mut() {
            agent_roster::refuse_unenforceable_withdrawal(session_id, codebase, record)?;
            if record.daemon_instance_id == local_instance_id {
                continue;
            }
            let claimed = self
                .claim_agent_clone(
                    session_id,
                    codebase,
                    &record.daemon_instance_id,
                    session_token,
                )
                .await?;
            record.codebase_session_id = Some(claimed.codebase_session_id.clone());
            guard.claimed(seeded_clone_guard::SeededAgent {
                agent_id: record.agent_id.clone(),
                daemon_instance_id: record.daemon_instance_id.clone(),
                clone: Some(claimed),
            });
        }
        Ok(guard)
    }

    /// Take a partially seeded roster back out, so a failed start leaves no entry, no half-built
    /// clone and no room membership on any host.
    ///
    /// Every failure is logged and none is propagated: the caller is already returning the refusal
    /// that brought it here, and replacing that with "the unwind also failed" would send an
    /// operator after the wrong problem. The last agent in `seeded` may never have reached the
    /// roster, which is why a detach that finds nothing is not treated as an error either.
    pub(crate) async fn unwind_seeded_roster(
        &self,
        session_id: &str,
        codebase: &seed_codebase::SeedCodebase,
        session_token: &str,
        seeded: Vec<seeded_clone_guard::SeededAgent>,
    ) {
        for agent in seeded.into_iter().rev() {
            if let Err(status) = self.session_agent_rosters.detach(
                session_id,
                &codebase.session_dir,
                &agent.agent_id,
            ) {
                log::warn!(
                    "StartSession: could not take seeded agent '{}' back out of session \
                     {session_id}'s roster: {}",
                    agent.agent_id,
                    status.message()
                );
            }
            if let Some(clone) = &agent.clone {
                self.unwind_agent_clone_claim(
                    session_id,
                    &agent.daemon_instance_id,
                    clone,
                    session_token,
                )
                .await;
            }
        }
    }
}
