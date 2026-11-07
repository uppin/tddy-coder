use crate::stack_parent;

use crate::harness_from_spec::child_session_type;

use uuid::Uuid;

use tddy_session_split::attached_initial_prompt::attached_initial_prompt;

use tddy_session_files::attachment_progress::AttachmentProgressSink;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::path::PathBuf;

use crate::svc_start_session_core::CliStart;

use crate::harness_from_spec::{resolve_session_agent, CliSessionAgent, CliSessionType};
use crate::launch_ports::LaunchSessions;
use crate::placement::managed_recipe_for;
use crate::session_provider_route::{session_route_sources_with_default, start_route, RouteAgent};
use crate::start_request_checks::{
    refuse_a_cli_start_without_a_model, refuse_an_unsupported_cli_session_start,
};
use tddy_service::proto::session::StartSessionRequest;
use tddy_session_split::split_start::resolve_caller_chosen_session_id;

impl LaunchSessions {
    /// The refusals the start path would answer an unsandboxed, co-located claude-cli `req` with
    /// before it creates anything — a missing model, a caller-chosen id, an unknown recipe, and a
    /// provider route the owner's vault cannot serve — checked by the same functions, starting and
    /// creating nothing. For a caller that must stop something before the start may begin
    /// (replacing a project's main), so a refusal leaves it unstopped. A spawn that fails after
    /// these pass is not caught here.
    pub(crate) fn refuse_a_claude_cli_start_up_front(
        &self,
        req: &StartSessionRequest,
    ) -> Result<(), Status> {
        refuse_a_cli_start_without_a_model(req, Some(&*self.coding_agents))?;
        resolve_caller_chosen_session_id(&req.requested_session_id, req.session_type.trim())?;
        managed_recipe_for(req)?;
        // The agent the session type names — `claude` for `claude-cli`, a dropped-in copy for its own.
        let session_type = req.session_type.trim();
        let agent_id = self
            .coding_agents
            .agents()
            .for_session_type(session_type)
            .map(|spec| spec.id.clone())
            .ok_or_else(|| {
                Status::failed_precondition(format!("no coding agent spec launches {session_type}"))
            })?;
        let access = self.host.session_account_access(&req.session_token);
        let sources = session_route_sources_with_default(req, session_type, &access)?;
        start_route(
            &access,
            RouteAgent::OnHost {
                agents: Some(&*self.coding_agents),
                id: &agent_id,
            },
            &sources,
            req.model.trim(),
        )?;
        Ok(())
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
}

impl LaunchSessions {
    /// The start of a CLI coding agent that is cut a worktree of its own (`opencode-cli`, `muse-cli`,
    /// `warp-cli`), launched from its spec: refused over a managed codebase or, when sandboxed, on a
    /// placement its spec excludes; its agent resolved on this host before anything is cut; then the
    /// shared CLI prelude, and the agent in its jail when the session is sandboxed or in a PTY when it
    /// is not.
    pub(super) async fn start_cli_harness(
        &self,
        kind: CliSessionType,
        req: &StartSessionRequest,
        progress: &AttachmentProgressSink,
        os_user: &str,
    ) -> Result<Response<StartSessionResponse>, Status> {
        // Resolved first, with no side effect: the managed-codebase refusal reads the agent's spec.
        let session_agent = resolve_session_agent(&self.coding_agents, kind.clone())?;
        refuse_an_unsupported_cli_session_start(&session_agent.agent.spec, kind.name, req)?;
        if req.sandbox {
            self.require_sandbox_placement(kind.name)?;
        }
        let start = self.cli_start_prelude(req, progress, os_user).await?;
        let sessions_base = start.sessions_base.clone();
        let session_id = start.session_id.clone();
        let started = if req.sandbox {
            self.start_sandboxed_cli_harness_from_request(
                &session_agent,
                req,
                os_user,
                start,
                progress,
            )
            .await?
        } else {
            self.start_cli_harness_from_request(&session_agent, req, os_user, start, progress)
                .await?
        };
        self.announce_worktree_ready(&sessions_base, &session_id);
        Ok(started)
    }

