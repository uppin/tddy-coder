use crate::stack_parent;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_tool_engine::start_ledger::StartLedger;
use tddy_tool_engine::start_session_args::{StartContext, StartSessionArgs};

use crate::claude_cli_spawn::spawn_claude_cli_session_inner;

use uuid::Uuid;

use crate::conversation_spawn::conversation_branch_slug;

use crate::conversation_spawn::GrillMeConversationSpawnHandler;

#[async_trait::async_trait]
impl tddy_core::toolcall::ConversationSpawnHandler for GrillMeConversationSpawnHandler {
    async fn spawn_conversation(
        &self,
        prompt: &str,
        branch: Option<&str>,
        base_ref: Option<&str>,
    ) -> Result<String, String> {
        if prompt.trim().is_empty() {
            return Err("spawn_conversation requires a non-empty prompt".to_string());
        }
        let new_branch_name = branch
            .map(str::to_string)
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| conversation_branch_slug(prompt));

        // Inherit the orchestrator's model — the daemon has no standalone model default and an
        // empty model is rejected by the spawn path.
        let meta = tddy_core::read_session_metadata(&self.orchestrator_session_dir)
            .map_err(|e| format!("failed to read orchestrator session metadata: {e}"))?;
        let model = meta
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| self.model_override.clone())
            .unwrap_or_default();
        if model.trim().is_empty() {
            return Err(
                "orchestrator session has no model to inherit for the conversation".to_string(),
            );
        }

        let child_session_id = Uuid::new_v4().to_string();
        let response = spawn_claude_cli_session_inner(
            &self.config,
            &self.tddy_data_dir,
            &self.claude_cli_manager,
            &self.os_user,
            &child_session_id,
            self.sessions_base.clone(),
            &model,
            &self.project_id,
            "new_branch_from_base",
            &new_branch_name,
            base_ref.unwrap_or_default(),
            "",
            prompt,
            "auto",
            false,
            // The orchestrator is a session of this daemon, so its stack is resolved off this
            // daemon's own disk and no credential travels anywhere.
            stack_parent::SpawnStackParent::OwnedBy {
                session_id: &self.orchestrator_session_id,
                daemon_instance_id: &local_instance_id_for_config(&self.config),
                // The orchestrator agent spawns by branch, in its own process on its own host, so
                // the node is found from the branch here exactly as it always was (D34).
                stack_node_id: "",
                session_token: "",
                host: self.stack_parent_host.as_ref(),
            },
            None,
            None,
            None,
            // A spawned child conversation never runs its own semantic index.
            false,
            // Child conversations are spawned by the orchestrator, never pushing a remote branch.
            false,
            "",
            // A spawned child conversation is never a managed-codebase session.
            false,
            // A spawned child conversation has no standing role.
            crate::project_home_launch::StartRole::ordinary(),
            // A child conversation cuts its own worktree.
            "",
            // The orchestrator's owner's vault: a child is the same owner's, on the same project.
            &self.account_access,
            // A child conversation runs on its vendor: the orchestrator's account is not inherited.
            &tddy_core::provider_route::RouteSources::default(),
            &self.claude_cli_manager.task_registry(),
            // Nobody watches a spawned child's start: the orchestrator is told its id, not its steps.
            &tddy_session_files::attachment_progress::AttachmentProgressSink::discarding(),
        )
        .await
        .map_err(|status| status.message().to_string())?;
        Ok(response.into_inner().session_id)
    }

    async fn start_conversation(&self, args_json: &str) -> Result<String, String> {
        let args: StartSessionArgs = serde_json::from_str(args_json)
            .map_err(|e| format!("invalid spawn_conversation arguments: {e}"))?;
        // The start authenticates as the orchestrator's own session, exactly as the `StartSession`
        // tool does: a start on another host and its cancel carry that token. A session that holds
        // none cannot be vouched for, so it starts nothing.
        let session_token = self.account_access.session_token();
        if session_token.is_empty() {
            return Err(
                "this session holds no start token, so it cannot start a conversation".to_string(),
            );
        }
        let request = args.into_request(StartContext {
            session_token: session_token.to_string(),
            project_id: self.project_id.clone(),
            orchestrator_session_id: self.orchestrator_session_id.clone(),
            orchestrator_daemon_instance_id: local_instance_id_for_config(&self.config),
        });
        let handle = self
            .service
            .begin_recorded_start(request, &self.os_user, self.start_ledger()?)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&handle).map_err(|e| format!("serialize the start's handle: {e}"))
    }

    async fn conversation_start_status(&self, start_id: &str) -> Result<String, String> {
        let record = self
            .start_ledger()?
            .read(start_id)
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&record).map_err(|e| format!("serialize the start's record: {e}"))
    }

    async fn cancel_conversation_start(&self, start_id: &str) -> Result<(), String> {
        self.service
            .cancel_recorded_start(
                &self.start_ledger()?,
                start_id,
                self.account_access.session_token(),
            )
            .await
            .map_err(|e| e.to_string())
    }
}

impl GrillMeConversationSpawnHandler {
    /// The start ledger of the orchestrator this handler serves.
    fn start_ledger(&self) -> Result<StartLedger, String> {
        StartLedger::recovered_in_session_dir(&self.orchestrator_session_dir)
            .map_err(|e| e.to_string())
    }
}
