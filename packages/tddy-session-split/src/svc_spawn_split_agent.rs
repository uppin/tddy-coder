use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use tddy_session_files::attachment_progress::AttachmentProgressSink;

use tddy_service::proto::session::StartSessionRequest;

use std::path::Path;

use crate::attached_initial_prompt::attached_initial_prompt;
use crate::split_ports::SplitSessions;
use tddy_daemon_kernel::daemon_hook_urls;

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
    /// The session's directory, where the hook wait its launch wrote is recorded.
    session_dir: &'a std::path::Path,
}

/// Who a split start is for and which two halves it pairs: the identity half of
/// `spawn_split_agent`'s inputs, so the method takes one value for them.
pub(crate) struct SplitAgentStart<'a> {
    pub(crate) os_user: &'a str,
    pub(crate) session_id: &'a str,
    pub(crate) sessions_base: &'a Path,
    pub(crate) codebase_instance_id: &'a str,
    pub(crate) codebase_session_id: &'a str,
}

impl SplitSessions {
    /// Spawn the agent half of a session whose checkout it does not hold, and record the pairing.
    ///
    /// Serves both placements that separate the agent from its worktree, because they differ in
    /// exactly one thing — how the agent reaches the checkout:
    ///
    /// - `Some(room)` is a **split** session. The checkout is on `codebase_instance_id`, reached
    ///   over the session room this daemon hosts and measures the remote worktree through.
    /// - `None` is a **sandboxed codebase** session. The checkout is a jailed `workspace` session
    ///   on *this* daemon, reached over this daemon's own HTTP URL — so there is no room to open,
    ///   nothing remote to measure, and no join to issue.
    ///
    /// The argv is deliberately identical either way: both agents run outside the checkout with
    /// every native filesystem and shell tool withdrawn, and the jailed-codebase placement's whole
    /// confinement claim rests on that list (`split_session::withdrawal_contract_tests`).
    pub(crate) async fn spawn_split_agent(
        &self,
        start: &SplitAgentStart<'_>,
        room: Option<&crate::split_session::SplitAgentRoom>,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let SplitAgentStart {
            os_user,
            session_id,
            codebase_instance_id,
            codebase_session_id,
            ..
        } = *start;
        let (session_dir, initial_prompt, tddy_tools_path, remote) = self
            .prepare_split_launch(start, room, req, progress)
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
        let (context_dir, extra_args) = self
            .split_agent_launch_args(
                req,
                codebase_instance_id,
                codebase_session_id,
                &session_dir,
                &tddy_tools_path,
                &remote,
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
                session_dir: &session_dir,
            })
            .await?;

        self.ensure_split_agent_alive(session_id, &handle).await?;

        write_split_agent_metadata(
            session_id,
            codebase_instance_id,
            codebase_session_id,
            req,
            session_dir,
            hook_token,
            &handle,
        )?;

