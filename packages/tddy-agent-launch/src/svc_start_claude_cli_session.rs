use crate::conversation_spawn::GrillMeConversationSpawnHandler;

use crate::conversation_spawn::recipe_enables_conversation_spawn;

use std::path::Path;

use crate::stack_child_spawn::StackChildSpawnHandler;

use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;

use crate::launch_ports::LaunchSessions;

use crate::session_acting_identity::SessionAccountAccess;

/// A managed session's child spawner and conversation spawner, each present only for its recipe.
pub(crate) type ManagedSpawnHandlers = (
    Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>>,
    Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
);

impl LaunchSessions {
    /// The handlers a managed session's toolcall listener is bound to: a pr-stack orchestrator's
    /// child spawner (the agent's `pr_spawn_child` relay materializes planned nodes into child
    /// sessions) and, for a grill-me session, the conversation spawner. Neither for any other
    /// recipe, or for a session with none.
    pub(crate) fn managed_spawn_handlers(
        &self,
        managed_recipe: &Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        sessions_base: &Path,
        session_token: &str,
    ) -> ManagedSpawnHandlers {
        // A pr-stack orchestrator gets a child-spawn handler bound to its toolcall listener so the
        // agent's `pr_spawn_child` relay can materialize planned nodes into child sessions.
        let child_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>> =
            if managed_recipe
                .as_ref()
                .is_some_and(|r| r.name() == "pr-stack")
            {
                let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
                Some(Arc::new(StackChildSpawnHandler {
                    stack_parent_host: Arc::new(self.clone()),
                    service: self.clone(),
                    config: self.config.clone(),
                    tddy_data_dir: self.tddy_data_dir.clone(),
                    claude_cli_manager: Arc::clone(&self.claude_cli_manager),
                    os_user: os_user.to_string(),
                    project_id: project_id.to_string(),
                    sessions_base: sessions_base.to_path_buf(),
                    orchestrator_session_id: session_id.to_string(),
                    orchestrator_session_dir: session_dir,
                    account_access: self.host.session_account_access(session_token),
                }))
            } else {
                None
            };
        // A grill-me session instead gets a conversation-spawn handler so the agent's
        // `spawn_conversation` relay can start a fresh implementation conversation.
        let conversation_spawn_handler = managed_recipe.as_ref().and_then(|recipe| {
            self.conversation_spawn_handler_for(
                recipe,
                os_user,
                session_id,
                project_id,
                sessions_base,
                &sessions_base.join(SESSIONS_SUBDIR).join(session_id),
                self.host.session_account_access(session_token),
            )
        });
        (child_spawn_handler, conversation_spawn_handler)
    }

    /// Build the per-session [`ConversationSpawnHandler`] for a managed session when its recipe
    /// enables conversation spawning (grill-me). Returns `None` for recipes that don't (a plain TDD
    /// session, or a PR-stack orchestrator which uses `spawn-child` instead), so `spawn_conversation`
    /// is rejected there rather than silently spawning.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn conversation_spawn_handler_for(
        &self,
        recipe: &Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        sessions_base: &Path,
        orchestrator_session_dir: &Path,
        // The orchestrator's owner's vault access: a conversation it spawns is that owner's, on the
        // same project, and resolves its account through this.
        account_access: SessionAccountAccess,
    ) -> Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>> {
        if !recipe_enables_conversation_spawn(recipe.name()) {
            return None;
        }
        Some(Arc::new(GrillMeConversationSpawnHandler {
            service: self.clone(),
            stack_parent_host: Arc::new(self.clone()),
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            claude_cli_manager: Arc::clone(&self.claude_cli_manager),
            os_user: os_user.to_string(),
            project_id: project_id.to_string(),
            sessions_base: sessions_base.to_path_buf(),
            orchestrator_session_id: session_id.to_string(),
            model_override: None,
            orchestrator_session_dir: orchestrator_session_dir.to_path_buf(),
            account_access,
        }))
    }

    /// Put a starting or resuming tool session on its OS user's host-session socket and return the
    /// path to hand its coder as `--host-session-socket`.
    ///
    /// The socket is the user's (see [`super::host_session_socket`]): bound here if this daemon is
    /// not already serving it, shared by every tool session of that user. What is per-session is the
    /// registration: the session's id maps to the handlers its requests are answered by — the
    /// `github-token` handler built from its assignment snapshot and start token, and, for a recipe
    /// that spawns conversations (grill-me), the conversation handler. A resume registers again and
    /// replaces the entry, so it answers from the refreshed assignments and token.
    ///
    /// `None` when the socket cannot be bound with owner-only access for that user (logged with the
    /// reason): the session then starts without the flag, and its tools refuse a token request as
    /// having no credential handler — there is no wider-permission or environment fallback.
    pub(crate) async fn register_tool_session_on_host_socket(
        &self,
        session: ToolSessionHostRegistration<'_>,
    ) -> Option<String> {
        let path = match self
            .host_session_sockets
            .ensure_bound(session.os_user, &self.tddy_data_dir)
            .await
        {
            Ok(path) => path,
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "session {} starts without a host-session socket: {e:#}",
                    session.session_id
                );
                return None;
            }
        };
        let conversation_spawn_handler: Option<
            Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>,
        > = session
            .recipe
            .filter(|recipe| recipe_enables_conversation_spawn(recipe))
            .map(|_| {
                let sessions_base = self.tddy_data_dir.clone();
                Arc::new(GrillMeConversationSpawnHandler {
                    service: self.clone(),
                    stack_parent_host: Arc::new(self.clone()),
                    config: self.config.clone(),
                    tddy_data_dir: self.tddy_data_dir.clone(),
                    claude_cli_manager: Arc::clone(&self.claude_cli_manager),
                    os_user: session.os_user.to_string(),
                    project_id: session.project_id.to_string(),
                    orchestrator_session_dir: sessions_base
                        .join(SESSIONS_SUBDIR)
                        .join(session.session_id),
                    sessions_base,
                    orchestrator_session_id: session.session_id.to_string(),
                    model_override: session.model,
                    account_access: session.account_access,
                }) as Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>
            });
        self.host_session_sockets.registry().register(
            session.session_id,
            tddy_host_service::host_session_service::RegisteredSession {
                os_user: session.os_user.to_string(),
                conversation_spawn_handler,
                github_credential_handler: session.github_credential,
            },
        );
        Some(path.to_string_lossy().into_owned())
    }
}

impl LaunchSessions {
    /// The tool session's spawn returned: watch its process, and stop answering the session when it
    /// stops. A spawn that failed leaves nothing registered — nobody is running to ask.
    pub(crate) fn tool_session_spawned<E>(
        &self,
        session_id: &str,
        spawned: &Result<tddy_spawn::spawner::SpawnResult, E>,
    ) {
        match spawned {
            Ok(result) => self
                .host_session_sockets
                .watch_until_stopped(session_id, result.pid),
            Err(_) => self.host_session_sockets.registry().unregister(session_id),
        }
    }
}

/// What a tool session registers on its OS user's host-session socket.
pub(crate) struct ToolSessionHostRegistration<'a> {
    pub(crate) os_user: &'a str,
    pub(crate) session_id: &'a str,
    pub(crate) project_id: &'a str,
    /// The session's recipe; decides whether it may spawn conversations.
    pub(crate) recipe: Option<&'a str>,
    pub(crate) model: Option<String>,
    /// The owner's vault access a conversation this session spawns resolves its account through.
    pub(crate) account_access: SessionAccountAccess,
    /// Answers the session's `github_token` request; `None` when its project could not be read.
    pub(crate) github_credential: Option<crate::session_acting_identity::SharedGithubCredential>,
}
