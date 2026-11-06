//! The steps the generic CLI spec launch (`opencode_cli_spawn::start_cli_harness_session` and its
//! resume) gains so that an agent with Claude Code's or Cursor's needs launches through it
//! (`#agent-tools 14/19`). Each step is keyed on a spec slot or capability, never on an agent id:
//!
//! - [`HostLaunchSlots`] / [`render_host_launch_slots`]: the `hooks` slot when the spec declares
//!   `hooks`, the MCP and tool-list slots from the role's [`AgentToolset`], the appended system
//!   prompt (a managed workflow's recipe prompt, a main's prompt) and `{home}`.
//! - [`write_slot_files_on_target`]: an ssh session's slot files written on its target
//!   (`slot_file_over_ssh`, bounded by `SLOT_FILE_OVER_SSH_DEADLINE`), never at that path here.
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

use tddy_coding_agent_specs::slots::conversation::ConversationSlot;
use tddy_coding_agent_specs::slots::{self, RenderedSlots, SlotInputs};
use tddy_coding_agent_specs::{Capability, CodingAgentSpec};
use tddy_core::adapters::{ConversationMint, ConversationMintError, LaunchAdapters};
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_core::{HookCommandParams, SessionMetadata};
use tddy_rpc::Status;

use tddy_cli_sessions::session_toolcall::ManagedWorkflow;
use tddy_daemon_kernel::config::DaemonConfig;

use crate::claude_cli_spawn_steps::{claude_cli_hooks, ClaudeCliHooks};
use crate::harness_from_spec::HarnessLaunch;
use crate::launch_ports::LaunchSessions;
use crate::managed_launch;
use crate::project_home_launch::{spec_role_launch, RoleWiring, StartRole};
use crate::slot_file_over_ssh::write_slot_file_over_ssh;
use crate::spec_launch::{AgentToolset, HostAgent, McpRegistration, SpecLaunchError};

/// What a generic host launch fills beyond its spec's own argv.
#[derive(Default)]
pub(crate) struct HostLaunchSlots<'a> {
    /// The tool server registered and the tools pre-approved and withdrawn.
    pub toolset: AgentToolset,
    /// The session its `hooks` slot writes settings for; `None` writes none (a spec without
    /// `hooks`, or a resume, which keeps the file its start wrote).
    pub hooks: Option<HookCommandParams<'a>>,
    /// Passed with the spec's `instructions.append_system_prompt` flag.
    pub system_prompt_file: Option<&'a Path>,
    /// `{home}` of a placed MCP path: the host's `$HOME`.
    pub home: Option<PathBuf>,
}

/// `agent`'s slots for a host launch in `worktree`, its registration files under `session_dir`.
pub(crate) fn render_host_launch_slots(
    agent: &HostAgent,
    worktree: &Path,
    session_dir: &Path,
    slots: HostLaunchSlots<'_>,
) -> Result<RenderedSlots, Status> {
    let HostLaunchSlots {
        toolset,
        hooks,
        system_prompt_file,
        home,
    } = slots;
    let AgentToolset {
        mcp,
        allowed_tools,
        denied_tools,
    } = toolset;
    let (mcp_server, scratch_dir) = match mcp {
        Some(McpRegistration { server, dir }) => (Some(server), dir),
        None => (None, session_dir.to_path_buf()),
    };
    slots::render(
        &agent.spec,
        &SlotInputs {
            workdir: worktree.to_path_buf(),
            scratch_dir,
            mcp_server,
            allowed_tools,
            denied_tools,
            permission_prompt_tool: None,
            preamble: None,
            append_system_prompt_file: system_prompt_file.map(Path::to_path_buf),
            hooks,
            home,
        },
    )
    .map_err(|e| Status::internal(e.to_string()))
}