    /// The seeded roster, resolved before the spawn, then the agent's route and its start.
    async fn start_cli_harness_from_request(
        &self,
        session_agent: &CliSessionAgent,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let mut started_agents = self
            .agent_roster
            .seeded_roster_records_placed(&req.specialized_agents, &req.clone_placed_agents)
            .await?;
        let clones = tddy_session_agents::agent_host_callbacks::DaemonSeedCloneClaimant {
            service: self.agent_roster.clone(),
        };
        let agent_tool_socket = self.agent_tool_socket_of(os_user).await?;
        let account_access = self.host.session_account_access(&req.session_token);
        let route_sources =
            session_route_sources_with_default(req, session_agent.kind.name, &account_access)?;
        // A managed codebase driven by a recipe: admitted only for a spec declaring
        // `managed_workflow` (`refuse_an_unsupported_cli_session_start`). An unknown recipe is a
        // request error, never silently ignored.
        let managed_recipe = managed_recipe_for(req)?;
        let (child_spawn_handler, conversation_spawn_handler) = self.managed_spawn_handlers(
            &managed_recipe,
            os_user,
            &start.session_id,
            req.project_id.trim(),
            &start.sessions_base,
            &req.session_token,
        );
        let managed = managed_recipe.map(|recipe| crate::cli_agent_steps::ManagedWiring {
            recipe,
            child_spawn_handler,
            conversation_spawn_handler,
        });
        let role = self.start_role(
            req.session_role.trim(),
            &start.session_id,
            &start.sessions_base,
            &req.session_token,
            &agent_tool_socket,
        )?;
        crate::opencode_cli_spawn::start_cli_harness_session(
            crate::opencode_cli_spawn::CliSessionStart {
                kind: session_agent.kind.clone(),
                agent: &session_agent.agent,
                config: &self.config,
                tddy_data_dir: &self.tddy_data_dir,
                agent_tool_socket: &agent_tool_socket,
                cli_manager: &self.cli_manager,
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
                account_access: &account_access,
                route_sources: &route_sources,
                progress,
                run_in_tmux: req.run_in_tmux,
                permission_mode: req.permission_mode.trim(),
                dangerously_skip_permissions: req.dangerously_skip_permissions,
                ssh_config_host: req.ssh_config_host.trim(),
                managed_codebase: req.managed_codebase,
                managed,
                role,
            },
        )
        .await
    }
}

/// What a conversation or pr-stack child is started from. The request that spawns one names no
/// agent: it runs its parent session's own ([`child_session_type`]).
pub(crate) struct SpawnedChild<'a> {
    pub os_user: &'a str,
    pub session_id: &'a str,
    pub sessions_base: PathBuf,
    pub model: &'a str,
    pub project_id: &'a str,
    pub new_branch_name: &'a str,
    pub selected_integration_base_ref: &'a str,
    pub initial_prompt: &'a str,
    /// The orchestrator the child descends from.
    pub stack_parent: stack_parent::SpawnStackParent<'a>,
    /// The orchestrator's recorded session type, whose agent the child runs.
    pub parent_session_type: Option<&'a str>,
    /// The owner's vault the child resolves its project's account over.
    pub account_access: &'a crate::session_acting_identity::SessionAccountAccess,
}

impl LaunchSessions {
    /// Start a conversation or pr-stack child through the one CLI launch, as its parent's agent: a
    /// host without coding agents, a parent that records no session type and a parent that is no
    /// CLI agent's refuse it `FAILED_PRECONDITION` before anything is cut.
    pub(crate) async fn start_spawned_cli_child(
        &self,
        child: SpawnedChild<'_>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let kind = child_session_type(Some(&*self.coding_agents), child.parent_session_type)?;
        let session_agent = resolve_session_agent(&self.coding_agents, kind)?;
        let mut started_agents = Vec::new();
        let clones = tddy_session_agents::agent_host_callbacks::DaemonSeedCloneClaimant {
            service: self.agent_roster.clone(),
        };
        let agent_tool_socket = self.agent_tool_socket_of(child.os_user).await?;
        // A child runs on its vendor: the orchestrator's account is not inherited.
        let route_sources = tddy_core::provider_route::RouteSources::default();
        crate::opencode_cli_spawn::start_cli_harness_session(
            crate::opencode_cli_spawn::CliSessionStart {
                kind: session_agent.kind.clone(),
                agent: &session_agent.agent,
                config: &self.config,
                tddy_data_dir: &self.tddy_data_dir,
                agent_tool_socket: &agent_tool_socket,
                cli_manager: &self.cli_manager,
                os_user: child.os_user,
                session_id: child.session_id,
                // The orchestrator is a session of this daemon: nothing leaves the host, so no
                // peer is asked with a token.
                session_token: "",
                sessions_base: child.sessions_base,
                model: child.model,
                project_id: child.project_id,
                // A child cuts its own worktree.
                repo_path: "",
                branch_worktree_intent: "new_branch_from_base",
                new_branch_name: child.new_branch_name,
                selected_integration_base_ref: child.selected_integration_base_ref,
                selected_branch_to_work_on: "",
                stack_parent: child.stack_parent,
                initial_prompt: child.initial_prompt,
                agents: &mut started_agents,
                // A spawned child never runs its own semantic index, nor pushes a remote branch.
                semantic_index: false,
                create_remote_branch: false,
                task_registry: &self.task_registry,
                agent_clones: &clones,
                account_access: child.account_access,
                route_sources: &route_sources,
                // Nobody watches a spawned child's start: the orchestrator is told its id.
                progress: &AttachmentProgressSink::discarding(),
                // A spawned child's agent is the PTY child itself, never run in tmux.
                run_in_tmux: false,
                permission_mode: "",
                dangerously_skip_permissions: false,
                ssh_config_host: "",
                // A spawned child is never a managed-codebase session, and has no standing role.
                managed_codebase: false,
                managed: None,
                role: crate::project_home_launch::StartRole::ordinary(),
            },
        )
        .await
    }
}
