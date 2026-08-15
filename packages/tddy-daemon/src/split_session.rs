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
use tddy_github::{GitHubUser, SessionTokenError, SessionTokenSigner, TokenKind};
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

/// Mint the agent's own session token from the caller's, refusing anything the caller could not
/// legitimately have presented.
///
/// The web caller's access token lives [`tddy_github::SESSION_TOKEN_TTL`] — five minutes — because
/// the browser holds a refresh token and re-mints it long before expiry. A spawned agent holds
/// neither: whatever life was left on the caller's token when the session started is all its
/// `tddy-tools --mcp` child would ever have, and every remote tool call after that fails
/// `UNAUTHENTICATED` on the codebase daemon with nothing on this host to notice. So the agent is
/// given a credential of its own, scoped to the same [`SPLIT_AGENT_TOKEN_TTL`] as the join token
/// minted beside it — one agent process's working life, not the session's.
///
/// It is minted under the *verified* caller's identity, never the claimed one: the login in these
/// claims is what the codebase daemon looks up in its own `users[]` table to pick the OS user the
/// tools run as, so a token this daemon could not verify would be this daemon choosing a user on
/// another host's behalf. `livekit.api_secret` is the deployment-wide signing secret
/// (`auth::build_auth_entries`), which is exactly why the codebase daemon will accept what is minted
/// here. Every failure is a refusal rather than a fallback to forwarding the caller's token: an
/// expired, forged or malformed credential must not buy a session-length one.
fn mint_agent_session_token(api_secret: &str, caller_token: &str) -> Result<String, Status> {
    let signer = SessionTokenSigner::new(api_secret.as_bytes());
    let caller = verified_caller(&signer, caller_token)?;
    Ok(signer.mint(&caller, SPLIT_AGENT_TOKEN_TTL))
}

/// The identity a caller's token *proves*, or a refusal.
///
/// Shared by everything a split session start signs under the caller, because each of them
/// (the agent's own token, the room poller's per-poll credential) is this daemon asserting an
/// identity to another host: the login in the claims is what the codebase daemon looks up in its
/// own `users[]` table to pick the OS user, so a token this daemon could not verify would be this
/// daemon choosing a user on another host's behalf. Every failure is a refusal rather than a
/// fallback to forwarding the caller's token: an expired, forged or malformed credential must not
/// buy a minted one.
fn verified_caller(signer: &SessionTokenSigner, caller_token: &str) -> Result<GitHubUser, Status> {
    let claims = signer.verify(caller_token).map_err(|e| match e {
        SessionTokenError::Expired => Status::unauthenticated(
            "cannot wire a split session: the caller's session token has expired",
        ),
        SessionTokenError::InvalidSignature => Status::unauthenticated(
            "cannot wire a split session: the caller's session token is not signed by this \
             deployment's secret",
        ),
        SessionTokenError::Malformed => Status::unauthenticated(
            "cannot wire a split session: the caller's session token is malformed",
        ),
    })?;
    // A refresh token mints access tokens and never authenticates an RPC (see [`TokenKind`]), so
    // accepting one here would let the credential a browser keeps at rest authorize a whole
    // session's toolchain on the codebase host.
    if claims.kind == TokenKind::Refresh {
        return Err(Status::unauthenticated(
            "cannot wire a split session: the caller presented a refresh token, which never \
             authenticates an RPC",
        ));
    }
    Ok(GitHubUser {
        id: claims.id,
        login: claims.login,
        avatar_url: claims.avatar_url,
        name: claims.name,
    })
}

/// The credential the facilitating daemon's room poller presents to the codebase daemon, minted
/// fresh for every poll.
///
/// The room asks the codebase daemon for a worktree snapshot on a timer, and that peer
/// authenticates each one exactly as it authenticates a tool call. Holding the caller's token for
/// that would give the room five minutes ([`tddy_github::SESSION_TOKEN_TTL`]) of working life and
/// then a silent, permanent `Unauthenticated`. Unlike the agent — which lives in another process
/// and has to be handed something up front (see [`mint_agent_session_token`]) — the poller runs
/// inside the daemon that holds the signing secret, so it keeps the *identity* and signs a
/// short-lived token per poll: no expiry ceiling on the room, and no long-lived bearer token at
/// rest in this process.
pub struct RoomPollTokenMinter {
    signer: SessionTokenSigner,
    /// The verified caller, never the claimed one — [`verified_caller`].
    caller: GitHubUser,
}

