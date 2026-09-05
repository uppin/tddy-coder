//! The agent half of a `sandboxed` session: `claude` running **on this host**, with every native
//! route to the checkout withdrawn and every `mcp__tddy-tools__*` call dispatched into the jail.
//!
//! PRD: `docs/ft/coder/sandboxed-codebase-mode.md`.
//!
//! The mirror image of `spawn.rs`. There, the app builds a jail with an agent inside it and the
//! host answers the agent's tool calls; here the app builds a jail with the *codebase* inside it
//! and the agent, unconfined, asks the jail. The agent is an ordinary child of this process with
//! inherited stdio — it has the real controlling terminal, so there is no PTY, no terminal bridge
//! and no OSC resize convention on this path.
//!
//! What makes the placement hold is three things and nothing else: the MCP server the agent spawns
//! dispatches over the socket this app serves ([`host_mcp_env`]); the agent's native filesystem and
//! shell tools are hard-disabled ([`build_host_agent_argv`]); and the checkout's own configuration
//! — its `.mcp.json` and its `.claude/settings.json` hooks, neither of which is a *tool* — is never
//! loaded, because the agent's working directory is a repository nobody audited.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// `RUST_LOG` for the host `tddy-tools --mcp` server when the session names no level, matching what
/// the in-jail server defaults to — the two are the same program answering the same tool calls, so
/// a session's logs should not change shape with the placement.
const DEFAULT_MCP_RUST_LOG: &str = "info,tddy_tools=debug,tddy_discovery=debug";

/// What a host-run agent is spawned with.
pub struct HostAgentArgs {
    pub claude_binary: String,
    pub model: String,
    pub permission_mode: String,
    pub session_id: String,
    /// Where the agent's MCP config is written.
    ///
    /// **This must be a directory no jail grant covers**, and in a `sandboxed` session that means
    /// the session's `host/` dir rather than anything under `sandbox/`. The file written here
    /// names the `command` and `env` an *unconfined* host process is launched with on every MCP
    /// (re)connect, so a jail that could rewrite it would not need to escape — this host would run
    /// its payload for it. The jail's own scratch tree is writable by the confined build by
    /// design; that is what makes it the one place this may never be.
    pub host_config_dir: PathBuf,
    pub tddy_tools_path: PathBuf,
    /// The socket this app serves, over which the agent's `tddy-tools --mcp` reaches the jail.
    pub tool_ipc_socket: PathBuf,
    /// Extra args forwarded verbatim to `claude`.
    pub claude_args: Vec<String>,
    /// `RUST_LOG` for the host `tddy-tools --mcp` server.
    pub mcp_log_level: Option<String>,
    /// Where the MCP server's own logs are persisted, when the session has an egress dir.
    pub egress_dir: Option<PathBuf>,
    /// Exec tools the session's subagent defs replace.
    pub replaced_tools: Vec<String>,
}

/// The `env` block for the host `tddy-tools --mcp` server.
///
/// `TDDY_SANDBOX_TOOL_IPC` is the whole mechanism: `tddy-tools` already selects
/// `SessionToolTransport::SandboxIpc` from it and speaks `ExecuteTool` over the socket. Who is on
/// the other end was never its concern — in every other mode that is the in-jail runner relaying
/// *outwards*, and here it is this app relaying *inwards*.
pub(crate) fn host_mcp_env(
    tool_ipc_socket: &Path,
    mcp_log_level: Option<&str>,
    egress_dir: Option<&Path>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "TDDY_SANDBOX_TOOL_IPC".to_string(),
        tool_ipc_socket.to_string_lossy().into_owned(),
    );
    // Same two files the in-jail server writes (`tddy_sandbox_runner::runner::spawn_claude_pty`),
    // under the same session egress dir: a dispatch that never reached the jail leaves a record
    // here instead of vanishing into the stderr the agent captured from its MCP child.
    if let Some(egress_dir) = egress_dir {
        env.insert(
            "TDDY_TOOLS_LOG_FILE".to_string(),
            egress_dir
                .join("tddy-tools.mcp.log")
                .to_string_lossy()
                .into_owned(),
        );
        env.insert(
            "TDDY_TOOLS_ACCOUNTING_FILE".to_string(),
            egress_dir
                .join("accounting.json")
                .to_string_lossy()
                .into_owned(),
        );
    }
    env.insert(
        "RUST_LOG".to_string(),
        mcp_log_level.unwrap_or(DEFAULT_MCP_RUST_LOG).to_string(),
    );
    env
}

