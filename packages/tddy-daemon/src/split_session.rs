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

/// Identity prefix reserved for split sessions' agent participants.
///
/// Reserved, not merely conventional: peer eligibility is decided from self-declared participant
/// metadata, so this prefix is what
/// `livekit_peer_discovery::eligible_daemon_from_participant_fields` matches on to refuse an agent
/// advertising itself as a daemon. A daemon whose `daemon_instance_id` began with it would not be
/// discoverable — which is the intended trade, since the agent holds a token it can publish
/// metadata with and the daemon's instance id is an operator's free choice.
pub const SPLIT_AGENT_IDENTITY_PREFIX: &str = "split-agent-";

/// The LiveKit participant identity a split session's agent joins the common room under.
///
/// Session-scoped and distinct from both daemon identities (`daemon-…`) and the bare instance ids
/// the discovery participants use, so the token grants exactly one agent's presence and an operator
/// reading the room roster can tell whose it is.
pub fn split_agent_participant_identity(session_id: &str) -> String {
    format!("{SPLIT_AGENT_IDENTITY_PREFIX}{session_id}")
}

/// The codebase daemon and the workspace session on it that a session is paired with, or `None`
/// when the session is co-located.
///
/// The pairing is the *pair* — a recorded daemon with no session id names a host but nothing on it
/// to resume, re-wire or delete, so half a pairing is read as none rather than acted on. Every
/// caller needs both, so the check lives here instead of at each of them.
pub fn split_pairing(meta: &tddy_core::SessionMetadata) -> Option<(&str, &str)> {
    fn non_blank(field: &Option<String>) -> Option<&str> {
        field.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }
    Some((
        non_blank(&meta.codebase_daemon_instance_id)?,
        non_blank(&meta.codebase_session_id)?,
    ))
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

/// Filename of the MCP server's log, under the session directory.
///
/// Same basename the sandbox runner writes into its egress dir (`tddy-sandbox-runner`): a split
/// session's session dir is its equivalent — the per-session place the host can read afterwards.
const MCP_LOG_BASENAME: &str = "tddy-tools.mcp.log";

/// `RUST_LOG` for the agent's `tddy-tools --mcp` child.
///
/// Mirrors the sandbox runner's default, minus its `tddy_discovery=debug` — that one exists for
/// specialized subagents' HTTP activity, which a split session has none of. `tddy_tools=debug` is
/// the part that matters here: it is where a failed LiveKit dispatch to the codebase daemon (room
/// connect refused, peer absent, truncated stream) is reported.
const MCP_RUST_LOG: &str = "info,tddy_tools=debug";

/// Where a split session's MCP server writes its log.
pub fn split_mcp_log_path(session_dir: &Path) -> PathBuf {
    session_dir.join(MCP_LOG_BASENAME)
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
    // Every tool call a split session makes crosses LiveKit to the codebase daemon, and every way
    // that can fail is reported by `tddy-tools` itself. Claude Code captures an MCP server's stderr,
    // so without a log file those reports exist only inside a process nobody can attach to: a split
    // session whose dispatch is failing would leave no evidence on either daemon. The sandbox path
    // solves this the same way, pointing the same variable at its egress dir.
    let mcp_env = BTreeMap::from([
        (
            "TDDY_TOOLS_LOG_FILE".to_string(),
            split_mcp_log_path(session_dir)
                .to_string_lossy()
                .into_owned(),
        ),
        ("RUST_LOG".to_string(), MCP_RUST_LOG.to_string()),
    ]);
    let mcp_config = tddy_sandbox_recipes::write_claude_mcp_config(
        session_dir,
        Path::new(tddy_tools_path),
        &mcp_env,
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
    // `--mcp-config` alone *adds* to the user-scoped MCP configuration, so any filesystem or shell
    // MCP server the operator has configured would load beside `tddy-tools` — reachable under an
    // `mcp__*` name the disallowlist above does not cover, on this host rather than the codebase
    // host. The restriction the split placement rests on has to be impossible to route around, not
    // merely the default, so this config is the only one loaded.
    args.push("--strict-mcp-config".to_string());
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
    fn no_mcp_server_but_tddy_tools_is_loaded_for_the_agent() {
        // Given
        let tmp = tempfile::tempdir().unwrap();

        // When
        let args = split_claude_extra_args(tmp.path(), "/usr/bin/tddy-tools").expect("extra args");

        // Then — without this, Claude Code merges the user-scoped MCP configuration on top of ours,
        // and a filesystem or shell server configured there would run beside tddy-tools, outside
        // the disallowlist above: the restriction the split placement rests on would be advice
        assert!(
            args.iter().any(|a| a == "--strict-mcp-config"),
            "the MCP config must be the only one loaded; got {args:?}"
        );
    }

    /// Exec tools whose name is *not* also a Claude Code built-in, so [`NATIVE_FILESYSTEM_TOOLS`]
    /// has nothing to disallow for them: they are reachable only in their `mcp__tddy-tools__` form,
    /// which is the form a split session wants.
    ///
    /// Listed rather than inferred so that a **new** exec tool fails the test below until someone
    /// decides which side of the line it falls on. That decision is the whole point: an exec tool
    /// that shares a name with a Claude built-in (as `Read` and `Grep` do) needs the built-in hard
    /// disabled, or the agent gets this host's filesystem instead of the codebase host's.
    const EXEC_TOOLS_WITH_NO_CLAUDE_BUILT_IN: &[&str] = &[
        "StrReplace",
        "Delete",
        "Shell",
        "Await",
        "ReadLints",
        "SemanticSearch",
    ];

    /// The native Claude built-ins `tddy-sandbox-recipes` knows an exec tool by, other than the
    /// exec tool's own name: `Bash`/`BashOutput`/`KillShell` for `Shell`, `Edit`/`MultiEdit`/
    /// `NotebookEdit` for `Write`. Read out of the public disallowlist builder, which is where that
    /// knowledge lives, so this test tracks it rather than restating it.
    fn native_aliases_the_sandbox_recipes_know() -> Vec<String> {
        let exec_tools = tddy_sandbox::workspace_exec_tool_names();
        tddy_sandbox_recipes::build_claude_disallowlist(exec_tools)
            .into_iter()
            // The `mcp__tddy-tools__` forms are the proxied tools themselves — the split session's
            // only route to the codebase, and the one thing it must *not* disallow.
            .filter(|tool| !tool.starts_with("mcp__"))
            .filter(|tool| !exec_tools.contains(&tool.as_str()))
            .collect()
    }

    #[test]
    fn the_split_disallowlist_covers_every_native_alias_the_sandbox_recipes_know() {
        // Given the aliases the sandbox path hard-disables when it replaces a tool
        let aliases = native_aliases_the_sandbox_recipes_know();
        assert!(
            !aliases.is_empty(),
            "the sandbox recipes must still expose native aliases, or this test proves nothing"
        );

        // Then — the two lists answer the same question about Claude's own tool inventory, from two
        // crates. A built-in added upstream to one and not the other would silently leave a split
        // agent a native route to *this* host's filesystem, which is the one thing the placement
        // forbids and the only thing enforcing it.
        for alias in &aliases {
            assert!(
                NATIVE_FILESYSTEM_TOOLS.contains(&alias.as_str()),
                "native {alias} is hard-disabled by the sandbox recipes but not by a split session; add it to NATIVE_FILESYSTEM_TOOLS"
            );
        }
    }

    #[test]
    fn every_exec_tool_sharing_its_name_with_a_claude_built_in_is_hard_disabled() {
        for tool in tddy_sandbox::workspace_exec_tool_names() {
            if EXEC_TOOLS_WITH_NO_CLAUDE_BUILT_IN.contains(tool) {
                continue;
            }
            // Then — the proxied `mcp__tddy-tools__Read` stays allowed, but Claude's own `Read`
            // would open a file on the agent host, where the repository does not exist
            assert!(
                NATIVE_FILESYSTEM_TOOLS.contains(tool),
                "exec tool {tool} names a Claude built-in that a split session leaves reachable; \
                 add it to NATIVE_FILESYSTEM_TOOLS, or to EXEC_TOOLS_WITH_NO_CLAUDE_BUILT_IN if \
                 Claude has no tool by that name"
            );
        }
    }

    #[test]
    fn the_agents_tool_server_logs_to_a_file_under_the_session_dir() {
        // Given
        let tmp = tempfile::tempdir().unwrap();

        // When
        let args = split_claude_extra_args(tmp.path(), "/usr/bin/tddy-tools").expect("extra args");

        // Then — every remote tool call's failures (room connect, peer absent, truncated stream)
        // are reported by tddy-tools, and Claude Code captures an MCP server's stderr: without a
        // log file a split session whose dispatch is failing leaves no evidence on either daemon
        let config_path = args
            .windows(2)
            .find(|w| w[0] == "--mcp-config")
            .map(|w| w[1].clone())
            .expect("an --mcp-config path");
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_path).expect("read MCP config"))
                .expect("MCP config must be JSON");
        let env = &config["mcpServers"]["tddy-tools"]["env"];
        assert_eq!(
            env["TDDY_TOOLS_LOG_FILE"].as_str(),
            Some(
                split_mcp_log_path(tmp.path())
                    .to_string_lossy()
                    .as_ref()
                    .to_owned()
            )
            .as_deref(),
            "the MCP server's log must land in the session dir; got {env}"
        );
        assert!(
            env["RUST_LOG"]
                .as_str()
                .is_some_and(|level| level.contains("tddy_tools=debug")),
            "the dispatch failures worth reading are logged by tddy_tools; got {env}"
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
