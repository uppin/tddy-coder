//! Sandboxed start of a CLI coding agent that is cut a worktree of its own — OpenCode (`opencode-cli`)
//! and Muse Code (`muse-cli`) — launched in a jail from its spec through
//! [`LaunchSessions::launch_jailed`] (`#agent-tools 8/12`): its `run:` template renders its argv for
//! the jail, its `route` section lowers the provider route's files and configuration variable, and its
//! `placements` decide whether it may run in a jail at all (refused by the caller before anything is cut).
//!
//! The same shape as a sandboxed Cursor session — a daemon-cut worktree, a jail whose runner relays the
//! agent's PTY over the daemon's own connection. A resume of such a session relaunches its jail with
//! `resume` (see `svc_resume_sandboxed_cli_harness_session`), never the host PTY.

use std::collections::BTreeMap;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::provider_route::HarnessRoute;
use tddy_core::HookCommandParams;
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_agents::seed_codebase::{SeedCodebase, SeededAgentClones};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::branch_intent::BranchIntentRequest;

use crate::cli_agent_steps::conversation::refuse_a_role_the_agent_cannot_hold;
use crate::cli_agent_steps::conversation::start_conversation;
use crate::cli_agent_steps::conversation::ConversationStart;
use crate::cli_agent_steps::hooks_report_over_the_agent_tool_socket;
use crate::cli_harness_jail::{
    agent_tddy_tools, jail_route_config_env, prompt_ahead_of_slots, require_jail_route, CliJail,
    JailWiring, TDDY_TOOLS_ID,
};
use crate::cli_route_launch::routed_launch_model;
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{harness_template_params, CliSessionAgent, HarnessLaunch};
use crate::jail_launch::tool_ipc_socket;
use crate::jail_session_files::{
    prepare_jail_context_dir, prepare_jail_dirs, write_jail_session_metadata, JailRecord,
};
use crate::launch_ports::LaunchSessions;
use crate::placement::managed_recipe_for;
use crate::session_provider_route::{sandboxed_start_route, session_route_sources_with_default};
use crate::spec_launch::HostAgent;
use crate::stack_parent::SpawnStackParent;
use crate::svc_start_sandboxed_claude_cli_session::JailSession;
use crate::svc_start_session_core::CliStart;
use crate::worktree_source::sandbox_claude_passthrough_args;
use std::path::Path;
use tddy_daemon_kernel::config::DaemonConfig;

impl LaunchSessions {
    /// The sandboxed start of a CLI coding agent: its provider route resolved before anything is
    /// created, refused when its spec's jail needs one and there is none, then the start in its jail.
    pub(crate) async fn start_sandboxed_cli_harness_from_request(
        &self,
        session_agent: &CliSessionAgent,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let kind = &session_agent.kind;
        let access = self.host.session_account_access(&req.session_token);
        let sources = session_route_sources_with_default(req, kind.name, &access)?;
        let session_dir = start
            .sessions_base
            .join(SESSIONS_SUBDIR)
            .join(&start.session_id);
        let route = require_jail_route(
            kind.name,
            &session_agent.agent.spec,
            sandboxed_start_route(
                &access,
                (&kind.route).into(),
                &sources,
                req.model.trim(),
                &session_dir,
            )?,
        )?;
        self.start_sandboxed_cli_harness_session(
            session_agent,
            req,
            os_user,
            start,
            route,
            progress,
        )
        .await
    }

