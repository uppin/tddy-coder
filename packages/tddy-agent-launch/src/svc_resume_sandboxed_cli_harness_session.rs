//! Resume of a sandboxed CLI coding agent session (`opencode-cli`, `muse-cli`): its jail is relaunched
//! from its spec through [`LaunchSessions::launch_jailed`], rendered with `resume` (the agent continues
//! its worktree's session with `--continue`), on the provider route its start recorded and re-resolved
//! from the owner's vault as it stands now. A sandboxed session never resumes on the host's PTY.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::provider_route::HarnessRoute;
use tddy_core::{write_session_metadata, SessionAgentRecord, SessionMetadata};
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::{ResumeSessionRequest, ResumeSessionResponse};
use tddy_session_agents::agent_roster;

use crate::cli_agent_steps::{resumed_or_adopted_conversation, ConversationStart};
use crate::cli_harness_jail::{
    agent_tddy_tools, jail_route_config_env, require_jail_route, CliJail, JailWiring,
};
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{
    harness_template_params, resolve_session_agent, CliSessionAgent, CliSessionType, HarnessLaunch,
};
use crate::launch_ports::LaunchSessions;
use crate::relaunch_jail_dirs::{prepare_relaunch_dirs, refresh_relaunch_context_dir};
use crate::session_acting_identity::SessionIdentity;
use crate::session_provider_route::resume_route;
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};

/// What relaunching a sandboxed CLI session's jail is built from: the session, and the route and
/// identity its resume re-resolved.
struct CliHarnessRelaunch<'a> {
    session_agent: &'a CliSessionAgent,
    session_id: &'a str,
    project_id: &'a str,
    session_token: &'a str,
    session_dir: &'a Path,
    worktree_path: &'a Path,
    agents: &'a [SessionAgentRecord],
    /// The provider route the resume re-resolved; `None` for the agent's own setup.
    route: Option<HarnessRoute>,
    /// The project's account identity the relayed host-side commands run under, resolved from the
    /// session's own start token.
    identity: SessionIdentity,
    os_user: &'a str,
    /// The recipe the session's record names: its managed workflow is re-wired, resuming at the
    /// goal persisted in `changeset.yaml`.
    managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
    /// The record the start wrote: the conversation the resume continues, and its recorded model.
    metadata: &'a SessionMetadata,
}

impl LaunchSessions {
    /// Handle `ResumeSession` for a sandboxed CLI coding agent session.
    pub(crate) async fn resume_sandboxed_cli_harness(
        &self,
        kind: CliSessionType,
        os_user: &str,
        req: &ResumeSessionRequest,
        session_dir: &Path,
        metadata: SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        // Resolved before the running jail is stopped: a resume the host cannot launch leaves it running.
        let session_agent = resolve_session_agent(&self.coding_agents, kind.clone())?;
        if let Some(state) = self.sandbox_manager.remove(&req.session_id).await {
            state.stop();
        } else if let Some(pid) = metadata.pid {
            tddy_daemon_sandbox::sandbox_session::terminate_sandbox_process(pid);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;

        let worktree_path = metadata
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| Status::internal("sandbox session missing repo_path in metadata"))?;
        // The account the start recorded, re-resolved from the owner's vault as it stands now: a
        // removed account refuses the resume rather than running it on the vendor.
        let route = require_jail_route(
            kind.name,
            &session_agent.agent.spec,
            resume_route(
                &self.host.session_account_access(&req.session_token),
                (&kind.route).into(),
                session_dir,
            )?,
        )?;
        let identity = self.host.session_identity(
            os_user,
            &req.session_id,
            &metadata.project_id,
            &req.session_token,
        );
        // A recipe in the record marks a managed session; its workflow is re-wired on resume.
        let managed_recipe = match metadata
            .recipe
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            Some(name) => Some(
                tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(name)
                    .map_err(Status::invalid_argument)?,
            ),
            None => None,
        };

        let (pid, adopted_chat) = self
            .relaunch_sandboxed_cli_harness(CliHarnessRelaunch {
                session_agent: &session_agent,
                session_id: &req.session_id,
                project_id: &metadata.project_id,
                session_token: &req.session_token,
                session_dir,
                worktree_path: &worktree_path,
                agents: &metadata.agents,
                route,
                identity,
                os_user,
                managed_recipe,
                metadata: &metadata,
            })
            .await?;

