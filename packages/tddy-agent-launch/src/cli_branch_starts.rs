use crate::stack_parent;

use uuid::Uuid;

use tddy_session_split::attached_initial_prompt::attached_initial_prompt;

use tddy_session_files::attachment_progress::AttachmentProgressSink;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use crate::svc_start_session_core::CliStart;

use crate::launch_ports::LaunchSessions;
use crate::session_provider_route::{refuse_route_in_sandbox, session_route_sources_with_default};
use tddy_core::cli_harness::CliHarness;
use tddy_service::proto::session::StartSessionRequest;

impl LaunchSessions {
    pub(super) async fn start_sandboxed_claude_cli_from_request(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        stack_parent_for_claude_cli: Option<&str>,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // The person's default counts here too: a sandbox cannot run on an account yet, and
        // silently starting on the vendor instead of the chosen default is a fallback.
        refuse_route_in_sandbox(&session_route_sources_with_default(
            req,
            "claude-cli",
            &self.host.session_account_access(&req.session_token),
        )?)?;
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
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let route_sources = session_route_sources_with_default(
            req,
            "claude-cli",
            &self.host.session_account_access(&req.session_token),
        )?;
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
            &route_sources,
            progress,
        )
        .await
    }

    /// Where a new CLI-agent session lives, the id it is given, and its first prompt.
    pub(super) async fn cli_start_prelude(
        &self,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
        os_user: &str,
    ) -> Result<CliStart, Status> {
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_id = Uuid::now_v7().to_string();
        crate::start_request_checks::record_requested_github_source(
            req,
            &tddy_core::session_lifecycle::unified_session_dir_path(&sessions_base, &session_id),
        )?;
        let initial_prompt = attached_initial_prompt(
            &self.attachment_state(),
            req,
            os_user,
            &sessions_base,
            &session_id,
            progress,
        )
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

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn spawn_cursor_cli_from_request(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        mut started_agents: Vec<tddy_core::SessionAgentRecord>,
        clones: tddy_session_agents::agent_host_callbacks::DaemonSeedCloneClaimant,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        crate::cursor_cli_spawn::spawn_cursor_cli_session_reporting(
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
            &self.host.session_account_access(&req.session_token),
            progress,
        )
        .await
    }
}

impl LaunchSessions {
    /// The start of a registered CLI harness that is cut a worktree of its own (`opencode-cli`,
    /// `muse-cli`): refused in a sandbox or over a managed codebase, otherwise the shared CLI
    /// prelude, the seeded roster resolved before the spawn, then the harness in a PTY.
    pub(super) async fn start_cli_harness(
        &self,
        harness: &'static CliHarness,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
        os_user: &str,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // TODO(provider-route-jail): these harnesses are not sandboxed yet (#provider-route 9/9).
        // Refused rather than run unsandboxed.
        if req.sandbox {
            return Err(Status::failed_precondition(format!(
                "{} sessions cannot run in a sandbox yet",
                harness.session_type
            )));
        }
        // None of them has a workflow of its own for a managed codebase to drive.
        if req.managed_codebase {
            return Err(Status::invalid_argument(format!(
                "managed_codebase is not supported for {} sessions",
                harness.session_type
            )));
        }
        let start = self.cli_start_prelude(req, progress, os_user).await?;
        let started_agents = self
            .agent_roster
            .seeded_roster_records(&req.specialized_agents)
            .await?;
        let clones = tddy_session_agents::agent_host_callbacks::DaemonSeedCloneClaimant {
            service: self.agent_roster.clone(),
        };
        let sessions_base = start.sessions_base.clone();
        let session_id = start.session_id.clone();
        let started = self
            .start_cli_harness_from_request(
                harness,
                req,
                os_user,
                start,
                started_agents,
                clones,
                progress,
            )
            .await?;
        self.announce_worktree_ready(&sessions_base, &session_id);
        Ok(started)
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_cli_harness_from_request(
        &self,
        harness: &'static CliHarness,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        mut started_agents: Vec<tddy_core::SessionAgentRecord>,
        clones: tddy_session_agents::agent_host_callbacks::DaemonSeedCloneClaimant,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let route_sources = session_route_sources_with_default(
            req,
            harness.session_type,
            &self.host.session_account_access(&req.session_token),
        )?;
        crate::opencode_cli_spawn::start_cli_harness_session(
            crate::opencode_cli_spawn::CliHarnessStart {
                harness,
                config: &self.config,
                tddy_data_dir: &self.tddy_data_dir,
                cli_manager: &self.claude_cli_manager,
                os_user,
                session_id: &start.session_id,
                session_token: &req.session_token,
                sessions_base: start.sessions_base,
                model: req.model.trim(),
                project_id: req.project_id.trim(),
                repo_path: req.repo_path.trim(),
                branch_worktree_intent: req.branch_worktree_intent.trim(),
                new_branch_name: req.new_branch_name.trim(),
                selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
                stack_parent: match Some(req.stack_parent.trim()).filter(|s| !s.is_empty()) {
                    Some(session_id) => stack_parent::SpawnStackParent::OwnedBy {
                        session_id,
                        daemon_instance_id: req.stack_parent_daemon_instance_id.trim(),
                        stack_node_id: req.stack_node_id.trim(),
                        session_token: &req.session_token,
                        host: self,
                    },
                    None => stack_parent::SpawnStackParent::NoParent,
                },
                initial_prompt: &start.initial_prompt,
                agents: &mut started_agents,
                semantic_index: req.semantic_index,
                create_remote_branch: req.create_remote_branch,
                task_registry: &self.task_registry,
                agent_clones: &clones,
                account_access: &self.host.session_account_access(&req.session_token),
                route_sources: &route_sources,
                progress,
            },
        )
        .await
    }
}
