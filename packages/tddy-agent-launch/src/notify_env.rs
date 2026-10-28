//! What a daemon-launched claude-cli or cursor-cli agent is given so it can notify its operator
//! with `tddy-tools notify` (`#home-inbox` 1/8).
//!
//! Two parts:
//!
//! - **The environment** ([`notify_env_pairs`]): the session it runs in, the OS user that owns it,
//!   its hook token and how to reach the daemon. These are the credentials its hooks already carry
//!   in `.claude/settings.local.json`, so exporting them grants the agent nothing it could not
//!   already do.
//! - **The guidance** ([`AGENT_NOTIFY_GUIDANCE`]): one paragraph appended to a claude-cli agent's
//!   system prompt, telling it the command exists and when — and when not — to use it.
//!
//! A session with a standing role (the project-home main) has no shell to run the command in, so it
//! is given neither ([`agent_notify_unless_role`], [`resumed_agent_notify_launch`]).

use std::path::{Path, PathBuf};

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_hook_urls;
use tddy_rpc::Status;
use tddy_service::session_activity::notify_env;

/// The paragraph a claude-cli agent's system prompt is given about `tddy-tools notify`.
pub const AGENT_NOTIFY_GUIDANCE: &str = include_str!("agent_notify_guidance.md");

/// The file in the session directory that holds the system prompt a claude-cli agent appends.
const NOTIFY_SYSTEM_PROMPT_FILE: &str = "agent-system-prompt.md";

/// The `(name, value)` pairs `tddy-tools notify` reads its session from
/// ([`tddy_service::session_activity::notify_env`]).
///
/// The daemon is named by exactly one of two variables: `TDDY_DAEMON_SOCKET` when `daemon_socket`
/// is given — an embedded daemon serves no HTTP RPC, so a URL there would reach nothing — and
/// `TDDY_DAEMON_URL` otherwise.
#[must_use]
pub fn notify_env_pairs(
    session_id: &str,
    os_user: &str,
    hook_token: &str,
    daemon_url: &str,
    daemon_socket: Option<&Path>,
) -> Vec<(String, String)> {
    let daemon = match daemon_socket {
        Some(socket) => (
            notify_env::DAEMON_SOCKET,
            socket.to_string_lossy().into_owned(),
        ),
        None => (notify_env::DAEMON_URL, daemon_url.to_string()),
    };
    [
        (notify_env::SESSION_ID, session_id.to_string()),
        (notify_env::OS_USER, os_user.to_string()),
        (notify_env::HOOK_TOKEN, hook_token.to_string()),
        daemon,
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value))
    .collect()
}

/// What a claude-cli agent is given so it can notify its operator (`#home-inbox` 1/8).
pub struct AgentNotifyLaunch<'a> {
    pub config: &'a DaemonConfig,
    /// The data dir the daemon resolved — an embedded daemon's agent-tool socket is digested from
    /// it, as the claude-cli hooks installer reads it.
    pub tddy_data_dir: &'a Path,
    pub os_user: &'a str,
    pub session_id: &'a str,
    /// The token the claude-cli hooks installer minted for this session.
    pub hook_token: &'a str,
    /// Where the composed system-prompt file is written.
    pub session_dir: &'a Path,
    /// A managed recipe's orchestration prompt, when the session has one: the guidance is
    /// composed with it, because `claude` takes one `--append-system-prompt-file`.
    pub managed_prompt_file: Option<&'a Path>,
}

/// The notify environment and the system-prompt file a claude-cli agent is launched with.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AgentNotifyWiring {
    /// [`notify_env_pairs`] for this session.
    pub env: Vec<(String, String)>,
    /// The file `claude` appends to its system prompt: the notify guidance, followed by the
    /// managed recipe's prompt when there is one.
    pub append_system_prompt_file: Option<PathBuf>,
}

/// What a starting claude-cli agent is given so it can notify its operator.
pub fn agent_notify_launch(launch: AgentNotifyLaunch<'_>) -> Result<AgentNotifyWiring, Status> {
    let AgentNotifyLaunch {
        config,
        tddy_data_dir,
        os_user,
        session_id,
        hook_token,
        session_dir,
        managed_prompt_file,
    } = launch;
    let daemon_url = daemon_hook_urls::claude_hook_daemon_url(config);
    // An embedded daemon serves no HTTP RPC; its agents notify over this socket, as its hooks report.
    let daemon_socket = tddy_daemon_kernel::embedded_host_agent_tool_socket(tddy_data_dir);
    let env = notify_env_pairs(
        session_id,
        os_user,
        hook_token,
        &daemon_url,
        daemon_socket.as_deref().map(Path::new),
    );
    let prompt_file = session_dir.join(NOTIFY_SYSTEM_PROMPT_FILE);
    std::fs::write(&prompt_file, composed_system_prompt(managed_prompt_file)?).map_err(|e| {
        Status::internal(format!(
            "failed to write the system prompt {}: {e}",
            prompt_file.display()
        ))
    })?;
    Ok(AgentNotifyWiring {
        env,
        append_system_prompt_file: Some(prompt_file),
    })
}

/// The notify guidance, followed by the managed recipe's prompt read from `managed_prompt_file`.
fn composed_system_prompt(managed_prompt_file: Option<&Path>) -> Result<String, Status> {
    let guidance = AGENT_NOTIFY_GUIDANCE.trim_end();
    let Some(file) = managed_prompt_file else {
        return Ok(guidance.to_string());
    };
    let recipe_prompt = std::fs::read_to_string(file).map_err(|e| {
        Status::internal(format!(
            "failed to read the recipe prompt {}: {e}",
            file.display()
        ))
    })?;
    Ok(format!("{guidance}\n\n{recipe_prompt}"))
}

/// A start's system-prompt file, with the notify wiring applied unless the session has a standing
/// role: `env_extra` gains the notify environment and the composed file is returned. A session
/// with a role (`role_name` non-empty) keeps `launch.managed_prompt_file` and gains nothing.
pub fn agent_notify_unless_role(
    role_name: &str,
    launch: AgentNotifyLaunch<'_>,
    env_extra: &mut Vec<(String, String)>,
) -> Result<Option<PathBuf>, Status> {
    if !role_name.is_empty() {
        return Ok(launch.managed_prompt_file.map(Path::to_path_buf));
    }
    let notify = agent_notify_launch(launch)?;
    env_extra.extend(notify.env);
    Ok(notify.append_system_prompt_file)
}

/// What a resumed claude-cli agent is given again: the same wiring as at its start, built from the
/// hook token its `.session.yaml` recorded. A session that recorded none — a sandboxed one has no
/// hooks — and a session with a standing role are given nothing.
pub fn resumed_agent_notify_launch(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    os_user: &str,
    session_dir: &Path,
    meta: &tddy_core::SessionMetadata,
    managed_prompt_file: Option<&Path>,
) -> Result<AgentNotifyWiring, Status> {
    if !meta.session_role.as_deref().unwrap_or("").is_empty() {
        return Ok(AgentNotifyWiring::default());
    }
    let Some(hook_token) = meta.hook_token.as_deref().filter(|token| !token.is_empty()) else {
        return Ok(AgentNotifyWiring::default());
    };
    agent_notify_launch(AgentNotifyLaunch {
        config,
        tddy_data_dir,
        os_user,
        session_id: &meta.session_id,
        hook_token,
        session_dir,
        managed_prompt_file,
    })
}
