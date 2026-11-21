//! A resumed generic launch's wiring (split out of `cli_agent_steps.rs`): [`ResumedWiring`] and
//! [`LaunchSessions::resumed_session_wiring`], which gives a resumed agent again what a start gave it
//! — none of it is in a persisted argv.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_coding_agent_specs::{Capability, CodingAgentSpec};
use tddy_core::{HookCommandParams, SessionMetadata};
use tddy_rpc::Status;

use tddy_cli_sessions::session_toolcall::ManagedWorkflow;
use tddy_core::session_pointer::SessionPointer;

use super::{carry_on_the_tool_server, route_tool_server_env, session_tool_env};
use crate::back_channel::peer_name_args;
use crate::claude_cli_spawn_steps::{resumed_hooks, ClaudeCliHooks};
use crate::cli_session_pointer::session_pointer_of;
use crate::launch_ports::LaunchSessions;
use crate::project_home_launch::spec_role_launch;
use crate::spec_launch::{AgentToolset, HostAgent};

/// What a resumed generic launch re-wires: a main's tool restriction, the managed workflow resumed
/// at the goal it persisted, and the operator-notify wiring a session recorded a hook token for. The
/// hooks file its start wrote is kept as it is; argv hooks (Muse's `-c hooks=…`) are rendered again.
#[derive(Default)]
pub(crate) struct ResumedWiring {
    /// The session's hooks, rebuilt around its recorded token — for an argv adapter only.
    pub hooks: Option<ClaudeCliHooks>,
    pub workflow: Option<ManagedWorkflow>,
    pub system_prompt_file: Option<PathBuf>,
    /// Never a `TDDY_*` variable.
    pub env: Vec<(String, String)>,
    pub extra_args: Vec<String>,
    pub toolset: AgentToolset,
    /// The session pointer written again into the worktree, as at the start.
    pub session_pointer: Option<SessionPointer>,
}

impl LaunchSessions {
    /// The wiring a resume of `meta` gives `spec`'s agent again — none of it is in a persisted argv.
    /// `local_session` says the session is neither split nor aimed at another host: only such a
    /// session's agent is given the session tool transport.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn resumed_session_wiring(
        &self,
        agent: &HostAgent,
        meta: &SessionMetadata,
        session_dir: &Path,
        worktree: &Path,
        os_user: &str,
        session_token: &str,
        github_credential_handler: Option<Arc<dyn tddy_core::toolcall::GithubCredentialHandler>>,
        local_session: bool,
    ) -> Result<ResumedWiring, Status> {
        let spec = &agent.spec;
        let tddy_tools_path = crate::cli_harness_jail::agent_tddy_tools(agent)?;
        let session_id = meta.session_id.as_str();
        let tool_socket = self.agent_tool_socket_of(os_user).await?;
        let session_tool_token =
            self.resumed_session_tool_token(meta, session_token, local_session);
        let mut process_env =
            session_tool_env(&self.config, session_id, session_tool_token, &tool_socket);
        let mut tool_server_env = Vec::new();
        let mut notify_env = Vec::new();
        let mut system_prompt_file = None;
        let mut workflow = None;
        if spec.has(Capability::ManagedWorkflow) {
            if let Some(recipe_name) = crate::svc_resume_claude_cli_session::resumed_recipe(
                &self.cli_manager,
                meta,
                os_user,
                session_id,
            )
            .await?
            {
                let recipe =
                    tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(recipe_name)
                        .map_err(Status::invalid_argument)?;
                let resume_goal = LaunchSessions::managed_resume_goal(session_dir, &recipe);
                let launch = self.prepare_managed_workflow(
                    session_id,
                    recipe,
                    session_dir,
                    worktree,
                    session_dir,
                    &tddy_tools_path,
                    Some(resume_goal),
                    None,
                    github_credential_handler,
                )?;
                system_prompt_file = Some(launch.prompt_file);
                route_tool_server_env(launch.env, &mut process_env, &mut tool_server_env);
                workflow = Some(launch.workflow);
            }
        }
        let reports_to = self.resumed_reports_to(session_dir);
        let on_target = meta.target.is_some();
        if spec.has(Capability::OperatorNotify) {
            let back_channel = reports_to
                .as_ref()
                .and_then(|to| to.guidance_at(spec, on_target, meta.hook_token.as_deref()));
            let notify = crate::notify_env::resumed_agent_notify_launch(
                &self.config,
                &tool_socket,
                os_user,
                session_dir,
                meta,
                system_prompt_file.as_deref(),
                back_channel.as_deref(),
            )?;
            notify_env.extend(notify.env);
            system_prompt_file = notify.append_system_prompt_file.or(system_prompt_file);
        }
        let role = self.resumed_role(meta, session_id, session_dir, session_token, &tool_socket)?;
        route_tool_server_env(role.env, &mut process_env, &mut tool_server_env);
        let (mut extra_args, mut toolset) = spec_role_launch(role.main.as_ref());
        let role_name = meta.session_role.as_deref().unwrap_or_default();
        let reports = reports_to.is_some();
        extra_args.extend(peer_name_args(
            spec, session_id, on_target, reports, role_name,
        ));
        let session_pointer = session_pointer_of(&tool_server_env, notify_env, worktree);
        carry_on_the_tool_server(
            spec,
            &mut toolset,
            tool_server_env,
            &tddy_tools_path,
            session_dir,
        );
        let hooks = resumed_hooks(spec, &self.config, &tool_socket, &tddy_tools_path, meta)?;
        Ok(ResumedWiring {
            hooks,
            workflow,
            system_prompt_file,
            env: process_env,
            extra_args,
            toolset,
            session_pointer,
        })
    }
    /// The credential a resumed agent's session tool transport is minted around, minted now —
    /// nothing persists the previous process's token. A session whose agent runs on another host
    /// is given none: the socket and the URL name this host, which its shell cannot reach, and
    /// detection would prefer the dead route. A split agent's process runs model-authored code
    /// joined to its room over its own split transport — none of this.
    fn resumed_session_tool_token(
        &self,
        meta: &SessionMetadata,
        session_token: &str,
        local_session: bool,
    ) -> Option<String> {
        let runs_on_this_host = local_session
            && meta.target.is_none()
            && meta
                .ssh_config_host
                .as_deref()
                .is_none_or(|host| host.trim().is_empty());
        runs_on_this_host
            .then(|| self.session_tool_token_for(&meta.session_id, session_token))
            .flatten()
    }
}

impl ResumedWiring {
    /// The hook parameters of the resumed launch, when the session recorded a hook token.
    /// `route_env` is the resumed route's non-secret pairs, pinned beside the hooks as a start
    /// pins them: the resume rewrites the file that pin is in.
    pub(crate) fn hook_params<'a>(
        &'a self,
        spec: &CodingAgentSpec,
        session_id: &'a str,
        os_user: &'a str,
        route_env: &'a [(String, String)],
    ) -> Option<HookCommandParams<'a>> {
        let hooks = self
            .hooks
            .as_ref()
            .filter(|_| spec.has(Capability::Hooks))?;
        let mut params = hooks.params(session_id, os_user);
        params.route_env = route_env;
        Some(params)
    }
}
