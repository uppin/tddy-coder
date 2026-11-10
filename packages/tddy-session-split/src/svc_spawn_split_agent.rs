use uuid::Uuid;

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
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_split_agent(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: &Path,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        room: Option<&crate::split_session::SplitAgentRoom>,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
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
                &session_dir,
                &tddy_tools_path,
                &tool_server_env,
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
                session_dir: &session_dir,
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
                &self.agent_session_token_for(&req.session_token)?,
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

    async fn spawn_split_agent_process(
        &self,
        launch: SplitAgentProcess<'_>,
    ) -> Result<
        (
            String,
            Arc<tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle>,
        ),
        Status,
    > {
        let SplitAgentProcess {
            os_user,
            session_id,
            req,
            initial_prompt,
            tddy_tools_path,
            remote,
            context_dir,
            extra_args,
            session_dir,
        } = launch;
        let hook_token = Uuid::new_v4().to_string();
        // The agent's hook is told to wait this long, so the daemon holds it for as long: recorded
        // beside `.session.yaml`, where `AwaitQuestionAnswer` reads it. A split agent has no tool
        // file of its own to read the wait from, so it is the specs' default.
        let answer_wait_secs = record_split_answer_wait(session_dir)?;
        // The hooks file is the one the split agent's spec's hooks slot renders, written
        // under the context dir the agent runs in.
        let agent_tool_socket = self.agent_tool_socket_of(os_user).await?;
        let hook_params = tddy_core::HookCommandParams {
            tddy_tools_path: &tddy_tools_path.to_string_lossy(),
            daemon_url: &tddy_daemon_kernel::daemon_hook_urls::advertise_daemon_url(&self.config),
            session_id,
            os_user,
            hook_token: &hook_token,
            daemon_socket: Some(&agent_tool_socket),
            answer_wait_secs,
        };
        self.render_and_write_split_hooks(session_id, &context_dir, hook_params)?;
        // The launch argv is rendered from the split agent's spec — the same template a
        // spec-driven launch renders — with this start's model, session, permission mode and
        // prompt bound.
        let (spec, argv) = self.render_split_argv(session_id, req, &initial_prompt)?;
        let session_type = format!("{}-cli", spec.id);
        // The agent's `tddy-tools` finds its session through the pointer in the context dir (and
        // through its own registration, `extra_args`): the agent process is given no `TDDY_*`
        // variable. The pointer is removed when the agent exits.
        let pointer =
            crate::split_session::split_session_pointer(&remote, &context_dir, session_dir);
        let on_exit = self.session_pointers.place(&pointer, &context_dir, None)?;
        let argv = split_argv_with_flags(initial_prompt, extra_args, argv);
        let pointer_dir = context_dir.clone();
        let handle = spawn_split_agent_pty(
            &self.cli_manager,
            &spec,
            self.coding_agents.agent_env_base(),
            SplitAgentPty {
                session_id,
                session_type: &session_type,
                context_dir,
                model: req.model.trim(),
                argv,
                os_user: Some(os_user),
                on_exit,
            },
        )
        .await
        .map_err(|e| {
            // No agent exists whose exit would remove the pointer just placed.
            self.session_pointers.remove(&pointer_dir);
            Status::internal(format!("failed to spawn {session_type}: {e}"))
        })?;
        Ok((hook_token, handle))
    }

    fn render_and_write_split_hooks(
        &self,
        session_id: &str,
        context_dir: &std::path::Path,
        hook_params: tddy_core::HookCommandParams<'_>,
    ) -> Result<(), Status> {
        match tddy_coding_agent_specs::slots::render(
            &crate::agent_argv::split_agent_spec(&self.coding_agents)?,
            &tddy_coding_agent_specs::slots::SlotInputs {
                workdir: context_dir.to_owned(),
                scratch_dir: context_dir.to_owned(),
                mcp_server: None,
                allowed_tools: Vec::new(),
                denied_tools: Vec::new(),
                permission_prompt_tool: None,
                preamble: None,
                append_system_prompt_file: None,
                hooks: Some(hook_params),
                home: None,
            },
        ) {
            Ok(rendered) => {
                for (path, bytes) in &rendered.files {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(path, bytes) {
                        log::warn!(
                            "session {session_id}: failed to write {} — hooks will not fire: {e}",
                            path.display()
                        );
                    }
                }
            }
            Err(e) => {
                log::warn!("session {session_id}: failed to render its hooks file: {e}");
            }
        }
        Ok(())
    }

    fn render_split_argv(
        &self,
        session_id: &str,
        req: &StartSessionRequest,
        initial_prompt: &str,
    ) -> Result<(tddy_coding_agent_specs::CodingAgentSpec, Vec<String>), Status> {
        let spec = crate::agent_argv::split_agent_spec(&self.coding_agents)?;
        let argv = spec
            .render_run_argv(
                std::path::Path::new(&(self.agent_binary)(&spec.id)?),
                &[
                    ("session_id", session_id),
                    ("chat_id", session_id),
                    ("model", req.model.trim()),
                    ("permission_mode", req.permission_mode.trim()),
                    ("prompt", initial_prompt.trim()),
                ],
                if req.dangerously_skip_permissions {
                    &["dangerously_skip_permissions"][..]
                } else {
                    &[][..]
                },
            )
            .map_err(|e| Status::internal(format!("rendering the split agent's argv: {e}")))?;
        Ok((spec, argv))
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
    pub fn agent_session_token_for(&self, caller_token: &str) -> Result<String, Status> {
        crate::split_session::mint_agent_session_token(self.session_tokens()?, caller_token)
    }
}

fn record_split_answer_wait(session_dir: &Path) -> Result<u64, Status> {
    let answer_wait_secs = tddy_coding_agent_specs::DEFAULT_ANSWER_WAIT_SECS;
    tddy_session_activity::answer_wait::record_answer_wait(
        session_dir,
        std::time::Duration::from_secs(answer_wait_secs),
    )
    .map_err(|e| Status::internal(format!("failed to record the answer wait: {e}")))?;
    Ok(answer_wait_secs)
}

fn split_argv_with_flags(
    initial_prompt: String,
    extra_args: Vec<String>,
    argv: Vec<String>,
) -> Vec<String> {
    let argv = tddy_cli_sessions::cli_session_manager::CliSessionManager::with_launch_flags(
        argv,
        Some(initial_prompt.trim()).filter(|p| !p.is_empty()),
        None,
        &extra_args,
    );
    argv
}

mod split_agent_pty;
#[cfg(test)]
mod split_agent_pty_tests;

pub use crate::svc_paired_codebase_teardown;
use crate::svc_spawn_split_agent::split_agent_pty::spawn_split_agent_pty;
use crate::svc_spawn_split_agent::split_agent_pty::SplitAgentPty;
use crate::svc_split_agent_metadata::write_split_agent_metadata;
