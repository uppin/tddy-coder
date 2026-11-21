//! The steps the generic CLI spec launch (`opencode_cli_spawn::start_cli_harness_session` and its
//! resume) gains so that an agent with Claude Code's or Cursor's needs launches through it
//! (`#agent-tools 14/19`). Each step is keyed on a spec slot or capability, never on an agent id:
//!
//! - [`HostLaunchSlots`] / [`render_host_launch_slots`]: the `hooks` slot when the spec declares
//!   `hooks`, the MCP and tool-list slots from the role's [`AgentToolset`], the appended system
//!   prompt (a managed workflow's recipe prompt, a main's prompt) and `{home}`.
//! - [`write_slot_files_on_target`]: an ssh session's slot files written on its target
//!   (through the remote-shell port, bounded by its deadline), never at that path here.
//! - [`start_conversation`] / [`resumed_conversation`]: `coding_agent.conversation` — the daemon's
//!   session id, a minted id (bounded by the slot's mint deadline), the worktree, or a resume token.
//! - [`refuse_a_role_the_agent_cannot_hold`]: a project-home main only for a spec declaring
//!   `project_home_main`; its pre-approved tools are `project_home_launch::spec_role_launch`'s.
//!
//! Feature docs: `docs/ft/daemon/claude-cli-session.md`, `docs/ft/daemon/cursor-cli-session.md`.
//!
//! - [`wire_session_agent`] / [`LaunchSessions::resumed_session_wiring`]: the hooks, the managed
//!   workflow (`Capability::ManagedWorkflow`), the operator-notify guidance
//!   (`Capability::OperatorNotify`) and a main's tool restriction, on a start and again on a resume.
//!   A `TDDY_*` pair any of them produces goes to the agent's tool server, never the agent process.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_coding_agent_specs::{Capability, CodingAgentSpec};
use tddy_core::{HookCommandParams, SessionMetadata};
use tddy_rpc::Status;

use tddy_cli_sessions::session_toolcall::ManagedWorkflow;
use tddy_daemon_kernel::config::DaemonConfig;

use tddy_core::session_pointer::SessionPointer;

use crate::claude_cli_spawn_steps::{
    claude_cli_hooks, record_hook_answer_wait, resumed_hooks, ClaudeCliHooks,
};
use crate::cli_session_pointer::session_pointer_of;
use crate::launch_ports::LaunchSessions;
use crate::managed_launch;
use crate::project_home_launch::{spec_role_launch, RoleWiring, StartRole};
use crate::spec_launch::{AgentToolset, HostAgent, McpRegistration};

/// What a generic launch's session wiring is built from.
pub(crate) struct SessionWiringInputs<'a> {
    pub spec: &'a CodingAgentSpec,
    /// The agent's resolved `tddy-tools` dependency: what its hooks, managed workflow and tool
    /// server run.
    pub tddy_tools_path: &'a str,
    pub config: &'a DaemonConfig,
    pub tddy_data_dir: &'a Path,
    /// The session's OS user's agent-tool socket, served by this daemon: its hooks report and its
    /// notifications go over it.
    pub agent_tool_socket: &'a Path,
    pub os_user: &'a str,
    pub session_id: &'a str,
    pub session_dir: &'a Path,
    pub worktree: &'a Path,
    /// The managed workflow the session is driven by, when it has one.
    pub managed: Option<ManagedWiring>,
    /// The session's standing role: a project-home main is given its tool restriction.
    pub role: StartRole<'a>,
    /// The agent credential the session tool transport is minted around — `None` exports nothing:
    /// a start with no caller token (the daemon's own children), a session whose agent runs on
    /// another host, a daemon that signs no tokens.
    pub session_tool_token: Option<String>,
    /// Answers the agent's tools' `github-token` over the managed session's toolcall socket.
    pub github_credential_handler: Option<Arc<dyn tddy_core::toolcall::GithubCredentialHandler>>,
    /// The orchestrator that started the session, when one did: its system prompt is told how to
    /// report back (`#orchestra` 2/9).
    pub reports_to: Option<&'a crate::back_channel::ReportsTo<'a>>,
    /// Whether the agent runs on a target, which decides the channel it reports over.
    pub on_target: bool,
}

/// A session's managed workflow: its recipe and the handlers its toolcall listener is bound to.
pub(crate) struct ManagedWiring {
    pub recipe: Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>,
    pub child_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>>,
    pub conversation_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
}