/// Write `slots.files` on `remote_host` over ssh and take them out of `slots`, so the launch's
/// local write (`spec_launch::plan_host_pty`) writes nothing at those paths on this host. A local
/// checkout (`None`) leaves `slots` alone. Refused `DEADLINE_EXCEEDED` naming the host when the
/// target never answers.
pub(crate) fn write_slot_files_on_target(
    slots: &mut RenderedSlots,
    remote_host: Option<&str>,
) -> Result<(), Status> {
    let Some(host) = remote_host else {
        return Ok(());
    };
    for (path, bytes) in std::mem::take(&mut slots.files) {
        write_slot_file_over_ssh(host, &path, &bytes).map_err(|error| match error {
            SpecLaunchError::SlotFileOverSshTimedOut { .. } => {
                Status::deadline_exceeded(error.to_string())
            }
            other => Status::internal(format!("failed to write a slot file on {host}: {other}")),
        })?;
        slots.written.retain(|written| *written != path);
    }
    Ok(())
}

/// The conversation a launch binds, from the spec's `coding_agent.conversation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LaunchConversation {
    /// The daemon's session id (`session_id`).
    SessionId(String),
    /// A minted id (`chat_id`), recorded on the session (`cursor_chat_id`).
    Minted(String),
    /// The worktree's own (`--continue`); nothing to bind.
    Worktree,
    /// The token the spec's resume adapter read off the last run, when one was recorded.
    ResumeToken(Option<String>),
}

impl LaunchConversation {
    /// `launch` with this conversation's template parameter set.
    pub(crate) fn bound<'a>(&'a self, launch: HarnessLaunch<'a>) -> HarnessLaunch<'a> {
        match self {
            Self::SessionId(id) => HarnessLaunch {
                session_id: Some(id),
                ..launch
            },
            Self::Minted(id) => HarnessLaunch {
                chat_id: Some(id),
                ..launch
            },
            Self::ResumeToken(token) => HarnessLaunch {
                resume_token: token.as_deref(),
                ..launch
            },
            Self::Worktree => launch,
        }
    }

    /// The id to record on the session's `.session.yaml` (`cursor_chat_id`): the minted one only.
    pub(crate) fn recorded_chat_id(&self) -> Option<&str> {
        match self {
            Self::Minted(id) => Some(id),
            _ => None,
        }
    }
}

/// What a first start's conversation is settled from.
pub(crate) struct ConversationStart<'a> {
    pub spec: &'a CodingAgentSpec,
    /// The agent's resolved binary, which a `minted` adapter runs.
    pub binary: &'a Path,
    pub worktree: &'a Path,
    /// The environment the mint runs with on top of the daemon's (the session's git identity).
    pub env: &'a [(String, String)],
    pub session_id: &'a str,
}

/// The conversation a first start binds: the session id, a freshly minted id (bounded by the
/// slot's mint deadline, refused through [`conversation_mint_refused`]), or nothing to bind.
pub(crate) async fn start_conversation(
    start: ConversationStart<'_>,
) -> Result<LaunchConversation, Status> {
    let ConversationStart {
        spec,
        binary,
        worktree,
        env,
        session_id,
    } = start;
    let conversation = spec.conversation();
    match &conversation {
        ConversationSlot::DaemonSessionId => {
            Ok(LaunchConversation::SessionId(session_id.to_string()))
        }
        ConversationSlot::Worktree => Ok(LaunchConversation::Worktree),
        ConversationSlot::ResumeToken => Ok(LaunchConversation::ResumeToken(None)),
        ConversationSlot::Minted { adapter, .. } => {
            let minter = LaunchAdapters::builtin()
                .conversation_id(adapter)
                .ok_or_else(|| {
                    Status::failed_precondition(format!(
                        "{} names conversation adapter {adapter}, which this build does not have",
                        spec.id
                    ))
                })?;
            let deadline = conversation
                .mint_deadline()
                .expect("a minted conversation has a mint deadline");
            minter
                .mint(ConversationMint {
                    binary,
                    workdir: worktree,
                    env,
                    deadline,
                })
                .await
                .map(LaunchConversation::Minted)
                .map_err(|error| conversation_mint_refused(&spec.id, adapter, &error))
        }
    }
}

