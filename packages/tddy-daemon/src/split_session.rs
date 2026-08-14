//! Wiring for a **split** session: the agent runs on this daemon, its git worktree lives on another.
//!
//! PRD: `docs/ft/daemon/remote-managed-worktree.md`.
//!
//! There is no repository on this host, so the agent gets a context directory instead of a worktree,
//! an allowlist that leaves it no native filesystem tool, and a `TDDY_REMOTE_*` environment pointing
//! `tddy-tools --mcp` at the *codebase* daemon over LiveKit. Everything here is per-spawn and is
//! rebuilt on resume — notably the join token, which is scoped to a lifetime that may have elapsed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_core::backend::RemoteToolEnv;
use tddy_rpc::Status;

/// Lifetime of the agent's scoped LiveKit join token.
///
/// Matches the per-session PTY bridge's token (`cli_session_manager::spawn_livekit_bridge`): long
/// enough that a working day of tool calls never re-authenticates, short enough that a leaked token
/// expires. A resumed session mints a fresh one rather than reusing what was persisted, so this is a
/// ceiling on one agent process's life, not on the session's.
const SPLIT_AGENT_TOKEN_TTL: Duration = Duration::from_secs(86_400);

/// Claude's own filesystem and shell tools, hard-disabled for a split session.
///
/// `--allowedTools` alone only removes a tool's pre-approval: the native built-in stays callable
/// through the permission prompt. On this host those tools would operate on the context directory —
/// which is not the codebase — so every one of them is named in `--disallowedTools`, which takes
/// precedence and removes them outright. This is the enforcement the PRD's "claude-cli only"
/// restriction rests on (§ Why claude-cli only).
const NATIVE_FILESYSTEM_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "Bash",
    "BashOutput",
    "KillShell",
    "Glob",
    "Grep",
    "LS",
];

/// The MCP tool Claude asks before running anything not pre-approved. Served by the agent's own
/// `tddy-tools --mcp` child, the same one that carries the exec tools to the codebase daemon.
const PERMISSION_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";

/// Everything a split session's agent process needs to reach a worktree on another daemon.
pub struct SplitAgentWiring {
    /// Directory the agent runs in, standing in for the repository it does not have.
    pub context_dir: PathBuf,
    /// `claude` flags: the `mcp__tddy-tools__*` allowlist, the native-tool disallowlist, the
    /// permission-prompt tool, and the MCP config registering `tddy-tools --mcp`.
    pub extra_args: Vec<String>,
    /// `TDDY_REMOTE_*` pairs selecting the LiveKit transport in `tddy-tools`.
    pub env: Vec<(String, String)>,
}

/// The LiveKit participant identity a split session's agent joins the common room under.
///
/// Session-scoped and distinct from both daemon identities (`daemon-…`) and the bare instance ids
/// the discovery participants use, so the token grants exactly one agent's presence and an operator
/// reading the room roster can tell whose it is.
pub fn split_agent_participant_identity(session_id: &str) -> String {
    format!("split-agent-{session_id}")
}

/// Where a split session's context directory lives: inside the session directory, so it is removed
/// with the session and needs no separate lifetime.
pub fn split_context_dir(session_dir: &Path) -> PathBuf {
    session_dir.join("context")
}

/// Build the agent's working directory.
///
/// The agent has no repository here, so the directory carries the managed-codebase notice telling it
/// the codebase is elsewhere and reachable only through `mcp__tddy-tools__*`. Unlike the sandboxed
/// context dir, it is not made read-only: there the jail mounts it read-only, while here what keeps
/// the agent out of it is the tool disallowlist, and `claude` still needs a writable cwd.
///
/// TODO(remote-managed-worktree): sync the codebase host's `CLAUDE.md` / `AGENTS.md` / skills into
/// this directory. Until then a split session's agent sees the notice but not the project's own
/// guidance, which the co-located managed-codebase path does copy from the worktree.
pub fn build_split_context_dir(session_dir: &Path) -> Result<PathBuf, Status> {
    let context_dir = split_context_dir(session_dir);
    std::fs::create_dir_all(&context_dir)
        .map_err(|e| Status::internal(format!("failed to create split context dir: {e}")))?;
    std::fs::write(
        context_dir.join("CLAUDE.md"),
        tddy_sandbox::SANDBOX_REMOTE_APPENDIX.trim_start(),
    )
    .map_err(|e| Status::internal(format!("failed to write split context CLAUDE.md: {e}")))?;
    Ok(context_dir)
}