/// What the spec's capabilities and the session's role add to a generic launch.
#[derive(Default)]
pub(crate) struct SessionWiring {
    /// The session's hooks (a spec declaring `hooks` or `operator_notify`), with their token.
    pub hooks: Option<ClaudeCliHooks>,
    /// The managed workflow to attach to the session once its agent is spawned.
    pub workflow: Option<ManagedWorkflow>,
    /// Passed with the spec's `append_system_prompt` flag: a managed recipe's prompt, composed with
    /// the operator-notify guidance.
    pub system_prompt_file: Option<PathBuf>,
    /// The environment the agent process is given beyond the spec's: the non-`TDDY_` pairs the
    /// session's wiring produced, plus the session tool transport (`TDDY_REMOTE_*`), whose export
    /// to the agent process is what a `tddy-tools` run from its own shell routes by.
    pub env: Vec<(String, String)>,
    /// Flags already rendered (a main's role prompt), placed after the slot flags.
    pub extra_args: Vec<String>,
    /// The tool server, the pre-approved tools and the withdrawn ones.
    pub toolset: AgentToolset,
    /// What a `tddy-tools` the agent runs from its shell finds its session by — the managed
    /// workflow's socket and the operator-notify session — written into the worktree
    /// ([`crate::cli_session_pointer`]); `None` when there is neither.
    pub session_pointer: Option<SessionPointer>,
}

