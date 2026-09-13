//! `pty-relay` subcommand: spawn a command in a PTY and relay stdin/stdout, OR connect to an
//! existing daemon session via gRPC or LiveKit.
//!
//! Local PTY mode (default):
//!   tddy-tools pty-relay -- claude --model opus
//!
//! Start sandboxed claude-cli and attach your terminal (gRPC, no LiveKit):
//!   tddy-tools pty-relay \
//!     --daemon-url http://127.0.0.1:8899 \
//!     --project-id <project-id> \
//!     --sandbox
//!
//! Connect to an existing session (including sandbox):
//!   tddy-tools pty-relay \
//!     --daemon-url http://127.0.0.1:8899 \
//!     --session-id <session-id> \
//!     --session-token <token>
//!
//! LiveKit connect-only mode (--server-identity, requires --features livekit):
//!   tddy-tools pty-relay \
//!     --livekit-url ws://127.0.0.1:7880 \
//!     --livekit-room tddy-lobby --server-identity daemon-udoo-…-<session_id>
//!
//! LiveKit start-and-connect mode (--daemon-identity, requires --features livekit):
//!   tddy-tools pty-relay \
//!     --livekit-url ws://127.0.0.1:7880 \
//!     --livekit-room tddy-lobby \
//!     --daemon-identity udoo-1780828020298 \
//!     --project-id <id>
//!
//! # Where the relay itself lives
//!
//! `tddy_terminal_rpc::pty_relay` — it already owned the local PTY runtime this subcommand
//! delegates into, and **PR #475 (`#unbundle` node 6) serves the terminal session RPCs from that
//! crate**. What is left here is the clap surface: the twenty `#[arg]`s below, their defaults, and
//! the one function that turns them into a [`tddy_terminal_rpc::pty_relay::PtyRelayConfig`].
//! `tddy-terminal-rpc` has no `clap` and gains none.

use anyhow::Result;
use clap::Args;

use tddy_terminal_rpc::pty_relay::PtyRelayConfig;

#[derive(Args)]
pub struct PtyRelayArgs {
    /// Working directory for the spawned command (default: current directory).
    #[arg(short = 'C', long = "dir", default_value = ".")]
    pub dir: std::path::PathBuf,

    // -- LiveKit shared args (requires --features livekit) --------------------
    /// LiveKit server URL (e.g. ws://127.0.0.1:7880). Enables LiveKit mode.
    #[arg(long)]
    pub livekit_url: Option<String>,

    /// LiveKit API key for token generation.
    #[arg(long, default_value = "devkey")]
    pub livekit_api_key: String,

    /// LiveKit API secret for token generation.
    #[arg(long, default_value = "secret")]
    pub livekit_api_secret: String,

    /// LiveKit room name to join (common room, e.g. tddy-lobby).
    #[arg(long)]
    pub livekit_room: Option<String>,

    /// Local participant identity in the room.
    #[arg(long, default_value = "pty-relay-client")]
    pub client_identity: String,

    // -- Connect-only mode (--server-identity) --------------------------------
    /// Connect to an already-running session's terminal server. The identity is
    /// `daemon-<instance_id>-<session_id>` (from StartSessionResponse or daemon logs).
    #[arg(long)]
    pub server_identity: Option<String>,

    // -- Start-and-connect mode (--daemon-identity) ---------------------------
    /// Daemon's own LiveKit identity in the common room (e.g. udoo-1780828020298).
    /// Triggers start-and-connect: calls StartSession via LiveKit RPC, then connects terminal.
    #[arg(long)]
    pub daemon_identity: Option<String>,

    /// Daemon HTTP base URL for auth (default: http://127.0.0.1:8899).
    /// Used to auto-exchange a session token when --session-token is not provided.
    #[arg(long, default_value = "http://127.0.0.1:8899")]
    pub daemon_url: String,

    /// Session token for StartSession auth. When omitted, pty-relay calls the daemon's
    /// auth.AuthService to exchange a stub token automatically (works with stub auth).
    #[arg(long)]
    pub session_token: Option<String>,

    /// Project ID for the new session.
    #[arg(long)]
    pub project_id: Option<String>,

    /// Agent for the new session (optional).
    #[arg(long)]
    pub agent: Option<String>,

