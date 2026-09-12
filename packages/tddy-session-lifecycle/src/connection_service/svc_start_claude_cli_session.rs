use crate::connection_service::{seeded_clone_guard, stack_parent};

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

use super::DaemonSessionHost;

impl DaemonSessionHost {
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
        managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
        // When true, index the worktree before launch and expose the `SemanticSearch` tool.
        semantic_index: bool,
        // When true (new_branch_from_base only), push the new branch to origin at session start.
        create_remote_branch: bool,
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
            &self.task_registry,
        )
        .await
    }

    /// Build the per-session [`ConversationSpawnHandler`] for a managed session when its recipe
    /// enables conversation spawning (grill-me). Returns `None` for recipes that don't (a plain TDD
    /// session, or a PR-stack orchestrator which uses `spawn-child` instead), so `spawn_conversation`
    /// is rejected there rather than silently spawning.
    pub(crate) fn conversation_spawn_handler_for(
        &self,
        recipe: &Arc<dyn tddy_core::backend::WorkflowRecipe>,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        sessions_base: &Path,
        orchestrator_session_dir: &Path,
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
        }))
    }

    /// Bind a per-session unix socket hosting
    /// [`HostSessionService`](crate::host_session_service::HostSessionService) and return its path,
    /// to be passed to the spawned grill-me coder as `--host-session-socket`. The coder connects and
    /// relays `spawn_conversation` back over it. The orchestrator context (this session) is baked
    /// into the handler, and the path is unique per session and handed only to that session's coder,
    /// so a call arriving here is unambiguously that session — no auth token or caller id needed.
    ///
    /// A socket **path** (unlike the child's stdio fds) crosses the forked `spawn_worker` boundary as
    /// a plain string, so this works for both the worker-spawned and direct spawn paths. Binding
    /// happens before the spawn, so the coder's later `connect()` finds the listener ready.
    pub(crate) async fn spawn_host_session_socket(
        &self,
        session_id: &str,
        os_user: &str,
        project_id: &str,
        model: Option<String>,
    ) -> Option<String> {
        let path = std::env::temp_dir().join(format!("tddy-host-{session_id}.sock"));
        let _ = std::fs::remove_file(&path); // clear any stale socket from a prior run
        let listener = match tokio::net::UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                log::warn!("spawn_host_session_socket({session_id}): bind {path:?}: {e}");
                return None;
            }
        };
        // Dev runs the coder as the same OS user; loosen perms so a cross-user child can still
        // connect. TODO(stdio-relay): tighten perms / socket ownership for cross-user production.
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777));

        let sessions_base = self.tddy_data_dir.clone();
        let orchestrator_session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        let handler = Arc::new(GrillMeConversationSpawnHandler {
            stack_parent_host: Arc::new(self.clone()),
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            claude_cli_manager: Arc::clone(&self.claude_cli_manager),
            os_user: os_user.to_string(),
            project_id: project_id.to_string(),
            sessions_base,
            orchestrator_session_id: session_id.to_string(),
            orchestrator_session_dir,
            model_override: model,
        });
        let service = crate::host_session_service::HostSessionService::new(handler);
        let session_stdio = Arc::clone(&self.session_stdio);
        let sid = session_id.to_string();
        // Accept the coder's single connection, then run the reverse RPC endpoint over it.
        tokio::spawn(async move {
            let stream = match listener.accept().await {
                Ok((stream, _addr)) => stream,
                Err(e) => {
                    log::warn!("spawn_host_session_socket({sid}): accept: {e}");
                    return;
                }
            };
            let (reader, writer) = tokio::io::split(stream);
            let (client, endpoint) =
                tddy_stdio::StdioEndpoint::from_duplex(reader, writer, service);
            let task = tokio::spawn(endpoint.run());
            session_stdio.lock().await.insert(
                sid.clone(),
                seeded_clone_guard::SessionStdioEndpoint { client, task },
            );
            log::info!("spawn_host_session_socket({sid}): reverse endpoint connected + ready");
        });
        log::info!("spawn_host_session_socket({session_id}): listening at {path:?}");
        Some(path.to_string_lossy().into_owned())
    }
}