/// The wiring the spec's capabilities ask of a generic launch: hooks and operator notify for the
/// agent that declares them, the managed workflow for a session driven by a recipe, and a main's
/// tool restriction. A `TDDY_*` variable these steps produce belongs to the agent's tool server
/// (`tddy-tools --mcp`'s registration) and is carried by its `env` block, and the session a
/// shell-run `tddy-tools` reaches (the workflow socket, the operator-notify session) is the session
/// pointer's; neither reaches the agent process, whatever the spec declares. The one exception is
/// the session tool transport a `session_tool_token` asks for (`TDDY_REMOTE_*`): it is exported to
/// the agent process precisely so the shell — and any `tddy-tools --mcp` child, which inherits it —
/// routes `tddy-tools monitor` and `tddy-tools send` to this daemon.
pub(crate) fn wire_session_agent(inputs: SessionWiringInputs<'_>) -> Result<SessionWiring, Status> {
    let SessionWiringInputs {
        spec,
        tddy_tools_path,
        config,
        tddy_data_dir,
        agent_tool_socket,
        os_user,
        session_id,
        session_dir,
        worktree,
        managed,
        role,
        session_tool_token,
        github_credential_handler,
        reports_to,
        on_target,
    } = inputs;
    let hooks = (spec.has(Capability::Hooks) || spec.has(Capability::OperatorNotify))
        .then(|| claude_cli_hooks(config, agent_tool_socket, spec, tddy_tools_path));
    if let Some(hooks) = &hooks {
        record_hook_answer_wait(session_dir, hooks)?;
    }
    let mut process_env: Vec<(String, String)> = Vec::new();
    if let Some(agent_session_token) = session_tool_token {
        process_env.extend(
            tddy_session_split::split_session::colocated_jail_tool_env(
                &tddy_daemon_kernel::daemon_hook_urls::local_daemon_hook_url(config),
                session_id,
                &agent_session_token,
                Some(agent_tool_socket.to_string_lossy().into_owned()),
            )
            .env_pairs(),
        );
    }
    let mut tool_server_env: Vec<(String, String)> = Vec::new();
    let mut notify_env: Vec<(String, String)> = Vec::new();
    let mut system_prompt_file: Option<PathBuf> = None;
    let mut workflow = None;
    if let Some(ManagedWiring {
        recipe,
        child_spawn_handler,
        conversation_spawn_handler,
    }) = managed
    {
        let launch = managed_launch::prepare_managed_workflow_inner(
            tddy_data_dir,
            session_id,
            recipe,
            session_dir,
            worktree,
            session_dir,
            tddy_tools_path,
            None,
            child_spawn_handler,
            conversation_spawn_handler,
            github_credential_handler,
        )?;
        system_prompt_file = Some(launch.prompt_file);
        route_tool_server_env(launch.env, &mut process_env, &mut tool_server_env);
        workflow = Some(launch.workflow);
    }
    if let (true, Some(hooks)) = (spec.has(Capability::OperatorNotify), hooks.as_ref()) {
        let back_channel = reports_to.map(|reports_to| {
            let placement = if on_target {
                crate::back_channel::ChildPlacement::Target {
                    hook_token: &hooks.hook_token,
                    hook_log_path: tddy_core::REMOTE_HOOK_LOG,
                }
            } else {
                crate::back_channel::ChildPlacement::Local
            };
            reports_to.guidance_for(spec, placement)
        });
        system_prompt_file = crate::notify_env::agent_notify_unless_role(
            role.name,
            crate::notify_env::AgentNotifyLaunch {
                config,
                agent_tool_socket,
                os_user,
                session_id,
                hook_token: &hooks.hook_token,
                session_dir,
                managed_prompt_file: system_prompt_file.as_deref(),
            },
            back_channel.as_deref(),
            &mut notify_env,
        )?;
    }
    let role_wiring = RoleWiring::of(role.main)?;
    route_tool_server_env(role_wiring.env, &mut process_env, &mut tool_server_env);
    let (extra_args, mut toolset) = spec_role_launch(role_wiring.main.as_ref());
    let session_pointer = session_pointer_of(&tool_server_env, notify_env, worktree);
    carry_on_the_tool_server(
        spec,
        &mut toolset,
        tool_server_env,
        tddy_tools_path,
        session_dir,
    );
    Ok(SessionWiring {
        hooks,
        workflow,
        system_prompt_file,
        env: process_env,
        extra_args,
        toolset,
        session_pointer,
    })
}

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
    /// `os_user`'s agent-tool socket, served by this daemon (bound first when it is not yet): where
    /// that user's sessions report their hooks and make their tool calls. A user this daemon cannot
    /// give one — under the supervisor, one not listed under `agent_tool_sockets:` — fails the
    /// start `FAILED_PRECONDITION`, naming the setting, before anything is cut.
    pub(crate) async fn agent_tool_socket_of(&self, os_user: &str) -> Result<PathBuf, Status> {
        self.agent_tool_sockets
            .ensure_bound(os_user, &self.tddy_data_dir)
            .await
            .map_err(|e| Status::failed_precondition(format!("{e:#}")))
    }

    /// The agent credential a session's own shell authenticates its tool calls with, minted under
    /// the caller that started (or resumed) the session — `None` when there is nothing to mint
    /// from: the daemon's own callerless children, or a daemon that signs no tokens. A daemon that
    /// cannot mint is a session whose shell runs without the transport, not a failed start — it
    /// says so on stderr and carries on, as it did before the transport existed.
    pub(crate) fn session_tool_token_for(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> Option<String> {
        if session_token.is_empty() {
            return None;
        }
        match self
            .split_sessions
            .agent_session_token_for(session_token, session_id)
        {
            Ok(token) => Some(token),
            Err(e) => {
                log::warn!(
                    "session {session_id}: no session tool transport for its agent (token mint failed: {e:#})"
                );
                None
            }
        }
    }

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
        let mut process_env = Vec::new();
        // A resumed agent's shell gets the session tool transport again, under a credential minted
        // now — nothing persists the previous process's token. A session whose agent runs on
        // another host is exported none: the socket and the URL name this host, which its shell
        // cannot reach, and detection would prefer the dead route. A split agent's process runs
        // model-authored code joined to its room over its own split transport — none of this.
        let runs_on_this_host = local_session
            && meta.target.is_none()
            && meta
                .ssh_config_host
                .as_deref()
                .is_none_or(|host| host.trim().is_empty());
        let session_tool_token = if runs_on_this_host {
            self.session_tool_token_for(session_id, session_token)
        } else {
            None
        };
        if let Some(agent_session_token) = session_tool_token {
            process_env.extend(
                tddy_session_split::split_session::colocated_jail_tool_env(
                    &tddy_daemon_kernel::daemon_hook_urls::local_daemon_hook_url(&self.config),
                    session_id,
                    &agent_session_token,
                    Some(tool_socket.to_string_lossy().into_owned()),
                )
                .env_pairs(),
            );
        }
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
        let orchestrator = self.resumed_orchestrator_link(session_dir);
        let sessions_base = session_dir.parent().and_then(Path::parent);
        let reports_to =
            sessions_base.and_then(|base| self.reports_to_of(orchestrator.as_ref(), base));
        let on_target = meta.target.is_some();
        if spec.has(Capability::OperatorNotify) {
            let back_channel = reports_to.as_ref().and_then(|reports_to| {
                let placement = if on_target {
                    crate::back_channel::ChildPlacement::Target {
                        hook_token: meta
                            .hook_token
                            .as_deref()
                            .filter(|token| !token.is_empty())?,
                        hook_log_path: tddy_core::REMOTE_HOOK_LOG,
                    }
                } else {
                    crate::back_channel::ChildPlacement::Local
                };
                Some(reports_to.guidance_for(spec, placement))
            });
            let notify = crate::notify_env::resumed_agent_notify_launch_reporting(
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
        // An orchestrator and its children on this host message each other by peer name.
        if !on_target
            && (reports_to.is_some()
                || meta
                    .session_role
                    .as_deref()
                    .is_some_and(tddy_core::session_metadata::is_orchestrator_role))
        {
            extra_args.extend(crate::back_channel::peer_name_args(spec, session_id));
        }
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
}

/// Whether `spec`'s hooks adapter reports over the daemon's agent-tool socket: it refuses params
/// that carry none (`SlotError::NoDaemonSocket`), where the hooks of an adapter that reports over
/// HTTP render with none.
pub(crate) fn hooks_report_over_the_agent_tool_socket(spec: &CodingAgentSpec) -> bool {
    let Ok(adapter) = tddy_coding_agent_specs::slots::hooks::adapter(&spec.agent.hooks.adapter)
    else {
        return false;
    };
    let probe = HookCommandParams {
        tddy_tools_path: "probe",
        daemon_url: "probe",
        session_id: "probe",
        os_user: "probe",
        hook_token: "probe",
        daemon_socket: None,
        answer_wait_secs: spec.answer_wait_secs(),
        remote_log: None,
        route_env: &[],
    };
    adapter.try_render(Path::new("/"), &probe).is_err()
}

impl SessionWiring {
    /// The hook parameters of `session_id`'s start, when the spec declares `hooks`. `route_env` is
    /// the provider route's non-secret pairs, pinned beside the hooks in the settings file; the
    /// key is never among them.
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

/// Gives the `TDDY_*` pairs a session's wiring produced (a managed workflow's `TDDY_SOCKET`) to the
/// agent's tool server: the pairs join the registration of a role's toolset, and a session with none
/// is registered `tddy-tools --mcp` with them — in a file of the session's directory, for a spec whose
/// `mcp` slot names a file. A spec whose slot places its registration in the agent's home
/// (`$HOME/.cursor/mcp.json`) or takes none registers nothing: that file is the operator's, and the
/// pairs reach no one through the registration; the agent's own shell reaches its controller through
/// the worktree's `.tddy/session.json` pointer (`cli_session_pointer`).
fn carry_on_the_tool_server(
    spec: &CodingAgentSpec,
    toolset: &mut AgentToolset,
    tool_server_env: Vec<(String, String)>,
    tddy_tools_path: &str,
    session_dir: &Path,
) {
    if let Some(registration) = toolset.mcp.as_mut() {
        registration.server.env.extend(tool_server_env);
        return;
    }
    let names_a_file = matches!(
        spec.agent.mcp,
        tddy_coding_agent_specs::McpSlot::File { .. }
    );
    if tool_server_env.is_empty() || !names_a_file {
        return;
    }
    toolset.mcp = Some(McpRegistration {
        server: tddy_coding_agent_specs::slots::McpServerCommand {
            name: "tddy-tools".to_string(),
            command: PathBuf::from(tddy_tools_path),
            args: vec!["--mcp".to_string()],
            env: tool_server_env.into_iter().collect(),
            cwd: None,
        },
        dir: session_dir.to_path_buf(),
    });
}

/// `pairs` split: a `TDDY_*` pair is the tool server's, the rest the agent process's.
fn route_tool_server_env(
    pairs: Vec<(String, String)>,
    process: &mut Vec<(String, String)>,
    tool_server: &mut Vec<(String, String)>,
) {
    for (name, value) in pairs {
        if name.starts_with("TDDY_") {
            tool_server.push((name, value));
        } else {
            process.push((name, value));
        }
    }
}

#[cfg(test)]
mod cli_agent_steps_tests;
pub(crate) mod conversation;
pub(crate) mod host_launch_slots;
