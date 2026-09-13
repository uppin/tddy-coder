use crate::{
    connection_service::stack_parent, livekit_peer_discovery::local_instance_id_for_config,
};

use super::spawn_claude_cli_session_inner;

use uuid::Uuid;

use super::conversation_branch_slug;

use super::GrillMeConversationSpawnHandler;

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
            &self.claude_cli_manager.task_registry(),
        )
        .await
        .map_err(|status| status.message().to_string())?;
        Ok(response.into_inner().session_id)
    }
}