    async fn start_sandboxed_cli_harness_session(
        &self,
        session_agent: &CliSessionAgent,
        req: &StartSessionRequest,
        os_user: &str,
        start: CliStart,
        route: Option<HarnessRoute>,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let kind = &session_agent.kind;
        let session_id = start.session_id.as_str();
        let session_token = req.session_token.as_str();
        let project_id = req.project_id.trim();
        let session_role = req.session_role.trim();
        refuse_a_role_the_agent_cannot_hold(&session_agent.agent.spec, session_role)?;
        // A managed codebase driven by a recipe: admitted only for a spec declaring
        // `managed_workflow` (`refuse_an_unsupported_cli_session_start`). An unknown recipe is a
        // request error, never silently ignored.
        let managed_recipe = managed_recipe_for(req)?;
        let session_dir = start.sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;

        // The roster and its endpoints are resolved and warmed before anything is cut or spawned, as
        // a sandboxed claude or cursor start does: an unreachable subagent fails the start here.
        let mut started_agents = self
            .agent_roster
            .seeded_roster_records_placed(&req.specialized_agents, &req.clone_placed_agents)
            .await?;
        let specialized_defs = self
            .agent_roster
            .resolve_specialized_agent_defs(&req.specialized_agents)
            .await?;
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let stack_parent = match Some(req.stack_parent.trim()).filter(|s| !s.is_empty()) {
            Some(parent) => SpawnStackParent::OwnedBy {
                session_id: parent,
                daemon_instance_id: req.stack_parent_daemon_instance_id.trim(),
                stack_node_id: req.stack_node_id.trim(),
                session_token,
                host: self,
            },
            None => SpawnStackParent::NoParent,
        };
        let intent = tddy_session_split::service_util::write_initial_changeset(
            session_id,
            &BranchIntentRequest {
                branch_worktree_intent: req.branch_worktree_intent.trim(),
                new_branch_name: req.new_branch_name.trim(),
                selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
            },
            crate::harness_from_spec::cli_branch_intent(kind.clone(), &session_agent.agent.spec),
            None,
            &session_dir,
            stack_parent.session_id(),
            managed_recipe.as_deref(),
        )?;
        let CliWorktree {
            path: worktree_path,
            ..
        } = provision_cli_worktree(
            &CliWorktreeStart {
                session_type: kind.name,
                cut_label: "start_sandboxed_cli_harness_session: create worktree",
                session_id,
                session_dir: &session_dir,
                sessions_base: &start.sessions_base,
                tddy_data_dir: &self.tddy_data_dir,
                os_user,
                project_id,
                repo_path: req.repo_path.trim(),
                branch_worktree_intent: req.branch_worktree_intent.trim(),
                new_branch_name: req.new_branch_name.trim(),
                selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
                create_remote_branch: req.create_remote_branch,
                intent,
                timeout: self.config.spawn_worker_request_timeout(),
                ssh_alias: "",
            },
            &stack_parent,
            progress,
        )
        .await?;

        // The clones this session's seeded agents read, claimed now: the worktree they mirror exists.
        // A jail that registers `tddy-tools` for its agent (its spec's `mcp` slot) is what puts
        // `mcp__tddy-tools__*` in front of the main agent's file tools, which is what makes a
        // withdrawal enforceable (`session_enforces_a_withdrawal`).
        let clones = tddy_session_agents::agent_host_callbacks::DaemonSeedCloneClaimant {
            service: self.agent_roster.clone(),
        };
        let seeded_clones = clones
            .claim_for_seed(
                session_id,
                &SeedCodebase::of_a_starting_session(
                    session_dir.clone(),
                    worktree_path.clone(),
                    project_id,
                    !matches!(
                        session_agent.agent.spec.agent.mcp,
                        tddy_coding_agent_specs::McpSlot::Unsupported
                    ),
                ),
                session_token,
                &mut started_agents,
            )
            .await?;

        // The agent's hooks reach the daemon over the session's tool-IPC socket, the one socket a
        // jail can reach: written into the worktree now, because the jail reads a copy of it taken
        // after this.
        let hook_token = write_jail_hooks(
            &session_agent.agent,
            &self.config,
            &worktree_path,
            &session_dir,
            session_id,
            os_user,
        )?;
        let dirs = prepare_jail_dirs(&session_dir)?;
        prepare_jail_context_dir(
            &tddy_session_files::context_files::context_globs(
                self.coding_agents
                    .agents()
                    .get(kind.name)
                    .map(|spec| spec.agent.context_globs.as_slice()),
            ),
            self.coding_agents
                .agents()
                .get(kind.name)
                .map(|spec| spec.agent.instructions.files.as_slice())
                .unwrap_or(&[]),
            &started_agents,
            &worktree_path,
            &dirs.context_dir,
        )?;
        let jail = JailSession {
            session_id,
            project_id,
            session_token,
            session_dir: &session_dir,
            worktree_path: &worktree_path,
        };
        // Managed workflow: the per-session controller and toolcall listener, the recipe's prompt in
        // the jail-visible context directory, and the host-side env the relayed `tddy-tools
        // transition` runs with. The relay's commit identity belongs there too, managed or not.
        let tddy_tools_path = agent_tddy_tools(&session_agent.agent)?;
        let (managed, system_prompt_file, session_env) = self.managed_jail_env(
            &jail,
            os_user,
            &start.sessions_base,
            &managed_recipe,
            &dirs.context_dir,
            &tddy_tools_path,
        )?;

        let semantic_env = semantic_index_env(
            &self.tddy_data_dir,
            session_id,
            req.semantic_index,
            &self.task_registry,
            &session_dir,
            &worktree_path,
            progress,
        )
        .await?;
        let mut env: BTreeMap<String, String> = BTreeMap::new();
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(&worktree_path));
        env.extend(self.restructure_tools_env());
        env.extend(semantic_env);
        env.extend(jail_route_config_env(
            &session_agent.agent.spec,
            route.as_ref(),
            &dirs.scratch_dir,
        ));