/// The conversation a resume of `meta` binds: the session id, the recorded minted id (minted now
/// and recorded when none was, as `resume::chat_to_resume` adopts one today), the worktree, or the
/// recorded resume token.
pub(crate) fn resumed_conversation(
    spec: &CodingAgentSpec,
    meta: &SessionMetadata,
) -> Result<LaunchConversation, Status> {
    Ok(match spec.conversation() {
        ConversationSlot::DaemonSessionId => LaunchConversation::SessionId(meta.session_id.clone()),
        ConversationSlot::Minted { adapter, .. } => match meta.cursor_chat_id.as_deref() {
            Some(chat) if !chat.trim().is_empty() => LaunchConversation::Minted(chat.to_string()),
            // TODO(cli-spec-features): a session recorded without its chat (started before the chat
            // was minted at start) is minted again by `resume::chat_to_resume` on the bespoke path;
            // the generic resume refuses rather than start a conversation the session never had.
            _ => {
                return Err(Status::failed_precondition(format!(
                    "{} recorded no conversation minted by {adapter}: it cannot be resumed",
                    spec.id
                )))
            }
        },
        ConversationSlot::Worktree => LaunchConversation::Worktree,
        ConversationSlot::ResumeToken => {
            LaunchConversation::ResumeToken(meta.harness_resume_token.clone())
        }
    })
}

/// The refusal of a start whose conversation `adapter` did not mint an id for `agent_id`:
/// `DEADLINE_EXCEEDED` for a mint past its deadline, `FAILED_PRECONDITION` otherwise, naming the
/// agent and the adapter.
pub(crate) fn conversation_mint_refused(
    agent_id: &str,
    adapter: &str,
    error: &ConversationMintError,
) -> Status {
    let message = format!("{agent_id} could not mint its conversation with {adapter}: {error}");
    match error {
        ConversationMintError::TimedOut { .. } => Status::deadline_exceeded(message),
        ConversationMintError::Failed(_) => Status::failed_precondition(message),
    }
}

/// A managed codebase in a jail is refused: the jailed generic launch wires no workflow to drive it,
/// and starting without one would run an agent the operator believes is managed.
// TODO(cli-spec-features): the bespoke jailed claude start carries a managed workflow
// (`claude_jail::ClaudeJail.managed`); the generic jailed launch does not yet
// (docs/dev/todo/2026-11-05-generic-jail-managed-workflow.md).
pub(crate) fn refuse_a_managed_codebase_in_a_jail(
    spec: &CodingAgentSpec,
    req: &tddy_service::proto::session::StartSessionRequest,
) -> Result<(), Status> {
    if req.managed_codebase {
        return Err(Status::failed_precondition(format!(
            "a managed codebase in a jail is not launched from the spec yet: {} runs managed on the \
             host only",
            spec.id
        )));
    }
    Ok(())
}

/// A starting session's standing role, refused for an agent that cannot hold it: a project-home
/// main (`PROJECT_HOME_ROLE`) of a spec that does not declare `project_home_main` is refused
/// `FAILED_PRECONDITION` naming the agent. An ordinary session (an empty role) is admitted.
pub(crate) fn refuse_a_role_the_agent_cannot_hold(
    spec: &CodingAgentSpec,
    session_role: &str,
) -> Result<(), Status> {
    if session_role == PROJECT_HOME_ROLE && !spec.has(Capability::ProjectHomeMain) {
        return Err(Status::failed_precondition(format!(
            "{} cannot run a project's main: its spec declares no project_home_main",
            spec.id
        )));
    }
    Ok(())
}