/// Build the `claude` flags that leave the agent no route to this host's filesystem and point its
/// MCP server at `tddy-tools`.
///
/// The MCP config is written under `session_dir` rather than the context directory so the agent's
/// cwd holds only guidance.
pub fn split_claude_extra_args(
    session_dir: &Path,
    tddy_tools_path: &str,
) -> Result<Vec<String>, Status> {
    let mcp_config = tddy_sandbox_recipes::write_claude_mcp_config(
        session_dir,
        Path::new(tddy_tools_path),
        &BTreeMap::new(),
    )
    .map_err(|e| Status::internal(format!("failed to write MCP config: {e}")))?;

    let mut args = Vec::new();
    // Nothing is replaced by a subagent here: the exec tools are all reachable, they simply execute
    // on the codebase daemon.
    for tool in tddy_sandbox_recipes::build_claude_allowlist(false, &[]) {
        args.push("--allowedTools".to_string());
        args.push(tool);
    }
    for tool in NATIVE_FILESYSTEM_TOOLS {
        args.push("--disallowedTools".to_string());
        args.push((*tool).to_string());
    }
    args.push("--permission-prompt-tool".to_string());
    args.push(PERMISSION_PROMPT_TOOL.to_string());
    args.push("--mcp-config".to_string());
    args.push(mcp_config.to_string_lossy().into_owned());
    Ok(args)
}

/// Mint the agent's scoped LiveKit join token and build the `TDDY_REMOTE_*` environment around it.
///
/// `codebase_session_id` is the **workspace session on the codebase daemon**, not this session:
/// that daemon resolves the worktree from its own sessions base keyed by the id it is given, so the
/// agent's own id would find nothing there.
///
/// The token grants room-join under one pinned identity for one room. The daemon's
/// `livekit.api_secret` is deliberately not exported — it would let an agent running model-authored
/// code mint a token for any room as any identity (contrast `spawner.rs`, which passes it on the
/// command line to `tddy-coder`, where `/proc/<pid>/cmdline` exposes it).
pub fn split_remote_tool_env(
    livekit: &SplitLiveKitRoom,
    session_id: &str,
    codebase_instance_id: &str,
    codebase_session_id: &str,
    session_token: &str,
) -> Result<RemoteToolEnv, Status> {
    let identity = split_agent_participant_identity(session_id);
    let token = tddy_livekit::TokenGenerator::new(
        livekit.api_key.clone(),
        livekit.api_secret.clone(),
        livekit.room.clone(),
        identity,
        SPLIT_AGENT_TOKEN_TTL,
    )
    .generate()
    .map_err(|e| Status::internal(format!("mint LiveKit join token for split session: {e}")))?;

    Ok(RemoteToolEnv {
        // A split session has no HTTP route to its worktree: this daemon's own URL would answer,
        // but from the wrong host's filesystem. Left empty so the LiveKit transport is the only one
        // configured rather than a wrong one waiting behind it.
        daemon_url: String::new(),
        session_id: codebase_session_id.to_string(),
        session_token: session_token.to_string(),
        daemon_instance_id: Some(codebase_instance_id.to_string()),
        livekit_url: Some(livekit.url.clone()),
        livekit_room: Some(livekit.room.clone()),
        server_identity: Some(crate::livekit_peer_discovery::daemon_rpc_identity(
            codebase_instance_id,
        )),
        livekit_token: Some(token),
    })
}

/// The common room a split session's agent joins, and the credentials to mint its token.
pub struct SplitLiveKitRoom {
    pub room: String,
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
}

impl SplitLiveKitRoom {
    /// Resolve from daemon config, refusing a split placement the agent could not be wired for.
    ///
    /// Called before the codebase daemon is asked to create anything: a wiring failure discovered
    /// afterwards would strand a worktree on a host the operator may never look at.
    pub fn from_config(config: &crate::config::DaemonConfig) -> Result<Self, Status> {
        let (room, url, api_key, api_secret) =
            crate::livekit_peer_discovery::livekit_common_room_connect_strings(config).map_err(
                |e| {
                    Status::failed_precondition(format!(
                        "cannot place a session's codebase on another daemon: {e}"
                    ))
                },
            )?;
        Ok(Self {
            room,
            url,
            api_key,
            api_secret,
        })
    }
}

