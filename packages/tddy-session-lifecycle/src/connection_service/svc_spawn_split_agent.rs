use std::time::Duration;

use uuid::Uuid;

use crate::{
    connection_service::hooks_and_urls, livekit_peer_discovery::local_instance_id_for_config,
};

use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use super::AttachmentProgressSink;

use tddy_service::proto::session::StartSessionRequest;

use std::path::Path;

use super::DaemonSessionHost;

/// What the split agent's claude-cli process is spawned with: its context dir, its tools' route
/// back, and the request's model and prompt.
struct SplitAgentProcess<'a> {
    os_user: &'a str,
    session_id: &'a str,
    req: &'a StartSessionRequest,
    initial_prompt: String,
    tddy_tools_path: std::path::PathBuf,
    remote: tddy_core::RemoteToolEnv,
    context_dir: std::path::PathBuf,
    extra_args: Vec<String>,
}

impl DaemonSessionHost {
    /// Spawn the agent half of a session whose checkout it does not hold, and record the pairing.
    ///
    /// Serves both placements that separate the agent from its worktree, because they differ in
    /// exactly one thing — how the agent reaches the checkout:
    ///
    /// - `Some(room)` is a **split** session. The checkout is on `codebase_instance_id`, reached
    ///   over the LiveKit room this daemon hosts and measures the remote worktree through.
    /// - `None` is a **sandboxed codebase** session. The checkout is a jailed `workspace` session
    ///   on *this* daemon, reached over this daemon's own HTTP URL — so there is no room to open,
    ///   nothing remote to measure, and no join token to mint.
    ///
    /// The argv is deliberately identical either way: both agents run outside the checkout with
    /// every native filesystem and shell tool withdrawn, and the jailed-codebase placement's whole
    /// confinement claim rests on that list (`split_session::withdrawal_contract_tests`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_split_agent(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: &Path,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        livekit: Option<&crate::split_session::SplitLiveKitRoom>,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
        // Where the codebase lives is not a reason for a planned PR's child to come up without its
        // boundaries: the same rule the co-located branches apply, on the attachments this host
        // materialized into the session it is about to run the agent for.
        let initial_prompt = self
            .attached_initial_prompt(req, os_user, sessions_base, session_id, progress)
            .await?;

        let (tddy_tools_path, remote) = self.split_agent_tool_wiring(
            session_id,
            codebase_instance_id,
            codebase_session_id,
            livekit,
            req,
        )?;
        self.join_split_livekit_room(
            session_id,
            codebase_instance_id,
            codebase_session_id,
            livekit,
            req,
            &session_dir,
        )
        .await?;

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
        let withdrawals = self
            .split_agent_withdrawals(codebase_instance_id, codebase_session_id, req)
            .await?;
        // The agent's working directory is populated **before** the agent process exists (AC23): a
        // split agent that starts, reads its cwd and finds only the notice has already missed the
        // project's rules for its first turn. A fetch that fails fails the start, and the caller
        // tears down the worktree this start created on the codebase daemon (AC24).
        let (context_dir, extra_args) = self
            .split_agent_context_and_args(
                codebase_instance_id,
                codebase_session_id,
                req,
                &session_dir,
                &tddy_tools_path,
                withdrawals,
            )
            .await?;

        // Claude Code reads `.claude/settings.local.json` from its working directory, which for a
        // split session is the context dir rather than a worktree. Best-effort, as elsewhere: a
        // missing hook file costs status reporting, not the session.
        let (hook_token, handle) = self
            .spawn_split_agent_process(SplitAgentProcess {
                os_user,
                session_id,
                req,
                initial_prompt,
                tddy_tools_path,
                remote,
                context_dir,
                extra_args,
            })
            .await?;

        write_split_agent_metadata(
            session_id,
            codebase_instance_id,
            codebase_session_id,
            req,
            session_dir,
            hook_token,
            &handle,
        )?;

