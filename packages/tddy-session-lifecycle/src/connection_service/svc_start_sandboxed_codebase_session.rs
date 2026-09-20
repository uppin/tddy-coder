//! The **sandboxed codebase** start path: this daemon jails its own checkout and runs the agent
//! beside it, unconfined, with every native filesystem and shell tool withdrawn.
//!
//! PRD: `docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md`.
//!
//! The split orchestration with the peer hop removed. Everything
//! [`DaemonSessionHost::start_split_claude_cli_session`] forwards to another daemon is served
//! here instead — the same `workspace_start_request`, the same caller-chosen session id, the same
//! pairing fields — so the jail, the tool routing, the resume and the teardown are consumed
//! rather than rebuilt. Three things it does *not* have: a common room, a LiveKit forward, and a
//! [`crate::split_session::SplitLiveKitRoom`]. A placement whose halves share a host must start on
//! a daemon that has never joined a room.

use tddy_rpc::{Response, Status};
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use uuid::Uuid;

use super::{agent_roster, AttachmentProgressSink, DaemonSessionHost};
use crate::livekit_peer_discovery::local_instance_id_for_config;
use crate::session_deletion;
use crate::user_sessions_path::{projects_path_for_user, sessions_base_for_user};

impl DaemonSessionHost {
    /// Start a session whose **codebase** is jailed on this daemon and whose **agent** is not.
    ///
    /// Two sessions come out of one request: a local `workspace` session carrying
    /// `sandbox: Some(true)`, which holds the worktree and the jail over it, and the `claude-cli`
    /// agent, which has no repository on disk and reaches that worktree only through
    /// `mcp__tddy-tools__*` calls addressed at the workspace session's id.
    ///
    /// Atomic by construction, exactly as the split path is: everything resolvable is resolved
    /// before anything is created, and a failed agent spawn tears the workspace session — and with
    /// it the jail and the worktree — back down. A half-built session strands a jailed checkout
    /// with no agent left to reclaim it, and a `tddy-sandbox-runner` holding it open.
    pub(crate) async fn start_sandboxed_codebase_session(
        &self,
        os_user: &str,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // Resolved before anything is created, for the reason the split path resolves it before
        // the peer is contacted: a reference naming nothing is a request error, and refusing one
        // after the checkout existed would mean tearing a jailed worktree down to report a typo.
        self.resolve_specialized_agent_defs(&req.specialized_agents)
            .await?;

        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let local_instance_id = local_instance_id_for_config(&self.config);
        let session_id = Uuid::now_v7().to_string();
        // Chosen here and carried in the request, as on the split path — and for a reason that
        // survives the peer hop's removal: the id is what the teardown below names, so it has to
        // exist before the thing it names does.
        let checkout_session_id = Uuid::now_v7().to_string();

        // The same workspace half a split forwards to its codebase daemon, served by this one.
        // `sandbox` is set here rather than carried from the agent's request, which cannot have it
        // — `classify_placement` refuses `sandboxed_codebase` alongside `sandbox`, because that
        // flag jails the *agent*, which is the placement this one inverts. `sandboxed_codebase` is
        // cleared for the matching reason: the workspace half is the checkout, not another session
        // whose codebase needs placing.
        let workspace_req = StartSessionRequest {
            sandbox: true,
            sandboxed_codebase: false,
            ..agent_roster::workspace_start_request(
                req,
                &local_instance_id,
                &session_id,
                &checkout_session_id,
            )?
        };
        let workspace =
            Box::pin(self.start_session_core(workspace_req, &AttachmentProgressSink::discarding()))
                .await?
                .into_inner();
        // A branch another session owns is reported, not created: nothing was built, so the
        // conflict travels back to the caller as it would for a co-located start.
        if workspace.branch_conflict.is_some() {
            return Ok(Response::new(workspace));
        }
        if workspace.session_id != checkout_session_id {
            // Torn down under the id that actually exists, not the one that was asked for: the
            // checkout *was* built, and the only handle on its jail and its worktree is the id
            // the start answered with. Returning without this is how a `tddy-sandbox-runner`
            // ends up holding a worktree no session names.
            self.tear_down_local_checkout_session(&sessions_base, os_user, &workspace.session_id)
                .await;
            return Err(Status::internal(format!(
                "the jailed checkout came up as session {:?} instead of the requested {checkout_session_id:?}; it was torn down rather than left holding a worktree this session could not name",
                workspace.session_id
            )));
        }

        let started = self
            .spawn_split_agent(
                os_user,
                &session_id,
                &sessions_base,
                &local_instance_id,
                &checkout_session_id,
                // No room: both halves are on this host, so the agent reaches its checkout over
                // this daemon's own URL and the placement needs no LiveKit at all.
                None,
                req,
                progress,
            )
            .await;

        match started {
            Ok(response) => Ok(response),
            Err(status) => {
                self.tear_down_local_checkout_session(
                    &sessions_base,
                    os_user,
                    &checkout_session_id,
                )
                .await;
                Err(status)
            }
        }
    }

