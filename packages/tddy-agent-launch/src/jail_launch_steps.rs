use tddy_session_split::service_util;

use crate::launch_ports::LaunchSessions;

use std::path::PathBuf;

use crate::svc_start_sandboxed_claude_cli_session::ManagedJailEnv;

use std::sync::Arc;

use std::path::Path;

use crate::svc_start_sandboxed_claude_cli_session::JailSession;

use tddy_rpc::Status;

use crate::session_acting_identity::{RelayUser, SessionIdentity, SharedGithubCredential};

/// A jail's relay environment and the handler its tools' `github-token` requests are answered by.
pub(crate) type JailRelayIdentity = (Vec<(String, String)>, Option<SharedGithubCredential>);

impl LaunchSessions {
    /// Records the jailed `main` terminal of `session_id`, whose sandbox state was just registered:
    /// its history outlives the jail (see `SandboxSessionManager::record_main_capture`).
    pub(crate) async fn record_jailed_main(&self, session_id: &str) {
        self.sandbox_manager
            .record_main_capture(
                session_id,
                tddy_cli_sessions::cli_session_manager::main_capture_recording_path(
                    &self.tddy_data_dir,
                    session_id,
                ),
            )
            .await;
    }

    /// What a jail's host-side relay runs its commands under, and the handler its tools' token
    /// requests are answered by, for a session of `os_user`.
    ///
    /// The relay runs in this daemon's process. When that is not the session's OS user, a Local gh
    /// identity is not carried over — the daemon user's `gh` would answer instead — and the relay's
    /// `gh` is logged out by an empty `GH_CONFIG_DIR`.
    pub(crate) fn jail_relay_identity(
        &self,
        identity: SessionIdentity,
        os_user: &str,
    ) -> Result<JailRelayIdentity, Status> {
        // Only a Local gh identity depends on whose `gh` the relay reaches.
        if !identity.is_local_gh {
            let github_credential = identity.github_credential.clone();
            return Ok((identity.git_environment, github_credential));
        }
        let session_user = tddy_daemon_kernel::privilege_drop::resolve_pty_os_user(os_user)
            .map_err(Status::internal)?;
        let relay = if session_user.uid == unsafe { libc::geteuid() } {
            RelayUser::SessionUser
        } else {
            RelayUser::OtherUser
        };
        let logged_out_gh_config = self.tddy_data_dir.join("logged-out-gh");
        std::fs::create_dir_all(&logged_out_gh_config).map_err(|e| {
            Status::internal(format!(
                "failed to create {}: {e}",
                logged_out_gh_config.display()
            ))
        })?;
        let github_credential = identity.github_credential.clone();
        Ok((
            identity.relayed_by(relay, &logged_out_gh_config),
            github_credential,
        ))
    }

    pub(super) async fn warm_up_jail_agents(
        &self,
        specialized_agents: &[String],
        clone_placed_agents: &[String],
    ) -> Result<
        (
            Vec<tddy_core::SessionAgentRecord>,
            Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
        ),
        Status,
    > {
        let started_agents = self
            .agent_roster
            .seeded_roster_records_placed(specialized_agents, clone_placed_agents)
            .await?;
        // The defs behind those records, which the jail env can only carry for agents this host
        // holds — the records above are what carries the rest.
        let specialized_defs = self
            .agent_roster
            .resolve_specialized_agent_defs(specialized_agents)
            .await?;

        // Readiness gate: wake every specialized agent's endpoint and wait until each answers
        // before spawning the jail, so a cold/unreachable model fails session start here rather
        // than stalling the main agent's first subagent call. No fallback — the jail is never
        // spawned if warm-up fails. Resume gates separately, in `relaunch_sandboxed_runner`.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok((started_agents, specialized_defs))
    }

    pub(super) fn managed_jail_env(
        &self,
        jail: &JailSession<'_>,
        os_user: &str,
        sessions_base: &Path,
        managed_recipe: &Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        context_dir: &Path,
        tddy_tools_path: &str,
    ) -> Result<ManagedJailEnv, Status> {
        let mut managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow> = None;
        let mut append_system_prompt_file: Option<PathBuf> = None;
        // The agent in the jail edits and commits through the host-side Shell relay, which runs
        // under this env — so the project's account identity belongs here, managed or not. The
        // token does not: the agent's tools ask the host for it over the session's own socket.
        let identity = self.host.session_identity(
            os_user,
            jail.session_id,
            jail.project_id,
            jail.session_token,
        );
        let (mut session_env, github_credential) = self.jail_relay_identity(identity, os_user)?;
        if let Some(recipe) = managed_recipe.clone() {
            // A grill-me session gets a conversation-spawn handler bound to its toolcall listener so
            // the agent's `spawn_conversation` relay can start a fresh implementation conversation.
            let conversation_spawn_handler = self.conversation_spawn_handler_for(
                &recipe,
                os_user,
                jail.session_id,
                jail.project_id,
                sessions_base,
                jail.session_dir,
                self.host.session_account_access(jail.session_token),
            );
            let launch = self.prepare_managed_workflow(
                jail.session_id,
                recipe,
                jail.session_dir,
                jail.worktree_path,
                context_dir,
                tddy_tools_path,
                None,
                conversation_spawn_handler,
                // The host-side `tddy-tools` the jail's Shell relay runs reaches this listener, so
                // the token is asked of the host per call, as in a co-located session.
                github_credential,
            )?;
            append_system_prompt_file = Some(launch.prompt_file);
            session_env.extend(launch.env);
            managed = Some(launch.workflow);
        }
        Ok((managed, append_system_prompt_file, session_env))
    }

    pub(super) async fn jail_semantic_index_env(
        &self,
        session_id: &str,
        semantic_index: bool,
        session_dir: &Path,
        worktree_path: &Path,
    ) -> Result<Option<(String, String)>, Status> {
        let mut semantic_index_env_pair: Option<(String, String)> = None;
        if semantic_index {
            service_util::index_session_worktree(
                &self.tddy_data_dir,
                &self.task_registry,
                session_id,
                worktree_path,
                session_dir,
            )
            .await?;
            semantic_index_env_pair = Some(
                tddy_semantic_index::semantic_index::semantic_index_env(session_dir),
            );
        }
        Ok(semantic_index_env_pair)
    }
}