/// What a generic launch's session wiring is built from.
pub(crate) struct SessionWiringInputs<'a> {
    pub spec: &'a CodingAgentSpec,
    pub config: &'a DaemonConfig,
    pub tddy_data_dir: &'a Path,
    pub os_user: &'a str,
    pub session_id: &'a str,
    pub session_dir: &'a Path,
    pub worktree: &'a Path,
    /// The managed workflow the session is driven by, when it has one.
    pub managed: Option<ManagedWiring>,
    /// The session's standing role: a project-home main is given its tool restriction.
    pub role: StartRole<'a>,
    /// Answers the agent's tools' `github-token` over the managed session's toolcall socket.
    pub github_credential_handler: Option<Arc<dyn tddy_core::toolcall::GithubCredentialHandler>>,
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
    /// The agent-tool socket an adapter that reports over it is given when the daemon is not
    /// embedded (the daemon always owns one), `None` when the hooks need none.
    pub hook_socket: Option<String>,
    /// The managed workflow to attach to the session once its agent is spawned.
    pub workflow: Option<ManagedWorkflow>,
    /// Passed with the spec's `append_system_prompt` flag: a managed recipe's prompt, composed with
    /// the operator-notify guidance.
    pub system_prompt_file: Option<PathBuf>,
    /// The environment the agent process is given beyond the spec's: never a `TDDY_*` variable.
    pub env: Vec<(String, String)>,
    /// Flags already rendered (a main's role prompt), placed after the slot flags.
    pub extra_args: Vec<String>,
    /// The tool server, the pre-approved tools and the withdrawn ones.
    pub toolset: AgentToolset,
}