        split_start_response(
            room,
            session_id,
            codebase_instance_id,
            codebase_session_id,
            handle,
        )
    }

    async fn prepare_split_launch(
        &self,
        start: &SplitAgentStart<'_>,
        room: Option<&crate::split_session::SplitAgentRoom>,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<
        (
            std::path::PathBuf,
            String,
            std::path::PathBuf,
            tddy_core::RemoteToolEnv,
        ),
        Status,
    > {
        let SplitAgentStart {
            os_user,
            session_id,
            sessions_base,
            codebase_instance_id,
            codebase_session_id,
        } = *start;
        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
        // Where the codebase lives is not a reason for a planned PR's child to come up without its
        // boundaries: the same rule the co-located branches apply, on the attachments this host
        // materialized into the session it is about to run the agent for.
        let initial_prompt = attached_initial_prompt(
            &self.attachment_state(),
            req,
            os_user,
            sessions_base,
            session_id,
            progress,
        )
        .await?;
        let (tddy_tools_path, remote) = self
            .split_agent_tool_wiring(
                os_user,
                session_id,
                codebase_instance_id,
                codebase_session_id,
                room,
                req,
            )
            .await?;
        self.host_split_room(
            session_id,
            codebase_instance_id,
            codebase_session_id,
            room,
            req,
            &session_dir,
        )
        .await?;
        // On record before the agent exists: who may answer this session's prompts is read from
        // it, and a link written after the spawn is a window in which the session answers to no
        // one. The codebase branch rides in the same changeset — read back from the codebase
        // host now its half exists — and a branch that cannot be read fails the start here: the
        // caller's Err path tears the codebase half down.
        self.record_first_changeset(&session_dir, codebase_instance_id, codebase_session_id, req)
            .await?;
        Ok((session_dir, initial_prompt, tddy_tools_path, remote))
    }

    async fn split_agent_launch_args(
        &self,
        req: &StartSessionRequest,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        session_dir: &std::path::Path,
        tddy_tools_path: &std::path::Path,
        remote: &tddy_core::RemoteToolEnv,
    ) -> Result<(std::path::PathBuf, Vec<String>), Status> {
        let roster = self
            .split_agent_roster(codebase_instance_id, codebase_session_id, req)
            .await?;
        let withdrawals = crate::split_session::wire_roster_withdrawals(&roster.agents);
        let roster_seed = crate::split_session::roster_seed_env_pairs(&roster);
        // What the agent's `tddy-tools --mcp` reaches the codebase session with: the transport and
        // the roster seed, registered on that server alone.
        let tool_server_env: Vec<(String, String)> = remote
            .env_pairs()
            .into_iter()
            .chain(roster_seed.iter().cloned())
            .collect();
        // The agent's working directory is populated **before** the agent process exists (AC23): a
        // split agent that starts, reads its cwd and finds only the notice has already missed the
        // project's rules for its first turn. A fetch that fails fails the start, and the caller
        // tears down the worktree this start created on the codebase daemon (AC24).
        let (context_dir, extra_args) = self
            .split_agent_context_and_args(
                codebase_instance_id,
                codebase_session_id,
                req,
                session_dir,
                tddy_tools_path,
                &tool_server_env,
                withdrawals,
            )
            .await?;
        Ok((context_dir, extra_args))
    }

    async fn ensure_split_agent_alive(
        &self,
        session_id: &str,
        handle: &Arc<tddy_cli_sessions::cli_session_manager::PtyHandle>,
    ) -> Result<(), Status> {
        let recording = tddy_cli_sessions::cli_session_manager::main_capture_recording_path(
            &self.tddy_data_dir,
            session_id,
        );
        self.cli_manager
            .record_main_capture(session_id, recording.clone())
            .await;
        startup::ensure_split_agent_started(
            handle,
            startup::SplitAgentStartup::from_config(&self.config),
        )
        .await
        .map_err(|died| Status::failed_precondition(died.to_string()))?;
        // Recorded again now the agent has survived startup: the snapshot at registration can
        // predate its first output, and this one holds it.
        self.cli_manager
            .record_main_capture(session_id, recording)
            .await;
        Ok(())
    }

    async fn split_agent_tool_wiring(
        &self,
        os_user: &str,
        session_id: &str,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        room: Option<&crate::split_session::SplitAgentRoom>,
        req: &StartSessionRequest,
    ) -> Result<(std::path::PathBuf, tddy_core::RemoteToolEnv), Status> {
        let tddy_tools_path = self.resolve_tddy_tools_path()?;
        let remote = match room {
            Some(room) => crate::split_session::split_remote_tool_env(
                room,
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
                &daemon_hook_urls::local_daemon_hook_url(&self.config),
                codebase_session_id,
                &self.agent_session_token_for(&req.session_token, session_id)?,
                Some(self.agent_tool_socket_of(os_user).await?),
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

    /// Host this split session's room over its checkout on `codebase_instance_id`.
    ///
    /// This daemon runs the agent, so it is this session's facilitating daemon and hosts its room —
    /// even though the checkout is elsewhere. Opened before the agent is spawned (PRD FR2), and
    /// measured by asking the codebase daemon rather than by reading a filesystem this host does not
    /// have (FR5). The agent's join was issued for exactly this room.
    ///
    /// The poller signs its own credential per poll under the verified caller's identity rather
    /// than re-presenting `req.session_token`, which the codebase daemon stops accepting five
    /// minutes in — see `RoomPollTokenMinter`.
    ///
    /// A jailed-codebase session (`room` is `None`) opens no room: the worktree it would measure is
    /// on this filesystem, held by a `workspace` session that reports on itself.
    async fn host_split_room(
        &self,
        session_id: &str,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        room: Option<&crate::split_session::SplitAgentRoom>,
        req: &StartSessionRequest,
        session_dir: &std::path::Path,
    ) -> Result<(), Status> {
        if room.is_none() {
            return Ok(());
        }
        let token_minter = Arc::new(crate::split_session::RoomPollTokenMinter::new(
            self.session_tokens()?,
            &req.session_token,
        )?);
        self.split_rooms
            .host_remote_worktree(
                session_id,
                session_dir,
                codebase_session_id,
                codebase_instance_id,
                self.remote_worktree_snapshots(),
                token_minter,
                Arc::clone(&self.reachable_sessions),
            )
            .await
    }

    async fn split_agent_roster(
        &self,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        req: &StartSessionRequest,
    ) -> Result<tddy_service::proto::session_agents_svc::SessionAgentRoster, Status> {
        if req.specialized_agents.is_empty() {
            return Ok(Default::default());
        }
        self.split_roster_from_codebase_host(
            &req.session_token,
            codebase_session_id,
            codebase_instance_id,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn split_agent_context_and_args(
        &self,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        req: &StartSessionRequest,
        session_dir: &std::path::Path,
        tddy_tools_path: &std::path::Path,
        tool_server_env: &[(String, String)],
        withdrawals: Vec<(String, Vec<String>)>,
    ) -> Result<(std::path::PathBuf, Vec<String>), Status> {
        let agent_spec = crate::agent_argv::split_agent_spec(&self.coding_agents)?;
        let agent = agent_spec.id.as_str();
        let glob_refs: Vec<&str> = agent_spec
            .agent
            .context_globs
            .iter()
            .map(String::as_str)
            .collect();
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
            &agent_spec,
            session_dir,
            &withdrawals,
            &glob_refs,
            &context,
        )?;
        let extra_args = crate::split_session::split_claude_extra_args(
            &agent_spec,
            session_dir,
            &tddy_tools_path.to_string_lossy(),
            tool_server_env,
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
    ///
    /// Public for the placements outside this crate that hand an agent its own credential (a
    /// project-home main, launched by `tddy-agent-launch`).
    /// `session_id` is the session the agent runs for, which the token names.
    pub fn agent_session_token_for(
        &self,
        caller_token: &str,
        session_id: &str,
    ) -> Result<String, Status> {
        crate::split_session::mint_agent_session_token(
            self.session_tokens()?,
            caller_token,
            session_id,
        )
    }

    /// Who bears `token`: a session's agent when it verifies under this daemon's keys and was
    /// minted for one ([`tddy_github::TokenPrincipal::Agent`]), with the session it names; a
    /// person otherwise. A token these keys cannot verify, and every token on a host that signs
    /// none, is not an agent's.
    pub fn bearer_of(&self, token: &str) -> crate::split_ports::TokenBearer {
        let claims = self
            .session_tokens
            .as_ref()
            .and_then(|tokens| tokens.verifier().verify_now(token).ok());
        match claims {
            Some(claims) if claims.principal == Some(tddy_github::TokenPrincipal::Agent) => {
                crate::split_ports::TokenBearer::Agent {
                    session_id: claims.session_id,
                }
            }
            _ => crate::split_ports::TokenBearer::Person,
        }
    }
}

fn split_start_response(
    room: Option<&crate::split_session::SplitAgentRoom>,
    session_id: &str,
    codebase_instance_id: &str,
    codebase_session_id: &str,
    handle: Arc<tddy_cli_sessions::cli_session_manager::PtyHandle>,
) -> Result<Response<StartSessionResponse>, Status> {
    let placement = match room {
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

mod launch;
mod split_agent_pty;
#[cfg(test)]
mod split_agent_pty_tests;
mod startup;

pub use crate::svc_paired_codebase_teardown;
use crate::svc_split_agent_metadata::write_split_agent_metadata;
