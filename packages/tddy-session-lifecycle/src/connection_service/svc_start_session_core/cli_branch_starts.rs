use crate::connection_service::stack_parent;

use super::DaemonSessionHost;

use uuid::Uuid;

use super::super::AttachmentMaterialization;

use super::super::AttachmentProgressSink;

use std::path::Path;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use super::CliStart;

use tddy_service::proto::session::StartSessionRequest;

impl DaemonSessionHost {
    pub(super) async fn start_sandboxed_claude_cli_from_request(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        stack_parent_for_claude_cli: Option<&str>,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_sandboxed_claude_cli_session(
            os_user,
            &start.session_id,
            &req.session_token,
            start.sessions_base,
            req.model.trim(),
            req.project_id.trim(),
            req.repo_path.trim(),
            req.branch_worktree_intent.trim(),
            req.new_branch_name.trim(),
            req.selected_integration_base_ref.trim(),
            req.selected_branch_to_work_on.trim(),
            &start.initial_prompt,
            &req.claude_args,
            req.permission_mode.trim(),
            req.dangerously_skip_permissions,
            stack_parent_for_claude_cli,
            req.stack_parent_daemon_instance_id.trim(),
            req.stack_node_id.trim(),
            req.managed_codebase,
            &req.specialized_agents,
            managed_recipe,
            req.semantic_index,
            req.create_remote_branch,
        )
        .await
    }

    pub(super) async fn start_claude_cli_from_request(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        stack_parent_for_claude_cli: Option<String>,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_claude_cli_session(
            os_user,
            &start.session_id,
            start.sessions_base,
            req.model.trim(),
            req.project_id.trim(),
            req.branch_worktree_intent.trim(),
            req.new_branch_name.trim(),
            req.selected_integration_base_ref.trim(),
            req.selected_branch_to_work_on.trim(),
            &start.initial_prompt,
            req.permission_mode.trim(),
            req.dangerously_skip_permissions,
            stack_parent_for_claude_cli.as_deref(),
            req.stack_parent_daemon_instance_id.trim(),
            req.stack_node_id.trim(),
            &req.session_token,
            managed_recipe,
            req.semantic_index,
            req.create_remote_branch,
            req.ssh_config_host.trim(),
        )
        .await
    }

    /// Materialize the request's attachments into the session, and return its first prompt with a
    /// line naming the attached changeset when one materialized.
    pub(in crate::connection_service) async fn attached_initial_prompt(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        sessions_base: &Path,
        session_id: &str,
        progress: &AttachmentProgressSink,
    ) -> Result<String, Status> {
        let materialized = self
            .prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user,
                sessions_base,
                session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
        Ok(
            crate::stack_doc_attachments::prompt_with_attached_changeset(
                req.initial_prompt.trim(),
                &materialized,
            ),
        )
    }

    /// Where a new CLI-agent session lives, the id it is given, and its first prompt.
    pub(super) async fn cli_start_prelude(
        &self,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
        os_user: &str,
    ) -> Result<CliStart, Status> {
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_id = Uuid::now_v7().to_string();
        let initial_prompt = self
            .attached_initial_prompt(req, os_user, &sessions_base, &session_id, progress)
            .await?;
        Ok(CliStart {
            sessions_base,
            session_id,
            initial_prompt,
        })
    }

    pub(super) async fn start_sandboxed_cursor_cli_from_request(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_sandboxed_cursor_cli_session(
            os_user,
            &start.session_id,
            &req.session_token,
            start.sessions_base,
            req.model.trim(),
            req.project_id.trim(),
            req.branch_worktree_intent.trim(),
            req.new_branch_name.trim(),
            req.selected_integration_base_ref.trim(),
            req.selected_branch_to_work_on.trim(),
            Some(req.stack_parent.trim()).filter(|s| !s.is_empty()),
            req.stack_parent_daemon_instance_id.trim(),
            req.stack_node_id.trim(),
            &start.initial_prompt,
            req.managed_codebase,
            &req.specialized_agents,
            managed_recipe,
            req.semantic_index,
            req.create_remote_branch,
        )
        .await
    }

    pub(super) async fn spawn_cursor_cli_from_request(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        mut started_agents: Vec<tddy_core::SessionAgentRecord>,
        clones: super::super::DaemonSeedCloneClaimant,
    ) -> Result<Response<StartSessionResponse>, Status> {
        crate::cursor_cli_spawn::spawn_cursor_cli_session_inner(
            &self.config,
            &self.tddy_data_dir,
            &self.claude_cli_manager,
            os_user,
            &start.session_id,
            &req.session_token,
            start.sessions_base,
            req.model.trim(),
            req.project_id.trim(),
            req.branch_worktree_intent.trim(),
            req.new_branch_name.trim(),
            req.selected_integration_base_ref.trim(),
            req.selected_branch_to_work_on.trim(),
            req.repo_path.trim(),
            match Some(req.stack_parent.trim()).filter(|s| !s.is_empty()) {
                Some(session_id) => stack_parent::SpawnStackParent::OwnedBy {
                    session_id,
                    daemon_instance_id: req.stack_parent_daemon_instance_id.trim(),
                    stack_node_id: req.stack_node_id.trim(),
                    session_token: &req.session_token,
                    host: self,
                },
                None => stack_parent::SpawnStackParent::NoParent,
            },
            &start.initial_prompt,
            req.managed_codebase,
            &mut started_agents,
            managed_recipe,
            req.semantic_index,
            req.create_remote_branch,
            &self.task_registry,
            &clones,
        )
        .await
    }
}
