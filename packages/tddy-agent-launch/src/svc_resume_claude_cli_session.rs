use std::path::Path;

use std::sync::Arc;

use tddy_rpc::Status;

use tddy_service::proto::session::ResumeSessionResponse;

use tddy_rpc::Response;

use std::path::PathBuf;

use crate::claude_cli_spawn::record_main_terminal;
use crate::claude_cli_tmux::{
    agent_tmux_session_is_alive, relaunch_claude_cli, ClaudeCliTmuxLaunch,
};
use crate::hooks_and_urls;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;

use crate::launch_ports::LaunchSessions;
use crate::project_home_launch::ResumedFlags;
use crate::session_provider_route::resume_route_env;
use crate::spec_launch::{launch_claude_cli, ClaudeCliLaunch};
use tddy_core::provider_route::Harness;

impl LaunchSessions {
    /// Handle `ResumeSession` for `session_type = "claude-cli"` sessions.
    pub(crate) async fn resume_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: &Path,
        session_dir: PathBuf,
        meta: tddy_core::SessionMetadata,
        // The caller's token, re-exported to a split session's agent as TDDY_REMOTE_SESSION_TOKEN:
        // the codebase daemon verifies it on every tool call, so the resumed agent needs a live one.
        session_token: &str,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if meta.sandbox == Some(true) {
            return self
                .resume_sandboxed_claude_cli_session(
                    os_user,
                    session_id,
                    session_token,
                    session_dir,
                    meta,
                )
                .await;
        }
        let model = meta.model.clone().unwrap_or_default();

        // A split session has no `repo_path` here and its `TDDY_REMOTE_*` wiring was injected at
        // spawn time, so both are re-derived from the persisted pairing — including a **fresh** join
        // token, since the original is scoped to a lifetime that may well have elapsed while the
        // session was stopped.
        let split = self
            .split_sessions
            .resume_split_wiring(
                &meta,
                sessions_base,
                &session_dir,
                session_id,
                session_token,
            )
            .await?;
        let worktree_path = split
            .as_ref()
            .map(|w| w.context_dir.clone())
            .or_else(|| meta.repo_path.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| session_dir.clone());
        let is_split = split.is_some();
        // A role's wiring is not in a persisted argv, so a resumed agent is given it again.
        let (flags, split_env) = self
            .resumed_role(&meta, session_id, &session_dir, session_token)?
            .after_split(split);

        let manager = Arc::clone(&self.claude_cli_manager);
        let session_id_owned = session_id.to_string();
        let binary_owned = hooks_and_urls::resolve_resume_session_claude_binary(&self.config);

        // Re-wire managed-workflow orchestration when resuming a managed session — metadata records a
        // recipe only for managed sessions. The controller resumes at the goal persisted in
        // changeset.yaml so the workflow continues from where it left off, not from the start goal.
        let mut managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        // One resolution for the project this session belongs to; a refusal (or a project that can no
        // longer be read) resumes the agent under the checkout's own identity, with the reason logged.
        // A split agent gets no commit pairs: it has no checkout (its working directory is a context
        // dir) and commits are made by the tools running on the codebase daemon, never by this
        // process's environment.
        let identity =
            self.host
                .session_identity(os_user, session_id, &meta.project_id, session_token);
        let github_credential_handler = identity.github_credential;
        let mut env_extra = self.resumed_agent_env(
            (!is_split).then_some(identity.git_environment),
            split_env,
            session_token,
            &session_dir,
        )?;
        let in_tmux = meta.tmux_session.is_some();
        if let Some(recipe_name) = resumed_recipe(&manager, &meta, os_user, session_id).await? {
            let recipe = tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(recipe_name)
                .map_err(Status::invalid_argument)?;
            let resume_goal = LaunchSessions::managed_resume_goal(&session_dir, &recipe);
            let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
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
                github_credential_handler,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            env_extra.extend(launch.env);
            managed = Some(launch.workflow);
        }

        // A resumed agent is given its notify wiring again (`tddy-tools notify`).
        let append_system_prompt_file = self.resumed_notify(
            os_user,
            &session_dir,
            &meta,
            append_system_prompt_file,
            &mut env_extra,
        )?;

        let pid = if in_tmux {
            // TODO(agent-tools-cascade): a tmux-run session (#tmux-agent) re-attaches or resumes
            // through `claude_cli_tmux::relaunch_claude_cli` with the hand-built argv; confirm the
            // spec-rendered resume argv should be the one started in a fresh tmux pane.
            let launch = ClaudeCliTmuxLaunch {
                os_user,
                session_id: &session_id_owned,
                worktree_path,
                model: &model,
                binary: &binary_owned,
                append_system_prompt_file: append_system_prompt_file.as_deref(),
                extra_args: flags.hand_built_args()?,
                env: env_extra,
                session_dir: session_dir.clone(),
                ..Default::default()
            };
            relaunch_claude_cli(&manager, true, launch).await?
        } else {
            self.relaunch_claude_cli(
                &session_id_owned,
                &worktree_path,
                &model,
                append_system_prompt_file.as_deref(),
                flags,
                env_extra,
            )
            .await?
            .pid
        };

        if let Some(mw) = managed {
            manager.attach_managed_workflow(&session_id_owned, mw).await;
        }
        record_main_terminal(&manager, &self.tddy_data_dir, &session_id_owned).await;

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