/// Build the argv for the host-run agent, MCP block included.
///
/// The fixed flags are the shared `claude` base argv, so a host-run agent is configured the way
/// every other one is; what differs is the tail, where `append_host_agent_mcp_args` withdraws the
/// natives that would otherwise reach this host's filesystem directly.
pub fn build_host_agent_argv(args: HostAgentArgs) -> Result<Vec<String>> {
    let HostAgentArgs {
        claude_binary,
        model,
        permission_mode,
        session_id,
        host_config_dir,
        tddy_tools_path,
        tool_ipc_socket,
        claude_args,
        mcp_log_level,
        egress_dir,
        replaced_tools,
    } = args;

    let mut argv = tddy_core::claude_argv::build_claude_base_argv(
        &claude_binary,
        &model,
        &session_id,
        &permission_mode,
        false,
        false,
    );

    // The two flags that keep the *checkout's own* configuration from executing on this host, and
    // the only mitigations that reach it: neither of the things they block is a tool, so
    // `--disallowedTools` never sees either.
    //
    // `--strict-mcp-config` — use only the MCP servers named by `--mcp-config` below. A `.mcp.json`
    // in the repository would otherwise register servers that Claude launches as **unconfined host
    // processes**, with the repository's choice of command and env.
    //
    // `--setting-sources user` — load settings from this user's own home and from nowhere else,
    // dropping the `project` and `local` sources that read the checkout's `.claude/settings.json`.
    // That file can declare **hooks**: shell commands the CLI runs itself, on this host, around the
    // agent's turns. In every other mode the checkout's settings are read by an agent already
    // inside the jail; here the agent is not, and the premise of the mode is that the checkout is
    // unaudited — so a repository must not be able to hand this host a command to run.
    argv.push("--strict-mcp-config".to_string());
    argv.push("--setting-sources".to_string());
    argv.push("user".to_string());

    // Pass-through args go after the fixed flags but BEFORE the MCP block, which ends in the
    // variadic `--mcp-config`: a bare positional (a trailing prompt) placed after it would be
    // greedily swallowed as another config path. The in-jail spawn lives under the same rule.
    argv.extend(claude_args);

    let mcp_env = host_mcp_env(
        &tool_ipc_socket,
        mcp_log_level.as_deref(),
        egress_dir.as_deref(),
    );
    let replaced: Vec<&str> = replaced_tools.iter().map(String::as_str).collect();
    tddy_sandbox_recipes::append_host_agent_mcp_args(
        &mut argv,
        &host_config_dir,
        &tddy_tools_path,
        // The subagent tool surface is seeded through the *in-jail* `tddy-tools --mcp` process's
        // env, and this mode has no in-jail MCP server to seed (PRD § not in scope) — so no roster
        // is advertised. A def's `replaces:` still applies: a tool the session took away stays
        // taken away wherever the agent happens to run.
        false,
        &replaced,
        &mcp_env,
    )?;

    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ─── The host-run agent ─────────────────────────────────────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (criteria 7, 8)
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    const TOOL_SOCKET: &str = "/tmp/tddy-sandboxed-codebase.sock";

    fn a_host_agent(host_config_dir: &Path) -> HostAgentArgs {
        HostAgentArgs {
            claude_binary: "/usr/local/bin/claude".to_string(),
            model: "opus".to_string(),
            permission_mode: "auto".to_string(),
            session_id: "sandboxed-codebase-session".to_string(),
            host_config_dir: host_config_dir.to_path_buf(),
            tddy_tools_path: PathBuf::from("/opt/tddy/tddy-tools"),
            tool_ipc_socket: PathBuf::from(TOOL_SOCKET),
            claude_args: vec![],
            mcp_log_level: None,
            egress_dir: None,
            replaced_tools: vec![],
        }
    }

    fn value_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
        argv.iter()
            .position(|arg| arg == flag)
            .and_then(|at| argv.get(at + 1))
            .map(String::as_str)
    }

    fn values_after(argv: &[String], flag: &str) -> HashSet<String> {
        argv.windows(2)
            .filter(|w| w[0] == flag)
            .map(|w| w[1].clone())
            .collect()
    }

    /// The one line that makes the whole placement work: the agent's MCP server is told to dispatch
    /// over this app's socket, so every tool call lands in the jail instead of on this host.
    #[test]
    fn the_host_agents_mcp_server_dispatches_through_the_apps_tool_socket() {
        // Given / When
        let env = host_mcp_env(Path::new(TOOL_SOCKET), None, None);

        // Then
        assert_eq!(
            env.get("TDDY_SANDBOX_TOOL_IPC").map(String::as_str),
            Some(TOOL_SOCKET),
            "env was: {env:?}"
        );
    }

    /// The MCP server's own logs are persisted where the session's other egress lands, so a broken
    /// dispatch leaves evidence instead of vanishing into the agent's captured stderr.
    #[test]
    fn the_host_agents_mcp_server_persists_its_logs_under_the_session_egress_dir() {
        // Given
        let egress = PathBuf::from("/tmp/session/egress");

        // When
        let env = host_mcp_env(Path::new(TOOL_SOCKET), None, Some(&egress));

        // Then
        let log_file = env
            .get("TDDY_TOOLS_LOG_FILE")
            .expect("the MCP server's log file must be set when the session has an egress dir");
        assert!(
            log_file.starts_with("/tmp/session/egress"),
            "the log must land under the session egress dir; got: {log_file}"
        );
    }

    /// The agent keeps the full jailed tool surface.
    #[test]
    fn the_host_agent_keeps_every_mcp_exec_tool() {
        // Given
        let host_config = tempfile::tempdir().expect("host config tempdir");

        // When
        let argv =
            build_host_agent_argv(a_host_agent(host_config.path())).expect("argv must build");

        // Then
        let allowed = values_after(&argv, "--allowedTools");
        for tool in tddy_sandbox::workspace_exec_tool_names() {
            let prefixed = format!("mcp__tddy-tools__{tool}");
            assert!(
                allowed.contains(&prefixed),
                "the host agent must keep {prefixed}; allow-list was: {allowed:?}"
            );
        }
    }

    /// …and loses every native one. Unconfined on the host, a single surviving `Bash` or `Write`
    /// would make the jail decorative.
    #[test]
    fn the_host_agent_loses_every_native_filesystem_and_shell_tool() {
        // Given
        let host_config = tempfile::tempdir().expect("host config tempdir");

        // When
        let argv =
            build_host_agent_argv(a_host_agent(host_config.path())).expect("argv must build");

        // Then
        let disallowed = values_after(&argv, "--disallowedTools");
        for native in ["Read", "Write", "Edit", "Grep", "Glob", "Bash"] {
            assert!(
                disallowed.contains(native),
                "native {native} must be withdrawn; disallow-list was: {disallowed:?}"
            );
        }
    }

    /// A `.mcp.json` in the checkout would launch MCP servers as **unconfined host processes** the
    /// moment the agent starts in that directory. They are not tools, so no `--disallowedTools`
    /// entry touches them — and the premise of this mode is that the checkout is unaudited.
    #[test]
    fn the_host_agent_ignores_any_mcp_servers_the_untrusted_checkout_declares() {
        // Given
        let host_config = tempfile::tempdir().expect("host config tempdir");

        // When
        let argv =
            build_host_agent_argv(a_host_agent(host_config.path())).expect("argv must build");

        // Then
        assert!(
            argv.iter().any(|arg| arg == "--strict-mcp-config"),
            "the agent must use only the MCP config this app wrote; argv was: {argv:?}"
        );
    }

    /// The checkout's `.claude/settings.json` can declare **hooks** — shell commands the CLI runs
    /// itself, on this host, without going through a tool. Loading settings from the user's own
    /// home and nowhere else is what keeps the repository's from being read at all.
    #[test]
    fn the_host_agent_loads_settings_only_from_the_users_own_home() {
        // Given
        let host_config = tempfile::tempdir().expect("host config tempdir");

        // When
        let argv =
            build_host_agent_argv(a_host_agent(host_config.path())).expect("argv must build");

        // Then
        assert_eq!(
            value_after(&argv, "--setting-sources"),
            Some("user"),
            "neither `project` nor `local` may be loaded from an unaudited checkout; argv was: \
             {argv:?}"
        );
    }

    /// The MCP config the agent is pointed at registers `tddy-tools --mcp` carrying the socket, so
    /// the server the agent spawns is the one wired to the jail.
    #[test]
    fn the_host_agents_mcp_config_registers_tddy_tools_with_the_socket_in_its_env() {
        // Given
        let host_config = tempfile::tempdir().expect("host config tempdir");

        // When
        let argv =
            build_host_agent_argv(a_host_agent(host_config.path())).expect("argv must build");

        // Then
        let config_path = values_after(&argv, "--mcp-config")
            .into_iter()
            .next()
            .expect("the argv must register an MCP config");
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).expect("read MCP config"))
                .expect("the MCP config must be JSON");
        let server = &config["mcpServers"]["tddy-tools"];
        assert_eq!(
            server["env"]["TDDY_SANDBOX_TOOL_IPC"].as_str(),
            Some(TOOL_SOCKET),
            "config was: {config}"
        );
        assert_eq!(
            server["args"][0].as_str(),
            Some("--mcp"),
            "config was: {config}"
        );
    }

    /// Pass-through args land before the MCP block. `--mcp-config` is variadic, so a trailing
    /// positional prompt placed after it would be swallowed as another config path — the same
    /// ordering constraint the in-jail spawn already lives under.
    #[test]
    fn pass_through_agent_args_land_before_the_mcp_block() {
        // Given
        let host_config = tempfile::tempdir().expect("host config tempdir");
        let mut args = a_host_agent(host_config.path());
        args.claude_args = vec!["implement the feature".to_string()];

        // When
        let argv = build_host_agent_argv(args).expect("argv must build");

        // Then
        let prompt_at = argv
            .iter()
            .position(|arg| arg == "implement the feature")
            .expect("the pass-through prompt must be in the argv");
        let mcp_config_at = argv
            .iter()
            .position(|arg| arg == "--mcp-config")
            .expect("the argv must register an MCP config");
        assert!(
            prompt_at < mcp_config_at,
            "a trailing positional after --mcp-config is swallowed as a config path; argv was: {argv:?}"
        );
    }

    /// A subagent's replacements still apply: running on the host does not hand back a tool the
    /// session took away.
    #[test]
    fn the_host_agent_still_loses_a_tool_one_of_its_subagents_replaced() {
        // Given
        let host_config = tempfile::tempdir().expect("host config tempdir");
        let mut args = a_host_agent(host_config.path());
        args.replaced_tools = vec!["Grep".to_string()];

        // When
        let argv = build_host_agent_argv(args).expect("argv must build");

        // Then
        assert!(
            values_after(&argv, "--disallowedTools").contains("mcp__tddy-tools__Grep"),
            "argv was: {argv:?}"
        );
    }
}
