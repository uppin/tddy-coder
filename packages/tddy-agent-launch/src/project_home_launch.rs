//! What a session's standing role adds to its claude-cli agent's launch: for a project-home (main)
//! session, its wiring to `tddy-tools`; for every other role, nothing.

use std::path::{Path, PathBuf};

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_hook_urls;
use tddy_rpc::Status;
use tddy_session_split::split_session::SplitAgentWiring;

use crate::claude_cli_spawn_steps::role_allowed_tools_args;
use crate::launch_ports::LaunchSessions;

/// What a project-home (main) agent's launch is built from. See [`project_home_launch`].
pub(crate) struct ProjectHomeLaunch<'a> {
    pub(crate) config: &'a DaemonConfig,
    /// The data dir the daemon *resolved*: an embedded host's agent-tool socket path digests it.
    pub(crate) tddy_data_dir: &'a Path,
    /// Where the main's MCP config and role prompt are written.
    pub(crate) session_dir: &'a Path,
    /// The main's own session id, which its tool calls name.
    pub(crate) session_id: &'a str,
    /// A credential minted for the agent; never the caller's own.
    pub(crate) agent_session_token: &'a str,
    /// The toolchain's `tddy-tools`, absolute.
    pub(crate) tddy_tools_path: &'a Path,
}

/// The flags and environment a session's role adds to its agent's launch.
#[derive(Debug, Default)]
pub(crate) struct RoleLaunch {
    pub(crate) extra_args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

impl RoleLaunch {
    /// What a role adds to its agent's launch: a main's [`MainLaunchPlan`] carried out, or nothing.
    pub(crate) fn of(main: Option<MainLaunchPlan<'_>>) -> Result<Self, Status> {
        Ok(main
            .map(|plan| plan.launch())
            .transpose()?
            .unwrap_or_default())
    }

    /// A resumed agent's extra flags and environment: `split`'s wiring (none for a session that is
    /// not split), then this role's.
    pub(crate) fn after_split(
        self,
        split: Option<SplitAgentWiring>,
    ) -> (Vec<String>, Vec<(String, String)>) {
        let (mut args, mut env) = split.map(|w| (w.extra_args, w.env)).unwrap_or_default();
        args.extend(self.extra_args);
        env.extend(self.env);
        (args, env)
    }
}

/// A starting session's standing role: its name, recorded in the session's record before the agent
/// is spawned (empty for an ordinary session), and, for a main, the launch its agent is given.
pub(crate) struct StartRole<'a> {
    pub(crate) name: &'a str,
    pub(crate) main: Option<MainLaunchPlan<'a>>,
}

impl StartRole<'static> {
    /// No standing role: nothing recorded, and the agent's argv and environment are unchanged.
    pub(crate) fn ordinary() -> Self {
        Self {
            name: "",
            main: None,
        }
    }
}

/// Everything a main's launch is built from, its credential already minted. Nothing is written
/// until [`Self::launch`], so a start refused before its agent is spawned leaves nothing on disk.
pub(crate) struct MainLaunchPlan<'a> {
    config: &'a DaemonConfig,
    tddy_data_dir: &'a Path,
    session_dir: PathBuf,
    session_id: &'a str,
    agent_session_token: String,
}

impl MainLaunchPlan<'_> {
    /// The main's launch ([`project_home_launch`]) under the toolchain's `tddy-tools`, its prompt
    /// and MCP config written into its session dir.
    fn launch(self) -> Result<RoleLaunch, Status> {
        project_home_launch(ProjectHomeLaunch {
            config: self.config,
            tddy_data_dir: self.tddy_data_dir,
            session_dir: &self.session_dir,
            session_id: self.session_id,
            agent_session_token: &self.agent_session_token,
            tddy_tools_path:
                &tddy_session_split::svc_resolve_tddy_tools_path::resolve_tddy_tools_path(
                    self.config,
                )?,
        })
    }
}