/// The wiring the spec's capabilities ask of a generic launch: hooks and operator notify for the
/// agent that declares them, the managed workflow for a session driven by a recipe, and a main's
/// tool restriction. A `TDDY_*` variable these steps produce belongs to the agent's tool server
/// (`tddy-tools --mcp`'s registration) and is carried by its `env` block; it never reaches the agent
/// process, whatever the spec declares.
pub(crate) fn wire_session_agent(inputs: SessionWiringInputs<'_>) -> Result<SessionWiring, Status> {
    let SessionWiringInputs {
        spec,
        config,
        tddy_data_dir,
        os_user,
        session_id,
        session_dir,
        worktree,
        managed,
        role,
        github_credential_handler,
    } = inputs;
    let hooks = (spec.has(Capability::Hooks) || spec.has(Capability::OperatorNotify))
        .then(|| claude_cli_hooks(config, tddy_data_dir));
    let mut process_env: Vec<(String, String)> = Vec::new();
    let mut tool_server_env: Vec<(String, String)> = Vec::new();
    let mut system_prompt_file: Option<PathBuf> = None;
    let mut workflow = None;
    if let Some(ManagedWiring {
        recipe,
        child_spawn_handler,
        conversation_spawn_handler,
    }) = managed
    {
        let tddy_tools_path = match &hooks {
            Some(hooks) => hooks.tddy_tools_path.clone(),
            None => claude_cli_hooks(config, tddy_data_dir).tddy_tools_path,
        };
        let launch = managed_launch::prepare_managed_workflow_inner(
            tddy_data_dir,
            session_id,
            recipe,
            session_dir,
            worktree,
            session_dir,
            &tddy_tools_path,
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
        system_prompt_file = crate::notify_env::agent_notify_unless_role(
            role.name,
            crate::notify_env::AgentNotifyLaunch {
                config,
                tddy_data_dir,
                os_user,
                session_id,
                hook_token: &hooks.hook_token,
                session_dir,
                managed_prompt_file: system_prompt_file.as_deref(),
            },
            &mut process_env,
        )?;
    }
    let role_wiring = RoleWiring::of(role.main)?;
    route_tool_server_env(role_wiring.env, &mut process_env, &mut tool_server_env);
    let (extra_args, mut toolset) = spec_role_launch(role_wiring.main.as_ref());
    if let Some(registration) = toolset.mcp.as_mut() {
        registration.server.env.extend(tool_server_env);
    }
    let hook_socket = hooks
        .as_ref()
        .and_then(|hooks| socket_for_hooks(spec, hooks, tddy_data_dir));
    Ok(SessionWiring {
        hooks,
        hook_socket,
        workflow,
        system_prompt_file,
        env: process_env,
        extra_args,
        toolset,
    })
}

/// What a resumed generic launch re-wires: a main's tool restriction, the managed workflow resumed
/// at the goal it persisted, and the operator-notify wiring a session recorded a hook token for. The
/// hooks file its start wrote is kept as it is.
#[derive(Default)]
pub(crate) struct ResumedWiring {
    pub workflow: Option<ManagedWorkflow>,
    pub system_prompt_file: Option<PathBuf>,
    /// Never a `TDDY_*` variable but the notify pairs a spec declaring `operator_notify` asks for.
    pub env: Vec<(String, String)>,
    pub extra_args: Vec<String>,
    pub toolset: AgentToolset,
}

impl LaunchSessions {
    /// The wiring a resume of `meta` gives `spec`'s agent again — none of it is in a persisted argv.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn resumed_session_wiring(
        &self,
        spec: &CodingAgentSpec,
        meta: &SessionMetadata,
        session_dir: &Path,
        worktree: &Path,
        os_user: &str,
        session_token: &str,
        github_credential_handler: Option<Arc<dyn tddy_core::toolcall::GithubCredentialHandler>>,
    ) -> Result<ResumedWiring, Status> {
        let session_id = meta.session_id.as_str();
        let mut process_env = Vec::new();
        let mut tool_server_env = Vec::new();
        let mut system_prompt_file = None;
        let mut workflow = None;
        if spec.has(Capability::ManagedWorkflow) {
            if let Some(recipe_name) = crate::svc_resume_claude_cli_session::resumed_recipe(
                &self.claude_cli_manager,
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
                let tddy_tools_path =
                    claude_cli_hooks(&self.config, &self.tddy_data_dir).tddy_tools_path;
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
        if spec.has(Capability::OperatorNotify) {
            let notify = crate::notify_env::resumed_agent_notify_launch(
                &self.config,
                &self.tddy_data_dir,
                os_user,
                session_dir,
                meta,
                system_prompt_file.as_deref(),
            )?;
            process_env.extend(notify.env);
            system_prompt_file = notify.append_system_prompt_file.or(system_prompt_file);
        }
        let role = self.resumed_role(meta, session_id, session_dir, session_token)?;
        route_tool_server_env(role.env, &mut process_env, &mut tool_server_env);
        let (extra_args, mut toolset) = spec_role_launch(role.main.as_ref());
        if let Some(registration) = toolset.mcp.as_mut() {
            registration.server.env.extend(tool_server_env);
        }
        Ok(ResumedWiring {
            workflow,
            system_prompt_file,
            env: process_env,
            extra_args,
            toolset,
        })
    }
}

/// The daemon's agent-tool socket, for a spec whose hooks adapter reports over it and `hooks` carry
/// none: that adapter refuses params without one (`SlotError::NoDaemonSocket`), so it is given the
/// path the bespoke Cursor launch always passed.
fn socket_for_hooks(
    spec: &CodingAgentSpec,
    hooks: &ClaudeCliHooks,
    tddy_data_dir: &Path,
) -> Option<String> {
    let adapter = tddy_coding_agent_specs::slots::hooks::adapter(&spec.agent.hooks.adapter).ok()?;
    let needs_a_socket = adapter
        .try_render(Path::new("/"), &hooks.params("probe", "probe"))
        .is_err();
    needs_a_socket.then(|| {
        tddy_daemon_kernel::agent_tool_socket_path(tddy_data_dir)
            .to_string_lossy()
            .into_owned()
    })
}

impl SessionWiring {
    /// The hook parameters of `session_id`'s start, when the spec declares `hooks`.
    pub(crate) fn hook_params<'a>(
        &'a self,
        spec: &CodingAgentSpec,
        session_id: &'a str,
        os_user: &'a str,
    ) -> Option<HookCommandParams<'a>> {
        let hooks = self
            .hooks
            .as_ref()
            .filter(|_| spec.has(Capability::Hooks))?;
        let params = hooks.params(session_id, os_user);
        Some(HookCommandParams {
            daemon_socket: params.daemon_socket.or(self.hook_socket.as_deref()),
            ..params
        })
    }
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