        let updated = SessionMetadata {
            updated_at: chrono::Utc::now().to_rfc3339(),
            status: "active".to_string(),
            pid: Some(pid),
            cursor_chat_id: adopted_chat.or(metadata.cursor_chat_id.clone()),
            ..metadata
        };
        write_session_metadata(session_dir, &updated)
            .map_err(|e| Status::internal(format!("failed to update session metadata: {e}")))?;

        Ok(Response::new(ResumeSessionResponse {
            session_id: req.session_id.clone(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
        }))
    }

    /// Launch the agent in resume mode in the session's jail and return the runner's pid.
    async fn relaunch_sandboxed_cli_harness(
        &self,
        relaunch: CliHarnessRelaunch<'_>,
    ) -> Result<(u32, Option<String>), Status> {
        let CliHarnessRelaunch {
            session_agent,
            session_id,
            project_id,
            session_token,
            session_dir,
            worktree_path,
            agents,
            route,
            identity,
            os_user,
            managed_recipe,
            metadata,
        } = relaunch;

        // The readiness gate a start applies: a resumed subagent is only as usable as a fresh one if
        // its endpoint is awake before the jail comes back up. No fallback — the jail is not relaunched
        // when warm-up fails.
        let specialized_defs = self
            .agent_roster
            .resolve_specialized_agent_defs(&agent_roster::roster_agent_ids(agents))
            .await?;
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let (sandbox_root, egress_dir, scratch_dir, scratch_tmp, context_dir) =
            prepare_relaunch_dirs(session_dir)?;
        refresh_relaunch_context_dir(worktree_path, agents, &context_dir, session_agent.kind.name)?;

        // Re-wire the managed workflow of a managed session: the controller resumes at the goal
        // persisted in changeset.yaml, its prompt goes into the jail-visible context directory, and
        // the host-side env the relayed `tddy-tools transition` runs with carries `TDDY_SOCKET`.
        let tddy_tools_path = agent_tddy_tools(&session_agent.agent)?;
        let (managed, system_prompt_file, session_env) = self.relaunch_managed_workflow(
            session_id,
            session_dir,
            worktree_path,
            managed_recipe,
            &context_dir,
            &tddy_tools_path,
            os_user,
            identity,
        )?;

        // The daemon's environment for the session, as its start gave it — but for the semantic index,
        // which a resume does not rebuild: `SemanticSearch` stays withdrawn.
        let mut env: BTreeMap<String, String> = BTreeMap::new();
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(worktree_path));
        env.extend(self.restructure_tools_env());
        env.extend(jail_route_config_env(
            &session_agent.agent.spec,
            route.as_ref(),
            &scratch_dir,
        ));

        // A resume continues the agent's own conversation (the spec's `resume` branch): its session
        // id or minted chat, which re-binds the recorded model, or its worktree. The route's
        // arguments belong to the start.
        let conversation = resumed_or_adopted_conversation(
            ConversationStart {
                spec: &session_agent.agent.spec,
                binary: &session_agent.agent.tool.binary,
                worktree: worktree_path,
                env: &[],
                session_id,
            },
            metadata,
        )
        .await?;
        let launcher = node_launcher_of(&session_agent.agent.tool.binary);
        let params = harness_template_params(&conversation.bound(HarnessLaunch {
            model: metadata.model.as_deref().unwrap_or_default(),
            resume: true,
            node_launcher: launcher.as_ref(),
            ..HarnessLaunch::default()
        }));
        let jail = JailSession {
            session_id,
            project_id,
            session_token,
            session_dir,
            worktree_path,
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
                route_args: Vec::new(),
                env,
                session_env,
                route,
                wiring: JailWiring {
                    agents,
                    managed,
                    system_prompt_file,
                    managed_codebase: false,
                    passthrough_args: Vec::new(),
                    role: "",
                },
            })
            .await?;
        Ok((pid, conversation.recorded_chat_id().map(str::to_string)))
    }
}
