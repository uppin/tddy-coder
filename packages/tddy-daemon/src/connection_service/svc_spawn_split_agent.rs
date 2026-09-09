use tddy_core::read_session_metadata;

use tddy_core::session_lifecycle::unified_session_dir_path;



use super::peer_has_no_such_session;

use tddy_service::proto::connection::DeleteSessionRequest;

use super::SplitStartFailure;

use livekit::prelude::Room;

use std::time::Duration;

use uuid::Uuid;

use crate::{connection_service::hooks_and_urls, livekit_peer_discovery::local_instance_id_for_config};

use std::sync::Arc;

use super::AttachmentMaterialization;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::connection::StartSessionResponse;

use tddy_rpc::Response;

use super::AttachmentProgressSink;

use tddy_service::proto::connection::StartSessionRequest;

use std::path::Path;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Spawn the agent half of a split session and record the pairing.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_split_agent(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: &Path,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        livekit: &crate::split_session::SplitLiveKitRoom,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
        let materialized = self
            .prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user,
                sessions_base,
                session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
        // Where the codebase lives is not a reason for a planned PR's child to come up without its
        // boundaries: the same rule the co-located branches apply, on the attachments this host
        // materialized into the session it is about to run the agent for.
        let initial_prompt = crate::stack_doc_attachments::prompt_with_attached_changeset(
            req.initial_prompt.trim(),
            &materialized,
        );

        let tddy_tools_path = self.resolve_tddy_tools_path();
        let remote = crate::split_session::split_remote_tool_env(
            livekit,
            session_id,
            codebase_instance_id,
            codebase_session_id,
            &req.session_token,
        )?;
        // This daemon runs the agent, so it is this session's facilitating daemon and hosts its room —
        // even though the checkout is on `codebase_instance_id`. Opened before the agent is spawned
        // (PRD FR2), and measured by asking the codebase daemon rather than by reading a filesystem
        // this host does not have (FR5). The agent's token was minted for exactly this room.
        //
        // The poller signs its own credential per poll under the verified caller's identity rather
        // than re-presenting `req.session_token`, which the codebase daemon stops accepting five
        // minutes in — see `RoomPollTokenMinter`.
        let token_minter = Arc::new(crate::split_session::RoomPollTokenMinter::new(
            &livekit.api_secret,
            &req.session_token,
        )?);
        let remote_source = Arc::new(crate::session_room::RemoteCheckout::new(
            Arc::new(self.clone()),
            codebase_session_id.to_string(),
            codebase_instance_id.to_string(),
            token_minter,
            session_dir.clone(),
        ));
        let local_instance_id = local_instance_id_for_config(&self.config);
        match self
            .session_rooms
            .open_measured_by(
                &crate::session_room::DaemonRoomHosting {
                    config: &self.config,
                    instance_id: &local_instance_id,
                    rooms: &self.session_rooms,
                }
                .for_remote_worktree(session_id, &session_dir),
                tddy_service::ConnectionServiceServer::new(self.clone()),
                remote_source,
            )
            .await?
        {
            Some(room) => log::info!(
                "split session {session_id} facilitated in {} as {}, measuring session {codebase_session_id} on daemon {codebase_instance_id}",
                room.room,
                room.server_identity
            ),
            None => log::debug!(
                "split session {session_id} runs without a session room (LiveKit not configured)"
            ),
        }

        // A split session's roster lives on the codebase daemon, in the workspace session the
        // forward created moments ago — which is where this start's seed was recorded, before that
        // call answered. So the withdrawals are read back from the host that holds them, exactly as
        // the resume path reads them (`resume_split_wiring`): `--allowedTools` is fixed at launch,
        // and a seeded agent whose `replaces` reached the spawn as an empty list would leave the
        // main agent holding the tools it gave away until the first resume.
        //
        // A start that seeded nothing wrote nothing, so there is nothing to read and no forward is
        // spent asking: agents on such a session arrive later through `AttachSessionAgent`, which
        // relaunches the agent against the roster it just changed.
        let withdrawals = match req.specialized_agents.is_empty() {
            true => Vec::new(),
            false => {
                self.split_withdrawals_from_codebase_host(
                    &req.session_token,
                    codebase_session_id,
                    codebase_instance_id,
                )
                .await?
            }
        };
        // The agent's working directory is populated **before** the agent process exists (AC23): a
        // split agent that starts, reads its cwd and finds only the notice has already missed the
        // project's rules for its first turn. A fetch that fails fails the start, and the caller
        // tears down the worktree this start created on the codebase daemon (AC24).
        let agent = crate::context_files::context_agent_for_session_type("claude-cli");
        let context = self
            .split_context_from_codebase_host(
                &req.session_token,
                codebase_session_id,
                codebase_instance_id,
                agent,
                "start",
            )
            .await?;
        let context_dir = crate::split_session::build_split_context_dir(
            &session_dir,
            &withdrawals,
            tddy_core::backend::context_globs_for_agent(agent),
            &context,
        )?;
        let extra_args = crate::split_session::split_claude_extra_args(
            &session_dir,
            &tddy_tools_path.to_string_lossy(),
            &withdrawals,
        )?;

        // Claude Code reads `.claude/settings.local.json` from its working directory, which for a
        // split session is the context dir rather than a worktree. Best-effort, as elsewhere: a
        // missing hook file costs status reporting, not the session.
        let hook_token = Uuid::new_v4().to_string();
        hooks_and_urls::write_claude_hooks_settings(
            &context_dir,
            &tddy_core::HookCommandParams {
                tddy_tools_path: &tddy_tools_path.to_string_lossy(),
                daemon_url: &hooks_and_urls::claude_hook_daemon_url(&self.config),
                session_id,
                os_user,
                hook_token: &hook_token,
            },
        );

        let handle = self
            .claude_cli_manager
            .start_with_options(
                session_id,
                context_dir,
                req.model.trim(),
                &hooks_and_urls::resolve_start_session_claude_binary(&self.config),
                Some(initial_prompt.trim()).filter(|p| !p.is_empty()),
                Some(req.permission_mode.trim()).filter(|m| !m.is_empty()),
                req.dangerously_skip_permissions,
                false,
                None,
                extra_args,
                remote.env_pairs(),
                Some(os_user),
            )
            .await
            .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {e}")))?;

        let now = chrono::Utc::now().to_rfc3339();
        let meta = tddy_core::SessionMetadata {
            session_id: session_id.to_string(),
            project_id: req.project_id.trim().to_string(),
            created_at: now.clone(),
            updated_at: now,
            status: "active".to_string(),
            // No repository on this host — the pairing below is how the worktree is found.
            repo_path: None,
            pid: Some(handle.pid),
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some(req.model.trim().to_string()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: Some(hook_token),
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: Some(codebase_instance_id.to_string()),
            codebase_session_id: Some(codebase_session_id.to_string()),
            agent_daemon_instance_id: None,
            agent_session_id: None,
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;

        log::info!(
            target: "tddy_daemon::connection_service",
            "started split claude-cli session {session_id} pid={} codebase_daemon={codebase_instance_id} codebase_session={codebase_session_id}",
            handle.pid
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
            branch_conflict: None,
        }))
    }

    /// How long to wait for the codebase daemon's answer to a split session's forwarded start.
    ///
    /// Not the ordinary [`PEER_FORWARD_TIMEOUT`]: the peer serves this call by resolving the project
    /// — cloning it if it does not have it yet — and cutting a worktree, work it bounds by its own
    /// `spawn_worker_request_timeout` (5 minutes by default). Giving up after 30 s would mean
    /// erroring while the peer is still building, which is the state that used to strand a worktree.
    /// This daemon can only assume the peer's budget matches its own, so it waits that budget out
    /// plus one ordinary forward deadline of round-trip headroom. A peer configured with a *larger*
    /// budget still times out here — the teardown at the call site is what keeps that from becoming
    /// an orphan.
    ///
    /// The cost is that a peer whose RPC participant is gone surfaces after this wait rather than
    /// after 30 s. Accepted: the placement check already required the peer to be visible in the
    /// common room moments earlier, so that is the rarer failure, and the alternative trades a rare
    /// slow error for a routine orphaned worktree.
    pub fn split_forward_deadline(&self) -> Duration {
        self.config.spawn_worker_request_timeout()
            + crate::livekit_peer_discovery::PEER_FORWARD_TIMEOUT
    }

    /// Delete the `workspace` session holding a split session's worktree on `codebase_instance_id`.
    ///
    /// Used only to unwind a failed start, where the caller already has an error to return: the
    /// failure that got us here is the more useful one, so a teardown failure is logged with the
    /// orphaned session named rather than replacing it.
    ///
    /// `unwinding` is what the start failed with, which is the only thing that decides how much the
    /// peer's answer proves — see the `peer_has_no_such_session` arm below.
    pub(crate) async fn tear_down_codebase_session(
        &self,
        slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
        unwinding: SplitStartFailure,
    ) {
        let request = DeleteSessionRequest {
            session_token: session_token.to_string(),
            session_id: codebase_session_id.to_string(),
        };
        match crate::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            codebase_instance_id,
            &request,
        )
        .await
        {
            Ok(_) => log::info!(
                "StartSession: tore down workspace session {codebase_session_id} on daemon {codebase_instance_id} after a failed split start"
            ),
            // A start that failed before the peer created anything is the ordinary case here — the
            // teardown is issued blind, because a forward that never answered leaves this side
            // unable to tell what the peer got as far as building.
            //
            // "Not there" is a statement about *now*, not about the whole start. After a forward
            // deadline the peer may still be cutting the worktree, so it can answer this honestly
            // and create the session moments later — the one case that still orphans a checkout,
            // and the reason that case is a warning an operator can grep for rather than an info
            // line saying nothing was created.
            Err(e) if peer_has_no_such_session(&e) => match unwinding {
                SplitStartFailure::PeerAnswered => log::info!(
                    "StartSession: daemon {codebase_instance_id} did not have workspace session {codebase_session_id} at teardown time, after a failed split start"
                ),
                SplitStartFailure::ForwardDeadline => log::warn!(
                    "StartSession: daemon {codebase_instance_id} did not have workspace session {codebase_session_id} at teardown time, but the forwarded start had already timed out: if that daemon was still building the worktree it may create the session after this teardown, leaving an orphaned checkout there"
                ),
            },
            Err(e) => log::error!(
                "StartSession: could not delete workspace session {codebase_session_id} on daemon {codebase_instance_id} after a failed split start ({e}); its worktree is now orphaned there"
            ),
        }
    }

    /// Delete the `workspace` session paired with a split session, on the daemon that holds its
    /// worktree. A no-op for a co-located session, which records no pairing.
    ///
    /// Unlike the failed-start teardown, a failure here is returned: `DeleteSession` succeeding
    /// while the worktree survives on another host is exactly the silent leak this pairing exists to
    /// prevent, so the message names the session left behind and where.
    pub(crate) async fn delete_paired_codebase_session(
        &self,
        sessions_base: &Path,
        session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        let Ok(meta) = read_session_metadata(&session_dir) else {
            return Ok(());
        };
        let Some((codebase_daemon, codebase_session)) = crate::split_session::split_pairing(&meta)
        else {
            return Ok(());
        };

        let slot = self.common_room_slot("DeleteSession")?;
        // `common_room_slot` only proves this daemon is *configured* for a common room, not that it
        // is currently joined to one — the discovery loop empties this slot on every disconnect.
        // The distinction is load-bearing here: a forward attempted with no room fails locally with
        // `failed_precondition`, which is the same code the peer returns for "I do not have that
        // session". Without this check the two are indistinguishable, and a momentary disconnect
        // would be read as "already torn down", completing the local delete and stranding the
        // worktree on the codebase host — the exact leak the paired teardown exists to prevent.
        if slot.read().await.is_none() {
            return Err(Status::failed_precondition(format!(
                "cannot reach the common room to delete the paired workspace session \
                 {codebase_session} on daemon {codebase_daemon}, so its worktree's fate is unknown; \
                 this session was left in place — retry once the daemons can see each other, or \
                 delete that session on {codebase_daemon} directly and retry"
            )));
        }
        match crate::livekit_peer_discovery::forward_delete_session_via_livekit(
            slot,
            codebase_daemon,
            &DeleteSessionRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session.to_string(),
            },
        )
        .await
        {
            Ok(_) => log::info!(
                "DeleteSession: deleted paired workspace session {codebase_session} on daemon {codebase_daemon}"
            ),
            // The peer answering "I do not have that session" is the state this call exists to
            // reach, not a failure to reach it: an operator may have deleted it there directly, or
            // an earlier attempt may have succeeded on the peer and then failed locally. Continuing
            // is idempotency — the worktree is provably gone with the session that owned it. It is
            // deliberately *not* the treatment for any other outcome: an unreachable or failing peer
            // leaves the worktree's fate unknown, and unknown is refused below.
            Err(e) if peer_has_no_such_session(&e) => log::info!(
                "DeleteSession: daemon {codebase_daemon} no longer has the paired workspace session {codebase_session} ({e}); it was already torn down, so this session's deletion continues"
            ),
            Err(e) => {
                return Err(Status::internal(format!(
                    "could not delete the workspace session {codebase_session} holding this session's worktree on daemon {codebase_daemon} ({e}); its worktree would be orphaned, so the deletion was refused"
                )))
            }
        }
        Ok(())
    }

}
