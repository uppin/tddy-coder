use crate::connection_service::stack_parent;

use super::GrillMeConversationSpawnHandler;

use super::recipe_enables_conversation_spawn;

use std::path::Path;

use super::spawn_claude_cli_session_inner;

use super::StackChildSpawnHandler;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use std::path::PathBuf;

use super::AttachmentProgressSink;

use super::launch_ports::LaunchSessions;

use super::session_acting_identity::SessionAccountAccess;

impl LaunchSessions {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: PathBuf,
        model: &str,
        project_id: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        initial_prompt: &str,
        permission_mode: &str,
        dangerously_skip_permissions: bool,
        stack_parent: Option<&str>,
        // The daemon whose sessions tree holds `stack_parent` (empty = this one), and the caller's
        // token, which is what that daemon verifies the forwarded question with.
        stack_parent_daemon_instance_id: &str,
        // The planned node of that parent's stack this child materializes, named by the surface
        // that started it. Empty = derive the node from the branch, locally (D34).
        stack_node_id: &str,
        session_token: &str,
        // When `Some`, the session is launched workflow-aware: the recipe's orchestration prompt is
        // injected and its `transition` tool advances a per-session `WorkflowController`.
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
        // When true, index the worktree before launch and expose the `SemanticSearch` tool.
        semantic_index: bool,
        // When true (new_branch_from_base only), push the new branch to origin at session start.
        create_remote_branch: bool,
        ssh_config_host: &str,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
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
                    sessions_base: sessions_base.clone(),
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
                &sessions_base,
                &sessions_base.join(SESSIONS_SUBDIR).join(session_id),
                self.host.session_account_access(session_token),
            )
        });
        spawn_claude_cli_session_inner(
            &self.config,
            &self.tddy_data_dir,
            &self.claude_cli_manager,
            os_user,
            session_id,
            sessions_base,
            model,
            project_id,
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
            initial_prompt,
            permission_mode,
            dangerously_skip_permissions,
            match stack_parent {
                Some(session_id) => stack_parent::SpawnStackParent::OwnedBy {
                    session_id,
                    daemon_instance_id: stack_parent_daemon_instance_id,
                    stack_node_id,
                    session_token,
                    host: self,
                },
                None => stack_parent::SpawnStackParent::NoParent,
            },
            managed_recipe,
            child_spawn_handler,
            conversation_spawn_handler,
            semantic_index,
            create_remote_branch,
            ssh_config_host,
            &self.host.session_account_access(session_token),
            &self.task_registry,
            progress,
        )
        .await
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
    pub(crate) github_credential: Option<super::session_acting_identity::SharedGithubCredential>,
}