        let placement = match livekit {
            Some(_) => "split",
            None => "jailed-codebase",
        };
        log::info!(
            target: "tddy_daemon::connection_service",
            "started {placement} claude-cli session {session_id} pid={} codebase_daemon={codebase_instance_id} codebase_session={codebase_session_id}",
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

    fn split_agent_tool_wiring(
        &self,
        session_id: &str,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        livekit: Option<&crate::split_session::SplitLiveKitRoom>,
        req: &StartSessionRequest,
    ) -> Result<(std::path::PathBuf, tddy_core::RemoteToolEnv), Status> {
        let tddy_tools_path = self.resolve_tddy_tools_path()?;
        let remote = match livekit {
            Some(livekit) => crate::split_session::split_remote_tool_env(
                livekit,
                self.session_tokens()?,
                &crate::split_session::SplitSpawnTarget {
                    session_id,
                    codebase_instance_id,
                    codebase_session_id,
                    session_token: &req.session_token,
                },
            )?,
            // The checkout is a jailed `workspace` session on this daemon, so this daemon's own
            // URL is the route to it and no LiveKit field is set at all — see
            // [`crate::split_session::colocated_jail_tool_env`], which inverts the split builder's
            // reasoning field by field.
            None => crate::split_session::colocated_jail_tool_env(
                &hooks_and_urls::local_daemon_hook_url(&self.config),
                codebase_session_id,
                &self.agent_session_token_for(&req.session_token)?,
                self.agent_tool_socket_for_embedded_host(),
            ),
        };
        // What this agent was actually wired to, on one line, on the path *every* placement takes.
        // None of it used to be logged: a session started and the only record of which binaries it
        // got — or which route its tools would take back — was the process table, after the fact.
        // Both bit us. The tools path was written relative and resolved against the agent's own
        // context dir, so its MCP server never launched; and the relay was an HTTP URL on a host
        // that serves no HTTP, so every tool call returned `relay parse error`.
        log::info!(
            "agent tool wiring: session={} codebase_session={} tddy_tools={} relay={}",
            session_id,
            codebase_session_id,
            tddy_tools_path.display(),
            match (&remote.daemon_socket, remote.daemon_url.is_empty()) {
                (Some(sock), _) => format!("uds {sock}"),
                (None, false) => format!("http {}", remote.daemon_url),
                (None, true) => "livekit".to_string(),
            },
        );
        Ok((tddy_tools_path, remote))
    }

    async fn spawn_split_agent_process(
        &self,
        launch: SplitAgentProcess<'_>,
    ) -> Result<(String, Arc<crate::claude_cli_session::PtyHandle>), Status> {
        let SplitAgentProcess {
            os_user,
            session_id,
            req,
            initial_prompt,
            tddy_tools_path,
            remote,
            context_dir,
            extra_args,
        } = launch;
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
        Ok((hook_token, handle))
    }

    async fn join_split_livekit_room(
        &self,
        session_id: &str,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        livekit: Option<&crate::split_session::SplitLiveKitRoom>,
        req: &StartSessionRequest,
        session_dir: &std::path::Path,
    ) -> Result<(), Status> {
        if livekit.is_some() {
            // This daemon runs the agent, so it is this session's facilitating daemon and hosts its room —
            // even though the checkout is on `codebase_instance_id`. Opened before the agent is spawned
            // (PRD FR2), and measured by asking the codebase daemon rather than by reading a filesystem
            // this host does not have (FR5). The agent's token was minted for exactly this room.
            //
            // A jailed-codebase session opens none: the worktree it would measure is on this
            // filesystem, held by a `workspace` session that reports on itself.
            //
            // The poller signs its own credential per poll under the verified caller's identity rather
            // than re-presenting `req.session_token`, which the codebase daemon stops accepting five
            // minutes in — see `RoomPollTokenMinter`.
            let token_minter = Arc::new(crate::split_session::RoomPollTokenMinter::new(
                self.session_tokens()?,
                &req.session_token,
            )?);
            let remote_source = Arc::new(tddy_daemon_livekit::session_room::RemoteCheckout::new(
                Arc::new(self.clone()),
                codebase_session_id.to_string(),
                codebase_instance_id.to_string(),
                token_minter,
                session_dir.to_path_buf(),
            ));
            let local_instance_id = local_instance_id_for_config(&self.config);
            match self
                .session_rooms
                .open_measured_by(
                    &tddy_daemon_livekit::session_room::DaemonRoomHosting {
                        config: &self.config,
                        instance_id: &local_instance_id,
                        rooms: &self.session_rooms,
                    }
                    .for_remote_worktree(session_id, session_dir),
                    || Arc::new(self.clone()).session_room_roster(),
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
        }
        Ok(())
    }

    async fn split_agent_withdrawals(
        &self,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        req: &StartSessionRequest,
    ) -> Result<Vec<(String, Vec<String>)>, Status> {
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
        Ok(withdrawals)
    }

    async fn split_agent_context_and_args(
        &self,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        req: &StartSessionRequest,
        session_dir: &std::path::Path,
        tddy_tools_path: &std::path::Path,
        withdrawals: Vec<(String, Vec<String>)>,
    ) -> Result<(std::path::PathBuf, Vec<String>), Status> {
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
            session_dir,
            &withdrawals,
            tddy_core::backend::context_globs_for_agent(agent),
            &context,
        )?;
        let extra_args = crate::split_session::split_claude_extra_args(
            session_dir,
            &tddy_tools_path.to_string_lossy(),
            &withdrawals,
        )?;
        Ok((context_dir, extra_args))
    }

    /// The credential an agent presents on every tool call it makes back to a daemon.
    ///
    /// Minted for the agent under the caller's **verified** identity, for the reason
    /// [`crate::split_session::split_remote_tool_env`] gives: the caller's own access token is
    /// proof of who asked and expires minutes into a session that runs for hours, so forwarding it
    /// would tie the agent's whole toolchain to it.
    ///
    /// A session host built without [`Self::with_session_tokens`] signs nothing, and it is
    /// **refused here** rather than falling back to forwarding the caller's credential. Forwarding
    /// it would hand the agent a string that expires minutes into a session that runs for hours.
    ///
    /// Unreachable in the daemon, which builds its session host with the same tokens its auth
    /// entries were built from (`tddy-daemon/src/runtime.rs`), and deliberately a refusal rather
    /// than an `unreachable!`: the refusal is what keeps another way of assembling a session host
    /// from quietly reintroducing the fallback.
    pub(crate) fn agent_session_token_for(&self, caller_token: &str) -> Result<String, Status> {
        crate::split_session::mint_agent_session_token(self.session_tokens()?, caller_token)
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
}

fn write_split_agent_metadata(
    session_id: &str,
    codebase_instance_id: &str,
    codebase_session_id: &str,
    req: &StartSessionRequest,
    session_dir: std::path::PathBuf,
    hook_token: String,
    handle: &Arc<crate::claude_cli_session::PtyHandle>,
) -> Result<(), Status> {
    let meta = tddy_core::SessionMetadata {
        // No repository on this host — the pairing below is how the worktree is found.
        repo_path: None,
        pid: Some(handle.pid),
        model: Some(req.model.trim().to_string()),
        hook_token: Some(hook_token),
        codebase_daemon_instance_id: Some(codebase_instance_id.to_string()),
        codebase_session_id: Some(codebase_session_id.to_string()),
        ..crate::connection_service::starting_session_metadata(
            session_id,
            req.project_id.trim(),
            "claude-cli",
        )
    };
    tddy_core::write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
    Ok(())
}

mod svc_paired_codebase_teardown;