/// Assemble everything the agent spawn needs, minting a fresh join token.
pub fn prepare_split_agent_wiring(
    config: &crate::config::DaemonConfig,
    session_dir: &Path,
    tddy_tools_path: &str,
    session_id: &str,
    codebase_instance_id: &str,
    codebase_session_id: &str,
    session_token: &str,
) -> Result<SplitAgentWiring, Status> {
    let livekit = SplitLiveKitRoom::from_config(config)?;
    let remote = split_remote_tool_env(
        &livekit,
        session_id,
        codebase_instance_id,
        codebase_session_id,
        session_token,
    )?;
    Ok(SplitAgentWiring {
        context_dir: build_split_context_dir(session_dir)?,
        extra_args: split_claude_extra_args(session_dir, tddy_tools_path)?,
        env: remote.env_pairs(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_room() -> SplitLiveKitRoom {
        SplitLiveKitRoom {
            room: "tddy-lobby".to_string(),
            url: "ws://livekit.invalid:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "secret".to_string(),
        }
    }

    fn env_value(env: &RemoteToolEnv, key: &str) -> Option<String> {
        env.env_pairs()
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    #[test]
    fn the_agent_is_pointed_at_the_codebase_hosts_session_not_its_own() {
        // When
        let env = split_remote_tool_env(
            &a_room(),
            "agent-side-session",
            "workstation-b",
            "codebase-side-session",
            "token",
        )
        .expect("mint split env");

        // Then — the codebase daemon resolves the worktree from its own sessions base by this id, so
        // the agent's own session id would resolve to nothing there
        assert_eq!(
            env_value(&env, "TDDY_REMOTE_SESSION_ID").as_deref(),
            Some("codebase-side-session")
        );
        assert_eq!(
            env_value(&env, "TDDY_REMOTE_SERVER_IDENTITY").as_deref(),
            Some("daemon-workstation-b"),
            "tools must address the peer's RPC participant, not its discovery identity"
        );
    }

    #[test]
    fn the_agent_receives_a_scoped_join_token_and_never_the_api_secret() {
        // When
        let env = split_remote_tool_env(&a_room(), "sid", "workstation-b", "codebase-sid", "token")
            .expect("mint split env");

        // Then — a JWT, not the secret that mints JWTs for any room and identity
        let token = env_value(&env, "TDDY_REMOTE_LIVEKIT_TOKEN").expect("join token");
        assert!(
            token.starts_with("eyJ"),
            "expected a signed JWT; got {token:?}"
        );
        assert!(
            !env.env_pairs().iter().any(|(_, v)| v == "secret"),
            "the daemon's api_secret must never reach the agent's environment"
        );
    }

    #[test]
    fn every_native_filesystem_tool_is_hard_disabled() {
        // Given
        let tmp = tempfile::tempdir().unwrap();

        // When
        let args = split_claude_extra_args(tmp.path(), "/usr/bin/tddy-tools").expect("extra args");

        // Then — dropping a tool from --allowedTools only un-pre-approves it; --disallowedTools is
        // what makes a native Read or Bash impossible rather than merely discouraged
        let disallowed: Vec<&str> = args
            .windows(2)
            .filter(|w| w[0] == "--disallowedTools")
            .map(|w| w[1].as_str())
            .collect();
        for tool in ["Read", "Write", "Bash", "Grep", "Glob"] {
            assert!(
                disallowed.contains(&tool),
                "native {tool} must be disallowed; got {disallowed:?}"
            );
        }
        // And the proxied forms stay reachable — they are the only way to the codebase
        let allowed: Vec<&str> = args
            .windows(2)
            .filter(|w| w[0] == "--allowedTools")
            .map(|w| w[1].as_str())
            .collect();
        assert!(
            allowed.contains(&"mcp__tddy-tools__Read"),
            "the proxied Read must remain allowed; got {allowed:?}"
        );
    }

    #[test]
    fn the_context_dir_tells_the_agent_its_codebase_is_elsewhere() {
        // Given
        let tmp = tempfile::tempdir().unwrap();

        // When
        let context = build_split_context_dir(tmp.path()).expect("context dir");

        // Then — an agent that opened its cwd expecting a repository learns why it is empty
        let claude_md = std::fs::read_to_string(context.join("CLAUDE.md")).expect("CLAUDE.md");
        assert!(
            claude_md.contains("mcp__tddy-tools__"),
            "the notice must name the tools that reach the real codebase; got:\n{claude_md}"
        );
    }
}