    /// Model for claude-cli sessions. Defaults to the versionless `opus` alias so a relay started
    /// today runs the current Opus; pass a pinned id (e.g. `claude-opus-5`) to fix a generation.
    #[arg(long, default_value = tddy_core::backend::CLAUDE_DEFAULT_MODEL)]
    pub model: String,

    /// Session type: "claude-cli" (default) or empty for a tool session.
    #[arg(long, default_value = "claude-cli")]
    pub session_type: String,

    /// Seed the first user prompt for the new session (e.g. "opusplan").
    /// Passed as a positional argument to `claude` so it runs immediately on start.
    #[arg(long)]
    pub initial_prompt: Option<String>,

    /// Permission mode for claude-cli sessions (e.g. "auto", "bypassPermissions", "plan").
    /// Passed as `--permission-mode <mode>` to the claude binary. Empty defaults to "auto".
    #[arg(long)]
    pub permission_mode: Option<String>,

    /// Start claude-cli inside darwin Seatbelt (`StartSessionRequest.sandbox = true`, macOS only).
    #[arg(long, default_value_t = false)]
    pub sandbox: bool,

    /// Connect to an existing session via gRPC (`StreamTerminalOutput` / `SendTerminalInput`).
    /// Mutually exclusive with `--project-id` (start-and-connect).
    #[arg(long)]
    pub session_id: Option<String>,

    // -- Local PTY mode -------------------------------------------------------
    /// Command and arguments to relay in local PTY mode (after `--`).
    #[arg(last = true)]
    pub cmd: Vec<String>,
}

/// Run the `pty-relay` subcommand: hand the parsed arguments to the relay in
/// `tddy-terminal-rpc`, which picks its dispatch mode from which of them are set.
pub async fn run_pty_relay(args: PtyRelayArgs) -> Result<()> {
    tddy_terminal_rpc::pty_relay::run_pty_relay(args.into_config()).await
}

impl PtyRelayArgs {
    fn into_config(self) -> PtyRelayConfig {
        PtyRelayConfig {
            dir: self.dir,
            livekit_url: self.livekit_url,
            livekit_api_key: self.livekit_api_key,
            livekit_api_secret: self.livekit_api_secret,
            livekit_room: self.livekit_room,
            client_identity: self.client_identity,
            server_identity: self.server_identity,
            daemon_identity: self.daemon_identity,
            daemon_url: self.daemon_url,
            session_token: self.session_token,
            project_id: self.project_id,
            agent: self.agent,
            model: self.model,
            session_type: self.session_type,
            initial_prompt: self.initial_prompt,
            permission_mode: self.permission_mode,
            sandbox: self.sandbox,
            session_id: self.session_id,
            cmd: self.cmd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Every `#[arg]` default is declared here and nowhere else, so a parse with no flags is the
    /// only place they can be read back — and the config the relay receives must carry them.
    #[derive(Parser)]
    struct OnlyPtyRelay {
        #[command(flatten)]
        pty_relay: PtyRelayArgs,
    }

    #[test]
    fn a_bare_invocation_hands_the_relay_every_declared_default() {
        // Given / When
        let config = OnlyPtyRelay::parse_from(["pty-relay"])
            .pty_relay
            .into_config();

        // Then
        assert_eq!(config.dir, std::path::PathBuf::from("."));
        assert_eq!(config.livekit_api_key, "devkey");
        assert_eq!(config.livekit_api_secret, "secret");
        assert_eq!(config.client_identity, "pty-relay-client");
        assert_eq!(config.daemon_url, "http://127.0.0.1:8899");
        assert_eq!(config.model, tddy_core::backend::CLAUDE_DEFAULT_MODEL);
        assert_eq!(config.session_type, "claude-cli");
        assert!(!config.sandbox);
    }

    /// The flags that select a dispatch mode must survive the hand-off, or the relay picks a mode
    /// the operator did not ask for.
    #[test]
    fn the_flags_that_select_a_dispatch_mode_survive_the_hand_off() {
        // Given / When
        let config = OnlyPtyRelay::parse_from([
            "pty-relay",
            "--project-id",
            "proj-1",
            "--sandbox",
            "--",
            "claude",
            "--model",
            "opus",
        ])
        .pty_relay
        .into_config();

        // Then
        assert_eq!(config.project_id.as_deref(), Some("proj-1"));
        assert!(config.sandbox);
        assert_eq!(config.cmd, vec!["claude", "--model", "opus"]);
        assert_eq!(config.session_id, None);
    }
}