/// A main's launch: `tddy-tools --mcp` as its only MCP server, reaching this daemon as the main
/// (agent-tool socket on an embedded host, HTTP otherwise), every native filesystem and shell tool
/// withdrawn, its tool set pre-approved, and the role prompt appended.
///
/// No `--permission-prompt-tool`: a main is an interactive terminal, so a call outside its
/// pre-approved set prompts its operator there.
pub(crate) fn project_home_launch(launch: ProjectHomeLaunch<'_>) -> Result<RoleLaunch, Status> {
    let ProjectHomeLaunch {
        config,
        tddy_data_dir,
        session_dir,
        session_id,
        agent_session_token,
        tddy_tools_path,
    } = launch;
    let prompt_file = session_dir.join(PROJECT_HOME_PROMPT_BASENAME);
    std::fs::write(&prompt_file, PROJECT_HOME_PROMPT).map_err(|e| {
        Status::internal(format!(
            "failed to write the main's role prompt {}: {e}",
            prompt_file.display()
        ))
    })?;

    let mut extra_args = role_allowed_tools_args(Some(PROJECT_HOME_ROLE));
    for tool in tddy_session_split::split_session::NATIVE_FILESYSTEM_TOOLS {
        extra_args.push("--disallowedTools".to_string());
        extra_args.push((*tool).to_string());
    }
    extra_args.push("--append-system-prompt-file".to_string());
    extra_args.push(prompt_file.to_string_lossy().into_owned());
    extra_args.extend(tddy_session_split::agent_argv::tddy_tools_only_mcp_args(
        session_dir,
        tddy_tools_path,
    )?);

    // The main's own id: its record holds the home worktree, so the daemon runs its tools there
    // under the project-home role guard. The socket is set only where something serves it.
    let env = tddy_session_split::split_session::colocated_jail_tool_env(
        &daemon_hook_urls::local_daemon_hook_url(config),
        session_id,
        agent_session_token,
        tddy_daemon_kernel::embedded_host_agent_tool_socket(tddy_data_dir),
    )
    .env_pairs();
    Ok(RoleLaunch { extra_args, env })
}

/// What a main is told it is, appended to its system prompt.
const PROJECT_HOME_PROMPT: &str = include_str!("project_home_prompt.md");

/// The main's role prompt, under its session dir.
const PROJECT_HOME_PROMPT_BASENAME: &str = "project-home-prompt.md";

impl LaunchSessions {
    /// A starting session's role `name`, with the launch planned for session `session_id` under
    /// `sessions_base` when it is a main (see [`Self::main_plan`]).
    pub(crate) fn start_role<'a>(
        &'a self,
        name: &'a str,
        session_id: &'a str,
        sessions_base: &Path,
        session_token: &str,
    ) -> Result<StartRole<'a>, Status> {
        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        let main = self.main_plan(Some(name), session_id, session_dir, session_token)?;
        Ok(StartRole { name, main })
    }

    /// What a resumed session's recorded role adds to its agent's launch (see [`Self::main_plan`]):
    /// a main's credential is minted **afresh**, since the one its previous process held may have
    /// expired while it was stopped.
    pub(crate) fn resumed_role(
        &self,
        meta: &tddy_core::SessionMetadata,
        session_id: &str,
        session_dir: &Path,
        session_token: &str,
    ) -> Result<RoleLaunch, Status> {
        let role = meta.session_role.as_deref();
        RoleLaunch::of(self.main_plan(
            role,
            session_id,
            session_dir.to_path_buf(),
            session_token,
        )?)
    }

    /// For a main, the plan of its launch ([`project_home_launch`]) under a credential of its own,
    /// minted now from the caller's `session_token` — the caller's lives minutes, the agent hours. A
    /// refusal fails the launch. Every other role has no plan and mints nothing.
    fn main_plan<'a>(
        &'a self,
        role: Option<&str>,
        session_id: &'a str,
        session_dir: PathBuf,
        session_token: &str,
    ) -> Result<Option<MainLaunchPlan<'a>>, Status> {
        if role != Some(PROJECT_HOME_ROLE) {
            return Ok(None);
        }
        Ok(Some(MainLaunchPlan {
            config: &self.config,
            tddy_data_dir: &self.tddy_data_dir,
            session_dir,
            session_id,
            agent_session_token: self.split_sessions.agent_session_token_for(session_token)?,
        }))
    }
}
