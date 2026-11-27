//! What a session's standing role adds to its claude-cli agent's launch: for a project-home (main)
//! session, its wiring to `tddy-tools`; for every other role, nothing.

use std::path::{Path, PathBuf};

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_metadata::{is_orchestrator_role, ORCHESTRATOR_ROLE};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_hook_urls;
use tddy_rpc::Status;

use tddy_coding_agent_specs::slots::McpServerCommand;

use crate::launch_ports::LaunchSessions;
use crate::spec_launch::{AgentToolset, McpRegistration};

/// What a project-home (main) agent's launch is built from. See [`project_home_wiring`].
pub(crate) struct ProjectHomeLaunch<'a> {
    /// The session's standing role (`project-home` or `orchestrator`), which words its prompt.
    pub(crate) role: &'a str,
    pub(crate) config: &'a DaemonConfig,
    /// The main's OS user's agent-tool socket, which its tool calls reach this daemon over.
    pub(crate) agent_tool_socket: &'a Path,
    /// Where the main's MCP config and role prompt are written.
    pub(crate) session_dir: &'a Path,
    /// The main's own session id, which its tool calls name.
    pub(crate) session_id: &'a str,
    /// A credential minted for the agent; never the caller's own.
    pub(crate) agent_session_token: &'a str,
    /// The toolchain's `tddy-tools`, absolute.
    pub(crate) tddy_tools_path: &'a Path,
}

/// What a session's role adds to its agent's launch: a main's [`MainAgentFlags`] (none for any other
/// role) and the environment its tools reach the daemon by.
#[derive(Debug, Default)]
pub(crate) struct RoleWiring {
    pub(crate) main: Option<MainAgentFlags>,
    pub(crate) env: Vec<(String, String)>,
}

impl RoleWiring {
    /// What a role adds to its agent's launch: a main's [`MainLaunchPlan`] carried out, or nothing.
    pub(crate) fn of(main: Option<MainLaunchPlan<'_>>) -> Result<Self, Status> {
        Ok(main
            .map(|plan| plan.launch())
            .transpose()?
            .unwrap_or_default())
    }
}

/// A main's flags as launch inputs: its pre-approved tool set, every native filesystem and shell
/// tool withdrawn, `tddy-tools --mcp` as its only MCP server (registered in its session dir), and
/// its role prompt, which the spec-driven launch hands to the agent's slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MainAgentFlags {
    prompt_file: PathBuf,
    session_dir: PathBuf,
    tddy_tools_path: PathBuf,
}

impl MainAgentFlags {
    fn allowed_tools() -> Vec<String> {
        tddy_tool_engine::read_only::project_home_allowed_tools()
    }

    fn denied_tools() -> Vec<String> {
        tddy_session_split::split_session::NATIVE_FILESYSTEM_TOOLS
            .iter()
            .map(|tool| (*tool).to_string())
            .collect()
    }

    fn prompt_args(&self) -> Vec<String> {
        vec![
            "--append-system-prompt-file".to_string(),
            self.prompt_file.to_string_lossy().into_owned(),
        ]
    }

    /// The spec-driven launch's inputs: the toolset its slots render, and the role prompt
    /// pre-rendered — the `append_system_prompt` slot takes one file, and a managed recipe's prompt
    /// can take it on the same launch.
    pub(crate) fn spec_launch(&self) -> (Vec<String>, AgentToolset) {
        let server = McpServerCommand {
            name: "tddy-tools".to_string(),
            command: self.tddy_tools_path.clone(),
            args: vec!["--mcp".to_string()],
            env: tddy_session_split::agent_argv::tddy_tools_mcp_env(&self.session_dir),
            cwd: None,
        };
        let toolset = AgentToolset {
            mcp: Some(McpRegistration {
                server,
                dir: self.session_dir.clone(),
            }),
            allowed_tools: Self::allowed_tools(),
            denied_tools: Self::denied_tools(),
        };
        (self.prompt_args(), toolset)
    }
}

/// `main`'s flags as the spec-driven launch takes them; nothing for a session that is not a main.
pub(crate) fn spec_role_launch(main: Option<&MainAgentFlags>) -> (Vec<String>, AgentToolset) {
    main.map(MainAgentFlags::spec_launch).unwrap_or_default()
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
    role: &'a str,
    config: &'a DaemonConfig,
    agent_tool_socket: PathBuf,
    session_dir: PathBuf,
    session_id: &'a str,
    agent_session_token: String,
}