        // A start is launched with the spec's model and first prompt, as on the host; a route that
        // names the model itself (OpenCode's configuration file) leaves it to the route's arguments.
        let route_args = route
            .as_ref()
            .map(|route| route.args.clone())
            .unwrap_or_default();
        let launch_model = routed_launch_model(
            &session_agent.agent.spec,
            route.as_ref().map(|route| route.args.as_slice()),
            req.model.trim(),
        );
        // The conversation the spec declares: the session id, or a chat minted on the host (the jail
        // holds no credentials to mint it with), bound with the permission mode the start chose.
        let conversation = start_conversation(ConversationStart {
            spec: &session_agent.agent.spec,
            binary: &session_agent.agent.tool.binary,
            env_base: &session_agent.agent.env_base,
            worktree: &worktree_path,
            env: &[],
            session_id,
        })
        .await?;
        let launcher = node_launcher_of(&session_agent.agent.tool.binary);
        // The client's pass-through arguments go ahead of the slot flags, and with them the first
        // prompt when the spec's MCP flag would swallow it as a trailing positional.
        let prompt_ahead = prompt_ahead_of_slots(&session_agent.agent.spec);
        let passthrough_args = sandbox_claude_passthrough_args(
            &req.claude_args,
            if prompt_ahead {
                &start.initial_prompt
            } else {
                ""
            },
        );
        let params = harness_template_params(&conversation.bound(HarnessLaunch {
            model: launch_model,
            prompt: Some(start.initial_prompt.as_str()).filter(|_| !prompt_ahead),
            permission_mode: Some(req.permission_mode.trim()),
            dangerously_skip_permissions: req.dangerously_skip_permissions,
            node_launcher: launcher.as_ref(),
            ..HarnessLaunch::default()
        }));

        progress.begin_phase(StartStep::Agent);
        let pid = self
            .launch_jailed_cli_agent(CliJail {
                session_agent,
                jail: &jail,
                dirs: &dirs,
                params,
                route_args,
                env,
                session_env,
                route,
                wiring: JailWiring {
                    agents: &started_agents,
                    managed,
                    system_prompt_file,
                    managed_codebase: req.managed_codebase,
                    passthrough_args,
                    role: session_role,
                },
            })
            .await?;
        progress.end_phase(StartStep::Agent);

        write_jail_session_metadata(
            &jail,
            JailRecord {
                session_type: kind.name,
                model: req.model.trim(),
                managed_recipe,
                started_agents,
                pid,
                managed_codebase: req.managed_codebase,
                session_role,
                chat_id: conversation.recorded_chat_id(),
                hook_token,
            },
        )?;
        // The roster naming them is on disk now, so the clones belong to the session.
        seeded_clones.keep();

        log::info!(
            target: "tddy_daemon::connection_service",
            "started sandboxed {} session {session_id} pid={pid} worktree={} user={os_user}",
            kind.name,
            worktree_path.display()
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
            branch_conflict: None,
        }))
    }
}

/// The agent's hooks written into `worktree`, each event reporting over the session's tool-IPC
/// socket, when its spec declares `hooks` and its adapter reports over a socket — a jail has no other
/// way to reach the daemon. Returns the per-session token they report with, to be recorded.
fn write_jail_hooks(
    agent: &HostAgent,
    config: &DaemonConfig,
    worktree: &Path,
    session_dir: &Path,
    session_id: &str,
    os_user: &str,
) -> Result<Option<String>, Status> {
    let spec = &agent.spec;
    let tddy_tools = agent.dep(TDDY_TOOLS_ID).ok_or_else(|| {
        Status::failed_precondition(format!(
            "the {} tool does not depend on {TDDY_TOOLS_ID}",
            spec.id
        ))
    })?;
    let tddy_tools_path = tddy_tools.binary.to_string_lossy().into_owned();
    if !spec.has(tddy_coding_agent_specs::Capability::Hooks)
        || !hooks_report_over_the_agent_tool_socket(spec)
    {
        return Ok(None);
    }
    let hook_token = uuid::Uuid::new_v4().to_string();
    let daemon_socket = tool_ipc_socket(session_id).to_string_lossy().into_owned();
    let daemon_url = tddy_daemon_kernel::daemon_hook_urls::advertise_daemon_url(config);
    let params = HookCommandParams {
        tddy_tools_path: &tddy_tools_path,
        daemon_url: &daemon_url,
        session_id,
        os_user,
        hook_token: &hook_token,
        daemon_socket: Some(&daemon_socket),
        answer_wait_secs: spec.answer_wait_secs(),
    };
    crate::claude_cli_spawn_steps::record_answer_wait_secs(session_dir, spec.answer_wait_secs())?;
    let adapter = tddy_coding_agent_specs::slots::hooks::adapter(&spec.agent.hooks.adapter)
        .map_err(|e| Status::internal(e.to_string()))?;
    let (path, bytes) = adapter
        .try_render(worktree, &params)
        .map_err(|e| Status::internal(e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Status::internal(format!("create {}: {e}", parent.display())))?;
    }
    tddy_core::atomic_file::write_atomic(&path, bytes)
        .map_err(|e| Status::internal(format!("write {}: {e}", path.display())))?;
    Ok(Some(hook_token))
}