impl RoomPollTokenMinter {
    /// Verify the caller once, here, so a session whose room could never authenticate anything
    /// fails to start rather than starting and then measuring nothing forever.
    pub fn new(api_secret: &str, caller_token: &str) -> Result<Self, Status> {
        let signer = SessionTokenSigner::new(api_secret.as_bytes());
        let caller = verified_caller(&signer, caller_token)?;
        Ok(Self { signer, caller })
    }
}

impl crate::session_room::SessionTokenMinter for RoomPollTokenMinter {
    /// [`tddy_github::SESSION_TOKEN_TTL`] and no longer: a poll that outlives its own credential is
    /// the bug this exists to remove, and the next poll mints another.
    fn mint(&self) -> String {
        self.signer.mint_access(&self.caller)
    }
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
///
/// The RPC server the agent addresses is the room's host, taken from the same `livekit` value that
/// names the room: only the daemon that hosts a room is in it, so the identity and the room have to
/// travel together. `codebase_instance_id` is not that identity — it hosts no room and joins none —
/// it is the forwarding hint the room's host routes on to reach the checkout.
///
/// `session_token` is the *caller's* credential and is never forwarded: it is proof of who asked,
/// and the agent gets one of its own minted from it (see [`mint_agent_session_token`]).
pub fn split_remote_tool_env(
    livekit: &SplitLiveKitRoom,
    session_id: &str,
    codebase_instance_id: &str,
    codebase_session_id: &str,
    session_token: &str,
) -> Result<RemoteToolEnv, Status> {
    let agent_session_token = mint_agent_session_token(&livekit.api_secret, session_token)?;
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
        session_token: agent_session_token,
        daemon_instance_id: Some(codebase_instance_id.to_string()),
        livekit_url: Some(livekit.url.clone()),
        livekit_room: Some(livekit.room.clone()),
        server_identity: Some(livekit.host_identity.clone()),
        livekit_token: Some(token),
    })
}

/// The room a split session's agent joins, and the credentials to mint its token.
pub struct SplitLiveKitRoom {
    pub room: String,
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    /// The participant identity serving RPC in `room` — the facilitating daemon's, since that is
    /// the daemon that opens the room and the only one that joins it. Carried beside the room name
    /// so an agent can never be pointed at a participant that is not in the room it was given.
    pub host_identity: String,
}