impl MainLaunchPlan<'_> {
    /// The main's wiring ([`project_home_wiring`]) under the toolchain's `tddy-tools`, its prompt
    /// written into its session dir.
    fn launch(self) -> Result<RoleWiring, Status> {
        project_home_wiring(ProjectHomeLaunch {
            role: self.role,
            config: self.config,
            agent_tool_socket: &self.agent_tool_socket,
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

/// A main's wiring: `tddy-tools --mcp` as its only MCP server, reaching this daemon as the main
/// (over its OS user's agent-tool socket), every native filesystem and shell tool
/// withdrawn, its tool set pre-approved, and the role prompt appended ([`MainAgentFlags`]). The
/// role prompt is written now.
///
/// No `--permission-prompt-tool`: a main is an interactive terminal, so a call outside its
/// pre-approved set prompts its operator there.
pub(crate) fn project_home_wiring(launch: ProjectHomeLaunch<'_>) -> Result<RoleWiring, Status> {
    project_home_wiring_on(&tddy_fs::DiskFs, launch)
}

/// [`project_home_wiring`] through the filesystem port.
pub(crate) fn project_home_wiring_on(
    fs: &dyn tddy_fs::Fs,
    launch: ProjectHomeLaunch<'_>,
) -> Result<RoleWiring, Status> {
    let ProjectHomeLaunch {
        role,
        config,
        agent_tool_socket,
        session_dir,
        session_id,
        agent_session_token,
        tddy_tools_path,
    } = launch;
    let prompt_file = session_dir.join(PROJECT_HOME_PROMPT_BASENAME);
    fs.write(&prompt_file, project_home_prompt(role).as_bytes())
        .map_err(|e| {
            Status::internal(format!(
                "failed to write the main's role prompt {}: {e}",
                prompt_file.display()
            ))
        })?;

    // The main's own id: its record holds the home worktree, so the daemon runs its tools there
    // under the project-home role guard.
    let env = tddy_session_split::split_session::colocated_jail_tool_env(
        &daemon_hook_urls::local_daemon_hook_url(config),
        session_id,
        agent_session_token,
        Some(agent_tool_socket.to_string_lossy().into_owned()),
    )
    .env_pairs();
    Ok(RoleWiring {
        main: Some(MainAgentFlags {
            prompt_file,
            session_dir: session_dir.to_path_buf(),
            tddy_tools_path: tddy_tools_path.to_path_buf(),
        }),
        env,
    })
}

/// What a role is told after what it is: how it works and every tool it is offered.
const PROJECT_HOME_PROMPT_BODY: &str = include_str!("project_home_prompt.md");

/// The role prompt an orchestrator-role session is launched with (`#orchestra` 9/9): one prompt for
/// both roles, opening with what this session is — the project's main (`project-home`) or one of
/// its orchestrators (`orchestrator`) — then how it works: every piece of coding goes through a tddy
/// session it starts and follows to its end; each child is told to report its progress and status
/// over its back channel; children are messaged by their listed route; the base is checked and
/// refreshed before work starts and trunk commits are its own activities; reads beyond its reach
/// and any write go through `RequestPathAccess`; session files are attached to the sessions it
/// starts; the LSP tools take a root; a terminal it claimed is released when it is done with it.
pub(crate) fn project_home_prompt(role: &str) -> String {
    let opening = if role == ORCHESTRATOR_ROLE {
        "# You are one of this project's orchestrators"
    } else {
        "# You are this project's main session"
    };
    format!("{opening}\n\n{PROJECT_HOME_PROMPT_BODY}")
}

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
        agent_tool_socket: &Path,
    ) -> Result<StartRole<'a>, Status> {
        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        let main = self.main_plan(
            Some(name),
            session_id,
            session_dir,
            session_token,
            agent_tool_socket,
        )?;
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
        agent_tool_socket: &Path,
    ) -> Result<RoleWiring, Status> {
        let role = meta.session_role.as_deref();
        RoleWiring::of(self.main_plan(
            role,
            session_id,
            session_dir.to_path_buf(),
            session_token,
            agent_tool_socket,
        )?)
    }

    /// For a main, the plan of its launch ([`project_home_wiring`]) under a credential of its own,
    /// minted now from the caller's `session_token` — the caller's lives minutes, the agent hours. A
    /// refusal fails the launch. Every other role has no plan and mints nothing.
    fn main_plan<'a>(
        &'a self,
        role: Option<&'a str>,
        session_id: &'a str,
        session_dir: PathBuf,
        session_token: &str,
        agent_tool_socket: &Path,
    ) -> Result<Option<MainLaunchPlan<'a>>, Status> {
        let Some(role) = role.filter(|role| is_orchestrator_role(role)) else {
            return Ok(None);
        };
        Ok(Some(MainLaunchPlan {
            role,
            config: &self.config,
            agent_tool_socket: agent_tool_socket.to_path_buf(),
            session_dir,
            session_id,
            agent_session_token: self
                .split_sessions
                .agent_session_token_for(session_token, session_id)?,
        }))
    }
}

#[cfg(test)]
mod project_home_launch_tests;
#[cfg(test)]
mod project_home_prompt_tests;