    /// Remove the jailed `workspace` session a failed jailed-codebase start left behind.
    ///
    /// The co-located counterpart of [`Self::tear_down_codebase_session`], and it unwinds the same
    /// two things that call does on a peer: the jail — a live `tddy-sandbox-runner` holding the
    /// checkout open, which nothing but this registry would ever stop — and the session directory
    /// the worktree hangs off.
    ///
    /// The caller already has the failure that got us here, which is the more useful one to
    /// return, so a teardown failure is logged with the orphaned session named rather than
    /// replacing it.
    async fn tear_down_local_checkout_session(
        &self,
        sessions_base: &std::path::Path,
        os_user: &str,
        checkout_session_id: &str,
    ) {
        if let Some(jail) = self.workspace_sandboxes.remove(checkout_session_id).await {
            jail.stop();
        }
        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir));
        match session_deletion::delete_session_directory(
            sessions_base,
            checkout_session_id,
            projects_dir.as_deref(),
        ) {
            Ok(()) => log::info!(
                "StartSession: tore down jailed checkout session {checkout_session_id} after its agent could not be spawned"
            ),
            Err(e) => log::error!(
                "StartSession: could not remove jailed checkout session {checkout_session_id} after its agent could not be spawned ({}); its worktree is now orphaned on this host",
                e.message()
            ),
        }
    }

    /// Re-provision the jail over a jailed-codebase session's checkout, if this daemon is not
    /// already holding one for it.
    ///
    /// Reached from the **agent** half's resume: the operator resumes the session they can see,
    /// and the `workspace` session holding the checkout is not it. The registry is in-process, so
    /// a daemon restart empties it while the metadata that says `sandbox: Some(true)` survives —
    /// which is precisely the state `exec_tool_route` refuses rather than serving from the bare
    /// host, so without this the resumed agent would have no working tool call.
    ///
    /// Idempotent: a jail still registered is left alone rather than rebuilt beside itself.
    pub(crate) async fn reprovision_colocated_checkout_jail(
        &self,
        sessions_base: &std::path::Path,
        checkout_session_id: &str,
    ) -> Result<(), Status> {
        if self
            .workspace_sandboxes
            .get(checkout_session_id)
            .await
            .is_some()
        {
            return Ok(());
        }
        self.provision_workspace_tool_sandbox(sessions_base, checkout_session_id)
            .await
    }

    /// Rebuild a jailed-codebase agent's wiring for a resume, and give it its jail back.
    ///
    /// The co-located counterpart of [`crate::split_session::prepare_split_agent_wiring`], and it
    /// differs from it in exactly the two ways the placement does: the tool env is
    /// [`crate::split_session::colocated_jail_tool_env`] rather than the LiveKit one, and the
    /// checkout's jail is re-provisioned here because this daemon is the host that lost it.
    ///
    /// The roster and the project's guidance are still read *through* the session that holds the
    /// checkout rather than off this session's own directory — they live beside the codebase on
    /// every placement, and here that host happens to be this one. Reading them the same way
    /// keeps one answer to "what was this agent's tool surface when it stopped", which is what
    /// makes the relaunch honour a withdrawal.
    pub(crate) async fn resume_colocated_jail_wiring(
        &self,
        sessions_base: &std::path::Path,
        session_dir: &std::path::Path,
        session_id: &str,
        checkout_session_id: &str,
        session_token: &str,
    ) -> Result<crate::split_session::SplitAgentWiring, Status> {
        self.reprovision_colocated_checkout_jail(sessions_base, checkout_session_id)
            .await?;

        let local_instance_id = local_instance_id_for_config(&self.config);
        let withdrawals = self
            .split_withdrawals_from_codebase_host(
                session_token,
                checkout_session_id,
                &local_instance_id,
            )
            .await?;
        let agent = crate::context_files::context_agent_for_session_type("claude-cli");
        let context = self
            .split_context_from_codebase_host(
                session_token,
                checkout_session_id,
                &local_instance_id,
                agent,
                "resume",
            )
            .await?;

        let remote = crate::split_session::colocated_jail_tool_env(
            &super::hooks_and_urls::local_daemon_hook_url(&self.config),
            checkout_session_id,
            &self.agent_session_token_for(session_token)?,
            self.agent_tool_socket_for_embedded_host(),
        );
        let tddy_tools = self.resolve_tddy_tools_path()?;
        let context_dir = crate::split_session::build_split_context_dir(
            session_dir,
            &withdrawals,
            tddy_core::backend::context_globs_for_agent(agent),
            &context,
        )?;
        // What this placement is actually made of, on one line. Everything here was previously
        // unlogged: a session started, and the only record of *which* binaries it was wired to —
        // or that the agent's cwd is not the checkout — was the process table, after the fact.
        // The tools path is the one that bit: written relative, it resolved against the agent's
        // context dir and the MCP server never started.
        log::info!(
            "sandboxed-codebase wiring: agent_session={session_id} checkout_session={checkout_session_id} \
             toolchain={} tddy_tools={} agent_cwd={} withdrawn_tools={}",
            self.config.toolchain().describe(),
            tddy_tools.display(),
            context_dir.display(),
            withdrawals.len(),
        );
        let wiring = crate::split_session::SplitAgentWiring {
            context_dir,
            extra_args: crate::split_session::split_claude_extra_args(
                session_dir,
                &tddy_tools.to_string_lossy(),
                &withdrawals,
            )?,
            env: remote.env_pairs(),
        };
        log::info!(
            "ResumeSession: re-wired jailed-codebase session {session_id} to its checkout session {checkout_session_id} on this daemon"
        );
        Ok(wiring)
    }
}
