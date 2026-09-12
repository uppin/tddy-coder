use super::spawn_claude_cli_session_inner;

use super::AttachmentProgressSink;

use super::AttachmentMaterialization;

use uuid::Uuid;

use crate::{
    connection_service::stack_parent, livekit_peer_discovery::local_instance_id_for_config,
};

use super::StackChildSpawnHandler;

#[async_trait::async_trait]
impl tddy_core::toolcall::ChildSpawnHandler for StackChildSpawnHandler {
    async fn spawn_child(&self, node_id: &str) -> Result<String, String> {
        let changeset = tddy_core::read_changeset(&self.orchestrator_session_dir)
            .map_err(|e| format!("failed to read orchestrator changeset: {e}"))?;
        let stack = changeset
            .stack
            .ok_or_else(|| "orchestrator changeset has no stack".to_string())?;
        let node = stack
            .nodes
            .iter()
            .find(|n| n.node_id == node_id)
            .ok_or_else(|| format!("no planned PR node with id '{node_id}' in the stack"))?;
        // "Already spawned" means the node owns a branch: that branch is the work, and it outlives
        // whichever session created it. Spawning again would try to create a branch that exists.
        if let Some(branch) = node.branch.as_deref() {
            return Err(format!("node '{node_id}' already owns branch '{branch}'"));
        }
        let new_branch_name = node
            .branch_suggestion
            .clone()
            .ok_or_else(|| format!("node '{node_id}' has no branch_suggestion to create"))?;
        let node_brief = if node.description.trim().is_empty() {
            node.title.clone()
        } else {
            format!("{}\n\n{}", node.title, node.description)
        };
        // The documents the orchestrator authored for this node, attached by reference. One that
        // has not been written yet is skipped rather than fatal — starting a node before the docs
        // pass has run is sometimes correct (`docs/ft/coder/pr-stack-docs.md`).
        let attachments = crate::stack_doc_attachments::stack_doc_attachments(
            &self.orchestrator_session_dir,
            &self.orchestrator_session_id,
            &local_instance_id_for_config(&self.config),
            node_id,
        );
        // Inherit the orchestrator's model — the daemon has no standalone model default and an
        // empty model is rejected by the spawn path.
        let meta = tddy_core::read_session_metadata(&self.orchestrator_session_dir)
            .map_err(|e| format!("failed to read orchestrator session metadata: {e}"))?;
        let model = meta.model.clone().unwrap_or_default();
        if model.trim().is_empty() {
            return Err("orchestrator session has no model to inherit for the child".to_string());
        }

        let child_session_id = Uuid::new_v4().to_string();
        // Materialized before the spawn, exactly as `start_session_core` does for a `StartSession`
        // request: the child's `artifacts/attachments/` is in place by the time its agent starts.
        // The session token is empty because every ref this handler builds names *this* daemon —
        // the orchestrator is local to it — so the read never leaves the host and never needs one.
        let materialized = self
            .service
            .prepare_session_attachments(&AttachmentMaterialization {
                session_token: "",
                os_user: &self.os_user,
                sessions_base: &self.sessions_base,
                session_id: &child_session_id,
                attachments: &attachments,
                progress: &AttachmentProgressSink::discarding(),
            })
            .await
            .map_err(|status| {
                format!(
                    "failed to attach the node's documents: {}",
                    status.message()
                )
            })?;
        // The changeset is named in the prompt so the agent reads its boundaries before writing
        // code instead of finding the file by chance — the same hand-off the grill-me brief gets,
        // and the same rule the Start-session dialog's children go through.
        let initial_prompt = crate::stack_doc_attachments::prompt_with_attached_changeset(
            &node_brief,
            &materialized,
        );
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
            "",
            "",
            &initial_prompt,
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
            // A spawned child session never runs its own semantic index.
            false,
            // Child spawns are created by the orchestrator agent, not the Start-Session dialog, and
            // never push a remote branch here.
            false,
            &self.claude_cli_manager.task_registry(),
        )
        .await
        .map_err(|status| status.message().to_string())?;
        Ok(response.into_inner().session_id)
    }
}