        log::info!(target: "tddy_daemon::connection_service", "resumed claude-cli session {session_id} pid={pid}");
        Ok(resumed_without_livekit(session_id))
    }
}

impl LaunchSessions {
    /// Spawn the process of a resumed claude-cli session — from the `claude` spec when the host has
    /// coding agents, the hand-built argv otherwise. A resume never replays the
    /// prompt or carries a permission mode over, never rewrites the hooks file its start wrote, and
    /// runs as the daemon.
    async fn relaunch_claude_cli(
        &self,
        session_id: &str,
        worktree_path: &Path,
        model: &str,
        append_system_prompt_file: Option<&Path>,
        flags: ResumedFlags,
        env: Vec<(String, String)>,
    ) -> Result<Arc<tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle>, Status> {
        let relaunched = match self.coding_agents.as_deref() {
            Some(agents) => {
                let (extra_args, toolset) = flags.spec_launch();
                launch_claude_cli(ClaudeCliLaunch {
                    remote_host: None,
                    agents,
                    config: &self.config,
                    manager: &self.claude_cli_manager,
                    session_id,
                    worktree: worktree_path,
                    model,
                    resume: true,
                    initial_prompt: None,
                    permission_mode: None,
                    dangerously_skip_permissions: false,
                    append_system_prompt_file,
                    toolset,
                    hooks: None,
                    extra_args,
                    env,
                    os_user: None,
                })
                .await
                .map_err(anyhow::Error::from)
            }
            None => {
                let extra_args = flags.hand_built_args()?;
                self.claude_cli_manager
                    .resume_with_options(
                        session_id,
                        worktree_path.to_path_buf(),
                        model,
                        &hooks_and_urls::resolve_resume_session_claude_binary(&self.config),
                        append_system_prompt_file,
                        extra_args,
                        env,
                    )
                    .await
            }
        };
        relaunched.map_err(|e| Status::internal(format!("failed to relaunch claude-cli: {}", e)))
    }
}

/// The answer to a claude-cli resume: the session, with no LiveKit room — its terminal is bridged
/// when a consumer arrives, not at resume.
fn resumed_without_livekit(session_id: &str) -> Response<ResumeSessionResponse> {
    Response::new(ResumeSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
    })
}

impl LaunchSessions {
    /// The system-prompt file a resumed claude-cli agent appends: `managed_prompt_file` composed
    /// with the notify guidance, and `env_extra` given the notify environment, when the session
    /// recorded a hook token and has no standing role; `managed_prompt_file` unchanged otherwise.
    fn resumed_notify(
        &self,
        os_user: &str,
        session_dir: &Path,
        meta: &tddy_core::SessionMetadata,
        managed_prompt_file: Option<PathBuf>,
        env_extra: &mut Vec<(String, String)>,
    ) -> Result<Option<PathBuf>, Status> {
        let notify = crate::notify_env::resumed_agent_notify_launch(
            &self.config,
            &self.tddy_data_dir,
            os_user,
            session_dir,
            meta,
            managed_prompt_file.as_deref(),
        )?;
        env_extra.extend(notify.env);
        Ok(notify.append_system_prompt_file.or(managed_prompt_file))
    }
}

impl LaunchSessions {
    /// The environment a resumed claude-cli agent starts from: the checkout's commit identity
    /// (`None` for a split agent), the split wiring, and the provider account the session's start
    /// recorded — re-read from the owner's vault, so a rotated key is what the resume runs on and a
    /// removed account refuses the resume rather than running it on the vendor. It runs the renderer
    /// `claude_cli.fullscreen` asks for, as a fresh start does.
    fn resumed_agent_env(
        &self,
        git_environment: Option<Vec<(String, String)>>,
        split_env: Vec<(String, String)>,
        session_token: &str,
        session_dir: &Path,
    ) -> Result<Vec<(String, String)>, Status> {
        let route_env = resume_route_env(
            &self.host.session_account_access(session_token),
            Harness::ClaudeCli,
            session_dir,
        )?;
        Ok(git_environment
            .unwrap_or_default()
            .into_iter()
            .chain(split_env)
            .chain(route_env)
            .chain(self.config.claude_cli_renderer_env())
            .collect())
    }
}

/// The recipe a resume prepares a managed workflow for: the session's recorded one, unless its agent
/// is live in tmux and this daemon still serves its workflow.
///
/// A live tmux session holds the agent the session recorded, and its `TDDY_SOCKET` names the
/// toolcall socket of the workflow this daemon already serves: that workflow is kept, never
/// replaced — replacing it would drop the old listener, which unlinks the very socket the agent is
/// using. Only a daemon with no such workflow (a restart) prepares one, and it binds the same path.
async fn resumed_recipe<'m>(
    manager: &CliSessionManager,
    meta: &'m tddy_core::SessionMetadata,
    os_user: &str,
    session_id: &str,
) -> Result<Option<&'m str>, Status> {
    let Some(recipe_name) = meta.recipe.as_deref().filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    let keeps_workflow = meta.tmux_session.is_some()
        && agent_tmux_session_is_alive(os_user, session_id).await?
        && manager.has_managed_workflow(session_id).await;
    Ok((!keeps_workflow).then_some(recipe_name))
}