impl SplitLiveKitRoom {
    /// Resolve from daemon config for a named room, refusing a split placement the agent could not
    /// be wired for.
    ///
    /// The room is a parameter because it is no longer the lobby: the agent belongs in the room of
    /// the worktree it was given, so its token admits it there and nowhere else — not to the room
    /// every daemon and browser in the project shares. The credentials still come from the common
    /// room's resolution, since a daemon with no common room cannot forward `StartSession` to a
    /// peer in the first place, so a split placement is impossible before this is reached.
    ///
    /// Called before the codebase daemon is asked to create anything: a wiring failure discovered
    /// afterwards would strand a worktree on a host the operator may never look at.
    pub fn from_config(
        config: &crate::config::DaemonConfig,
        room: impl Into<String>,
    ) -> Result<Self, Status> {
        let (_common_room, url, api_key, api_secret) =
            crate::livekit_peer_discovery::livekit_common_room_connect_strings(config).map_err(
                |e| {
                    Status::failed_precondition(format!(
                        "cannot place a session's codebase on another daemon: {e}"
                    ))
                },
            )?;
        Ok(Self {
            room: room.into(),
            url,
            api_key,
            api_secret,
            host_identity: crate::livekit_peer_discovery::daemon_rpc_identity(
                &crate::livekit_peer_discovery::local_instance_id_for_config(config),
            ),
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
    // This session's own room, hosted by this daemon — the one running the agent, and therefore the
    // session's facilitating daemon. Named from `session_id`, never from `codebase_session_id`: the
    // codebase daemon hosts no room, so a room named after its session would be one nobody is in.
    // Start and resume derive it the same way, so a resumed agent rejoins the room it left.
    let livekit =
        SplitLiveKitRoom::from_config(config, crate::session_room::session_room_name(session_id))?;
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

    use std::time::SystemTime;

    use tddy_github::{GitHubUser, SessionClaims, SessionTokenSigner, SESSION_TOKEN_TTL};

    /// The session room as the facilitating daemon resolved it: its name, and the identity that
    /// daemon serves RPC on inside it. The two travel together because only the daemon that hosts a
    /// room can say who is in it.
    fn a_room_hosted_by(host_instance_id: &str) -> SplitLiveKitRoom {
        SplitLiveKitRoom {
            room: "session-agent-side-session".to_string(),
            url: "ws://livekit.invalid:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "secret".to_string(),
            host_identity: format!("daemon-{host_instance_id}"),
        }
    }

    fn a_room() -> SplitLiveKitRoom {
        a_room_hosted_by("workstation-a")
    }

    fn env_value(env: &RemoteToolEnv, key: &str) -> Option<String> {
        env.env_pairs()
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// The one secret every daemon in the deployment shares. It is `a_room()`'s `api_secret`, and
    /// `auth::build_auth_entries` signs session tokens with the very same value — which is what
    /// makes a token minted on the agent host verifiable on the codebase host.
    const SHARED_SIGNING_SECRET: &[u8] = b"secret";

    fn the_shared_signer() -> SessionTokenSigner {
        SessionTokenSigner::new(SHARED_SIGNING_SECRET)
    }

    /// The operator who pressed *start* in the browser. Their GitHub login is what the codebase
    /// daemon looks up in its own `users[]` table to decide which OS user the tools run as.
    fn a_signed_in_operator() -> GitHubUser {
        GitHubUser {
            id: 22_573_333,
            login: "uppin".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/22573333?v=4".to_string(),
            name: "Mantas Indrasius".to_string(),
        }
    }

    /// The credential the web caller presents on `StartSession`, with `seconds` of life left on it.
    ///
    /// The browser holds a refresh token and re-mints this well before expiry; a spawned agent
    /// holds neither, so whatever is left here is all it would ever get.
    fn a_caller_token_with_seconds_left(seconds: u64) -> String {
        the_shared_signer().mint(&a_signed_in_operator(), Duration::from_secs(seconds))
    }

    /// A caller token as freshly minted as one ever is — the full five minutes.
    fn a_caller_token() -> String {
        a_caller_token_with_seconds_left(SESSION_TOKEN_TTL.as_secs())
    }

    /// A caller token whose five minutes ran out ten minutes ago.
    fn an_expired_caller_token() -> String {
        the_shared_signer().mint_with_issued_at(
            &a_signed_in_operator(),
            SystemTime::now() - Duration::from_secs(900),
            SESSION_TOKEN_TTL,
        )
    }

    /// The long-lived credential the browser keeps to mint access tokens with. Never a valid RPC
    /// credential, so never a valid thing to wire a session's whole toolchain on.
    fn a_caller_refresh_token() -> String {
        the_shared_signer().mint_refresh(&a_signed_in_operator())
    }

    fn split_env_for_caller(caller_token: &str) -> Result<RemoteToolEnv, Status> {
        split_remote_tool_env(
            &a_room(),
            "agent-side-session",
            "workstation-b",
            "codebase-side-session",
            caller_token,
        )
    }

    fn the_agents_session_token(env: &RemoteToolEnv) -> String {
        env_value(env, "TDDY_REMOTE_SESSION_TOKEN")
            .expect("the agent must be given a session token")
    }

    /// The claims a codebase daemon holding the shared secret would read off the agent's token.
    fn claims_the_codebase_daemon_would_read(token: &str) -> SessionClaims {
        the_shared_signer()
            .verify(token)
            .expect("the agent's token must verify with the shared signing secret")
    }

    struct WiringRefusal(Status);

    fn assert_wiring_refused(result: Result<RemoteToolEnv, Status>) -> WiringRefusal {
        match result {
            Err(status) => WiringRefusal(status),
            Ok(_) => {
                panic!("expected the split wiring to be refused, but it produced an environment")
            }
        }
    }

    impl WiringRefusal {
        fn has_message_containing(self, fragment: &str) -> Self {
            let message = self.0.message().to_string();
            assert!(
                message.contains(fragment),
                "expected the refusal to mention '{fragment}'; got '{message}'"
            );
            self
        }
    }

    #[test]
    fn the_agents_session_token_outlives_the_callers_five_minute_access_token() {
        // Given a caller whose access token has 88 seconds left on it — the browser refreshes its
        // own well before expiry, but a spawned agent has no refresh token and no way to ask
        let caller_token = a_caller_token_with_seconds_left(88);

        // When
        let env = split_env_for_caller(&caller_token).expect("mint split env");

        // Then the agent is given a credential of its own, with the same life as the LiveKit join
        // token minted beside it. Copying the caller's instead leaves every remote tool call
        // failing UNAUTHENTICATED on the codebase daemon 88 seconds into the session.
        let claims = claims_the_codebase_daemon_would_read(&the_agents_session_token(&env));
        assert_eq!(claims.exp - claims.iat, SPLIT_AGENT_TOKEN_TTL.as_secs());
    }

    #[test]
    fn the_agent_gets_a_credential_of_its_own_under_the_callers_identity() {
        // Given
        let caller_token = a_caller_token();

        // When
        let env = split_env_for_caller(&caller_token).expect("mint split env");

        // Then the agent holds a token that is not the caller's — the caller's is refreshed in the
        // browser and dies in the agent's environment — minted under the same GitHub login, which
        // is what the codebase daemon looks up in its own `users[]` table to pick an OS user.
        let agent_token = the_agents_session_token(&env);
        assert_ne!(agent_token, caller_token);
        assert_eq!(
            claims_the_codebase_daemon_would_read(&agent_token).login,
            "uppin"
        );
    }

    #[test]
    fn wiring_is_refused_when_the_callers_token_has_already_expired() {
        // When
        let result = split_env_for_caller(&an_expired_caller_token());

        // Then — minting a session-length credential from an identity nobody proved would let an
        // expired login start a fully-privileged session on another host
        assert_wiring_refused(result).has_message_containing("expired");
    }

    #[test]
    fn wiring_is_refused_when_the_caller_presents_a_refresh_token() {
        // When
        let result = split_env_for_caller(&a_caller_refresh_token());

        // Then — a refresh token only mints access tokens and never authenticates an RPC.
        // Accepting one here would let the credential the browser keeps at rest authorize a
        // session's entire toolchain on the codebase host.
        assert_wiring_refused(result).has_message_containing("refresh");
    }

    #[test]
    fn the_agent_is_pointed_at_the_codebase_hosts_session_not_its_own() {
        // When
        let env = split_remote_tool_env(
            &a_room(),
            "agent-side-session",
            "workstation-b",
            "codebase-side-session",
            &a_caller_token(),
        )
        .expect("mint split env");

        // Then — the codebase daemon resolves the worktree from its own sessions base by this id, so
        // the agent's own session id would resolve to nothing there
        assert_eq!(
            env_value(&env, "TDDY_REMOTE_SESSION_ID").as_deref(),
            Some("codebase-side-session")
        );
    }

    #[test]
    fn the_agent_addresses_the_daemon_that_hosts_the_room_it_joins() {
        // Given the room this session's facilitating daemon opened and serves RPC in
        let room = a_room_hosted_by("workstation-a");

        // When the agent is wired for a checkout that lives on a different daemon entirely
        let env = split_remote_tool_env(
            &room,
            "agent-side-session",
            "workstation-b",
            "codebase-side-session",
            &a_caller_token(),
        )
        .expect("mint split env");

        // Then it addresses the daemon that is in that room. The codebase daemon hosts no room and
        // joins none, so naming it here leaves the agent calling a participant that never arrives:
        // every tool call waits out its timeout instead of reading a file.
        assert_eq!(
            env_value(&env, "TDDY_REMOTE_SERVER_IDENTITY").as_deref(),
            Some("daemon-workstation-a")
        );
    }

    #[test]
    fn the_codebase_daemon_is_named_as_the_forwarding_destination_not_as_the_rpc_server() {
        // Given the same room, hosted by the daemon running the agent
        let room = a_room_hosted_by("workstation-a");

        // When the agent is wired for a checkout on `workstation-b`
        let env = split_remote_tool_env(
            &room,
            "agent-side-session",
            "workstation-b",
            "codebase-side-session",
            &a_caller_token(),
        )
        .expect("mint split env");

        // Then the codebase host is still named — as the hop the facilitating daemon forwards to.
        // Addressing the room's host is only half of the route: the daemon that answers there holds
        // no checkout, and `ExecuteTool` routes on this id to reach the one that does.
        assert_eq!(
            env_value(&env, "TDDY_REMOTE_DAEMON_INSTANCE_ID").as_deref(),
            Some("workstation-b")
        );
    }

    #[test]
    fn the_agent_receives_a_scoped_join_token_and_never_the_api_secret() {
        // When
        let env = split_remote_tool_env(
            &a_room(),
            "sid",
            "workstation-b",
            "codebase-sid",
            &a_caller_token(),
        )
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
