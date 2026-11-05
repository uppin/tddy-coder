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
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_agents::seed_codebase::{SeedCodebase, SeededAgentClones};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::branch_intent::{BranchIntentPolicy, BranchIntentRequest};

use crate::cli_agent_steps::{start_conversation, ConversationStart};
use crate::cli_harness_jail::{jail_route_config_env, require_jail_route, CliJail};
use crate::cli_route_launch::routed_launch_model;
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{harness_template_params, CliSessionAgent, HarnessLaunch};
use crate::launch_ports::LaunchSessions;
use crate::relaunch_jail_dirs::{prepare_relaunch_dirs, refresh_relaunch_context_dir};
use crate::session_provider_route::{sandboxed_start_route, session_route_sources_with_default};
use crate::stack_parent::SpawnStackParent;
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};
use crate::svc_start_session_core::CliStart;

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
            BranchIntentPolicy::cli_agent(
                kind.name,
                session_agent
                    .agent
                    .spec
                    .has(tddy_coding_agent_specs::Capability::BranchNameRequired),
            ),
            None,
            &session_dir,
            stack_parent.session_id(),
            None,
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
        // The jail does not yet put the tddy-tools tools in front of the agent's own tools, so a
        // withdrawal is not enforced here.
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
                    false,
                ),
                session_token,
                &mut started_agents,
            )
            .await?;

        let (sandbox_root, egress_dir, scratch_dir, scratch_tmp, context_dir) =
            prepare_relaunch_dirs(&session_dir)?;
        refresh_relaunch_context_dir(&worktree_path, &started_agents, &context_dir, kind.name)?;

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
            &scratch_dir,
        ));

        // The jailed agent's commits carry the project's account identity, carried by the relay that
        // runs under this daemon's environment rather than the agent's.
        let identity = self
            .host
            .session_identity(os_user, session_id, project_id, session_token);
        let (session_env, _github_credential) = self.jail_relay_identity(identity, os_user)?;

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
            worktree: &worktree_path,
            env: &[],
            session_id,
        })
        .await?;
        let launcher = node_launcher_of(&session_agent.agent.tool.binary);
        let params = harness_template_params(&conversation.bound(HarnessLaunch {
            model: launch_model,
            prompt: Some(&start.initial_prompt),
            permission_mode: Some(req.permission_mode.trim()),
            dangerously_skip_permissions: req.dangerously_skip_permissions,
            node_launcher: launcher.as_ref(),
            ..HarnessLaunch::default()
        }));

        progress.begin_phase(StartStep::Agent);
        let jail = JailSession {
            session_id,
            project_id,
            session_token,
            session_dir: &session_dir,
            worktree_path: &worktree_path,
        };
        let dirs = JailDirs {
            sandbox_root,
            egress_dir,
            scratch_dir,
            scratch_tmp,
            context_dir,
        };
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
            })
            .await?;
        progress.end_phase(StartStep::Agent);

        let meta = SessionMetadata {
            repo_path: Some(worktree_path.to_string_lossy().to_string()),
            pid: Some(pid),
            model: Some(req.model.trim().to_string()),
            sandbox: Some(true),
            cursor_chat_id: conversation.recorded_chat_id().map(str::to_string),
            agents_rev: tddy_session_agents::agent_roster::started_roster_rev(&started_agents),
            agents: started_agents,
            ..tddy_session_split::service_util::starting_session_metadata(
                session_id, project_id, kind.name,
            )
        };
        write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
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
