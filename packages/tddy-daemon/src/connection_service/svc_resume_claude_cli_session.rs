// `list_session_agents` is a `ConnectionService` trait method, so the trait has to be in scope.
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
use tddy_service::proto::connection::ListSessionAgentsRequest;

use tddy_rpc::Request;

use std::path::Path;

use std::sync::Arc;

use tddy_rpc::Status;

use tddy_service::proto::connection::ResumeSessionResponse;

use tddy_rpc::Response;

use std::path::PathBuf;

use crate::connection_service::hooks_and_urls;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Handle `ResumeSession` for `session_type = "claude-cli"` sessions.
    pub(crate) async fn resume_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        session_dir: PathBuf,
        meta: tddy_core::SessionMetadata,
        // The caller's token, re-exported to a split session's agent as TDDY_REMOTE_SESSION_TOKEN:
        // the codebase daemon verifies it on every tool call, so the resumed agent needs a live one.
        session_token: &str,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if meta.sandbox == Some(true) {
            return self
                .resume_sandboxed_claude_cli_session(os_user, session_id, session_dir, meta)
                .await;
        }
        let model = meta.model.clone().unwrap_or_default();

        // A split session has no `repo_path` here and its `TDDY_REMOTE_*` wiring was injected at
        // spawn time, so both are re-derived from the persisted pairing — including a **fresh** join
        // token, since the original is scoped to a lifetime that may well have elapsed while the
        // session was stopped.
        let split = self
            .resume_split_wiring(&meta, &session_dir, session_id, session_token)
            .await?;
        let worktree_path = split
            .as_ref()
            .map(|w| w.context_dir.clone())
            .or_else(|| meta.repo_path.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| session_dir.clone());
        let (split_args, split_env) = match split {
            Some(w) => (w.extra_args, w.env),
            None => (Vec::new(), Vec::new()),
        };

        let manager = Arc::clone(&self.claude_cli_manager);
        let session_id_owned = session_id.to_string();
        let binary_owned = hooks_and_urls::resolve_resume_session_claude_binary(&self.config);

        // Re-wire managed-workflow orchestration when resuming a managed session — metadata records a
        // recipe only for managed sessions. The controller resumes at the goal persisted in
        // changeset.yaml so the workflow continues from where it left off, not from the start goal.
        let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        let mut env_extra: Vec<(String, String)> = split_env;
        if let Some(recipe_name) = meta.recipe.as_deref().filter(|s| !s.trim().is_empty()) {
            let recipe = tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(recipe_name)
                .map_err(Status::invalid_argument)?;
            let resume_goal = Self::managed_resume_goal(&session_dir, &recipe);
            let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
                self.config
                    .claude_cli
                    .as_ref()
                    .and_then(|c| c.tddy_tools_path.as_deref()),
            );
            let launch = self.prepare_managed_workflow(
                &session_id_owned,
                recipe,
                &session_dir,
                &worktree_path,
                &session_dir,
                &tddy_tools_path,
                Some(resume_goal),
                None,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            env_extra.extend(launch.env);
            managed = Some(launch.workflow);
        }

        let handle = manager
            .resume_with_options(
                &session_id_owned,
                worktree_path,
                &model,
                &binary_owned,
                append_system_prompt_file.as_deref(),
                split_args,
                env_extra,
            )
            .await
            .map_err(|e| Status::internal(format!("failed to relaunch claude-cli: {}", e)))?;

        if let Some(mw) = managed {
            manager.attach_managed_workflow(&session_id_owned, mw).await;
        }

        let pid = handle.pid;

        // Update .session.yaml with new pid and active status.
        let now = chrono::Utc::now().to_rfc3339();
        let updated = tddy_core::SessionMetadata {
            updated_at: now,
            status: "active".to_string(),
            pid: Some(pid),
            ..meta
        };
        tddy_core::write_session_metadata(&session_dir, &updated)
            .map_err(|e| Status::internal(format!("failed to update session metadata: {}", e)))?;

        log::info!(
            target: "tddy_daemon::connection_service",
            "resumed claude-cli session {} pid={}",
            session_id, pid
        );

        Ok(Response::new(ResumeSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
        }))
    }

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
        session_dir: &Path,
        session_id: &str,
        session_token: &str,
    ) -> Result<Option<crate::split_session::SplitAgentWiring>, Status> {
        let Some((codebase_daemon, codebase_session)) = crate::split_session::split_pairing(meta)
        else {
            return Ok(None);
        };

        let withdrawals = self
            .split_withdrawals_from_codebase_host(session_token, codebase_session, codebase_daemon)
            .await?;

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
            session_dir,
            &self.resolve_tddy_tools_path().to_string_lossy(),
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
        log::info!(
            "ResumeSession: re-wired split session {session_id} to workspace session {codebase_session} on daemon {codebase_daemon}"
        );
        Ok(Some(wiring))
    }

    /// The tool withdrawals a resumed split agent must launch with, read from the daemon that holds
    /// the session's roster.
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
    pub(crate) async fn split_withdrawals_from_codebase_host(
        &self,
        session_token: &str,
        codebase_session: &str,
        codebase_daemon: &str,
    ) -> Result<Vec<(String, Vec<String>)>, Status> {
        let roster = self
            .list_session_agents(Request::new(ListSessionAgentsRequest {
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
        Ok(crate::split_session::wire_roster_withdrawals(
            &roster.agents,
        ))
    }
}
