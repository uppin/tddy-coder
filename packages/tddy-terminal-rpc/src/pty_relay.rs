//! Attaching a terminal from outside the daemon, over whichever transport is available.
//!
//! Moved here from `tddy-tools`' `pty_relay.rs` by `#unbundle` node 5. This crate's own
//! description already named `tddy-tools` as a consumer and it already owned
//! [`crate::local_pty_relay`], which `run_local_pty` was already delegating into — the client half
//! belonged here from the start. **Node 6 serves `terminal_session.TerminalSessionService` from
//! this crate**, on top of the bridge it already owns.
//!
//! # Four dispatch modes, chosen by which fields of [`PtyRelayConfig`] are set
//!
//! | Mode | Selected by | Reaches the terminal via |
//! |---|---|---|
//! | Local PTY | none of the below, `cmd` non-empty | [`crate::local_pty_relay`] in this process |
//! | gRPC connect-only | `session_id` | `terminal_session.TerminalSessionService` on an existing session |
//! | gRPC start-and-connect | `project_id` | `StartSession`, then the same terminal RPCs |
//! | Room session | `server_identity` or `daemon_identity` | a room, joined through the [`RoomConnector`] the caller supplies |
//!
//! # What stayed in `tddy-tools`
//!
//! `PtyRelayArgs`, the clap `#[derive(Args)]` struct with its twenty `#[arg]` attributes and their
//! defaults. This crate has no `clap` and gains none: the shape of a CLI subcommand belongs to the
//! crate that parses arguments, which is the same rule that kept the MCP tool shapes in
//! `tddy-tools` at M3 and M4b. `tddy-tools` builds a [`PtyRelayConfig`] from its parsed args and
//! calls [`run_pty_relay`]; every default value is still declared exactly once, in the `#[arg]`
//! that documents it.

mod daemon_http;
mod room;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tddy_room_client::RoomConnector;

use crate::local_terminal::RawMode;
use crate::pty_relay::daemon_http::connectrpc_post;
use crate::pty_relay::daemon_http::exchange_stub_session_token;
use crate::pty_relay::room::run_livekit_session;

/// Everything a relay needs to attach, with the argument parsing already done.
///
/// The four dispatch modes are not an enum because they are not disjoint at the CLI: `--sandbox`,
/// `--model` and `--initial-prompt` describe the session both gRPC start-and-connect and the
/// LiveKit start path create, and `--daemon-url` is read by three of the four. An enum would have
/// to repeat those fields per variant, and [`run_pty_relay`] would still have to reject the
/// combinations the CLI can express — which it does here, in one place, with a message naming the
/// two flags that conflict.
#[derive(Debug, Clone)]
pub struct PtyRelayConfig {
    /// Working directory for the spawned command in local PTY mode.
    pub dir: PathBuf,
    /// LiveKit server URL. Its presence is what makes the LiveKit modes reachable.
    pub livekit_url: Option<String>,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    /// LiveKit room to join; the relay falls back to `tddy-lobby`.
    pub livekit_room: Option<String>,
    /// Local participant identity in the room.
    pub client_identity: String,
    /// Connect-only: the running session's LiveKit identity, `daemon-<instance>-<session>`.
    pub server_identity: Option<String>,
    /// Start-and-connect: the daemon's own LiveKit identity in the common room.
    pub daemon_identity: Option<String>,
    /// Daemon HTTP base URL, used for auth and for the connectrpc terminal calls.
    pub daemon_url: String,
    /// Session token for `StartSession`. When `None`, one is exchanged via stub auth.
    pub session_token: Option<String>,
    /// Project the new session is created in. Mutually exclusive with `session_id`.
    pub project_id: Option<String>,
    pub agent: Option<String>,
    pub model: String,
    pub session_type: String,
    pub initial_prompt: Option<String>,
    pub permission_mode: Option<String>,
    /// Start claude-cli inside darwin Seatbelt (macOS only).
    pub sandbox: bool,
    /// Attach to this already-running session. Mutually exclusive with `project_id`.
    pub session_id: Option<String>,
    /// Command and arguments to run in local PTY mode.
    pub cmd: Vec<String>,
}

/// The room a relay joins when its config names none.
const LOBBY_ROOM: &str = "tddy-lobby";

/// How long a relay's token stays valid.
const ROOM_TOKEN_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

/// How long to wait for the session's RPC-serving participant to appear. It is normally already
/// joined; this covers the window right after a daemon or session restart.
const SERVER_PARTICIPANT_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Attach a terminal as `config` says. `connector` joins the room in the room-session mode; the
/// other modes never use it.
pub async fn run_pty_relay(
    config: PtyRelayConfig,
    connector: Arc<dyn RoomConnector>,
) -> Result<()> {
    if config.session_id.is_some() && config.project_id.is_some() {
        anyhow::bail!("use either --session-id (connect) or --project-id (start), not both");
    }
    if config.session_id.is_some() {
        return run_grpc_connect_only(config).await;
    }
    if config.project_id.is_some() {
        return run_grpc_start_and_connect(config).await;
    }
    if config.daemon_identity.is_some() || config.server_identity.is_some() {
        return run_livekit_session(config, connector.as_ref()).await;
    }
    if config.cmd.is_empty() {
        anyhow::bail!(
            "no command provided. Modes:\n\
              local PTY:              pty-relay -- <cmd> [args...]\n\
              start sandbox + attach: pty-relay --daemon-url URL --project-id ID --sandbox\n\
              attach existing:        pty-relay --daemon-url URL --session-id ID [--session-token TOKEN]\n\
              LiveKit session:        pty-relay --daemon-identity ID --project-id ID [--livekit-url ws://...]"
        );
    }
    run_local_pty(config).await
}

// ---------------------------------------------------------------------------
// Local PTY mode  (delegates to tddy-terminal-rpc's shared PTY runtime)
// ---------------------------------------------------------------------------

async fn run_local_pty(config: PtyRelayConfig) -> Result<()> {
    let cwd = config.dir.canonicalize().unwrap_or(config.dir);
    crate::local_pty_relay::run(config.cmd, cwd, Vec::new()).await
}

// ---------------------------------------------------------------------------
// Daemon gRPC terminal (StartSession + attach, or connect-only)
// ---------------------------------------------------------------------------

fn build_start_session_request(
    config: &PtyRelayConfig,
    session_token: &str,
) -> tddy_service::proto::session::StartSessionRequest {
    use tddy_service::proto::session::StartSessionRequest;

    StartSessionRequest {
        session_token: session_token.to_string(),
        project_id: config.project_id.clone().unwrap_or_default(),
        agent: config.agent.clone().unwrap_or_default(),
        session_type: config.session_type.clone(),
        model: config.model.clone(),
        initial_prompt: config.initial_prompt.clone().unwrap_or_default(),
        permission_mode: config.permission_mode.clone().unwrap_or_default(),
        sandbox: config.sandbox,
        ..Default::default()
    }
}

async fn resolve_session_token(config: &PtyRelayConfig) -> Result<String> {
    match config.session_token.as_deref().filter(|s| !s.is_empty()) {
        Some(t) => Ok(t.to_string()),
        None => {
            log::info!(
                target: "tddy_terminal_rpc::pty_relay",
                "no --session-token; exchanging via {}",
                config.daemon_url
            );
            exchange_stub_session_token(&config.daemon_url)
                .await
                .map_err(|e| anyhow::anyhow!("auto-auth: {e}"))
        }
    }
}

async fn run_grpc_connect_only(config: PtyRelayConfig) -> Result<()> {
    let session_id = config
        .session_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("--session-id must be non-empty"))?;
    let session_token = resolve_session_token(&config).await?;
    log::info!(
        target: "tddy_terminal_rpc::pty_relay",
        "connecting via gRPC to session {session_id}"
    );
    run_grpc_terminal(&config.daemon_url, session_id, &session_token).await
}

async fn run_grpc_start_and_connect(config: PtyRelayConfig) -> Result<()> {
    use prost::Message as _;
    use tddy_service::proto::session::StartSessionResponse;

    let project_id = config
        .project_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("--project-id must be non-empty"))?;
    let session_token = resolve_session_token(&config).await?;
    let req = build_start_session_request(&config, &session_token);

    log::info!(
        target: "tddy_terminal_rpc::pty_relay",
        "calling StartSession via HTTP {} (project_id={project_id}, sandbox={})…",
        config.daemon_url,
        config.sandbox
    );
    let resp_bytes = connectrpc_post(
        &reqwest::Client::new(),
        &config.daemon_url,
        "session.SessionService",
        "StartSession",
        req.encode_to_vec(),
    )
    .await?;

    let resp = StartSessionResponse::decode(resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode StartSessionResponse: {e}"))?;

    log::info!(
        target: "tddy_terminal_rpc::pty_relay",
        "session started: id={} sandbox={}",
        resp.session_id,
        config.sandbox
    );
    // The one line this crate prints, and it is a contract rather than a log: an operator running
    // `pty-relay --project-id …` has no other way to learn the id of the session just created, and
    // docs/ft/coder/sandboxed-codebase-mode.md shows scripts reading `session_id=` off stderr. It
    // is safe here and only here — `run_pty_relay` owns the terminal for the rest of the process's
    // life, so there is no TUI for it to corrupt.
    eprintln!("session_id={}", resp.session_id);

    run_grpc_terminal(&config.daemon_url, &resp.session_id, &session_token).await
}

// ---------------------------------------------------------------------------
// Session mode: auth + StartSession, then route to LiveKit or gRPC terminal
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Stub auth: call daemon's auth.AuthService to exchange a session token
// ---------------------------------------------------------------------------

pub use crate::pty_relay::daemon_http::{join_terminal_room, open_session_room_over_http};

// ---------------------------------------------------------------------------
// gRPC terminal path for claude-cli sessions (no LiveKit)
// ---------------------------------------------------------------------------

/// Connect to a claude-cli session's terminal via the daemon's connectrpc HTTP endpoint.
/// Uses `StreamTerminalOutput` (server-streaming) for output and `SendTerminalInput`
/// (unary) for input — the same path the web UI's `GhosttyTerminalGrpc` uses.
async fn run_grpc_terminal(
    daemon_url: &str,
    session_id: &str,
    session_token: &str,
) -> anyhow::Result<()> {
    use crate::proto::terminal_session::{
        SessionTerminalInput, SessionTerminalOutput, StreamReplayMode, StreamTerminalOutputRequest,
    };
    use prost::Message as _;
    use std::sync::atomic::{AtomicBool, Ordering};

    let http_client = reqwest::Client::new();

    let (rows, cols) = crate::local_terminal::terminal_size();

    // Output: open the streaming request and parse connect-protocol envelope frames.
    let stream_req = StreamTerminalOutputRequest {
        session_token: session_token.to_string(),
        session_id: session_id.to_string(),
        terminal_id: String::new(),
        initial_cols: cols as u32,
        initial_rows: rows as u32,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
        control_token: String::new(),
    };
    let mut resp = connectrpc_post_streaming(
        &http_client,
        daemon_url,
        crate::TERMINAL_SESSION_SERVICE,
        "StreamTerminalOutput",
        stream_req.encode_to_vec(),
    )
    .await?;

    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let shutdown = Arc::new(AtomicBool::new(false));

    // Input: dedicated task sends keystrokes as unary SendTerminalInput calls.
    let input_client = http_client.clone();
    let input_daemon_url = daemon_url.to_string();
    let input_session_id = session_id.to_string();
    let input_session_token = session_token.to_string();
    let shutdown_input = Arc::clone(&shutdown);
    tokio::spawn(async move {
        if let Some(resize) = encode_resize_osc() {
            let req = SessionTerminalInput {
                session_token: input_session_token.clone(),
                session_id: input_session_id.clone(),
                data: resize,
                terminal_id: String::new(),
                control_token: String::new(),
                input_offset: 0,
                mode: StreamReplayMode::Tail as i32,
                from_offset: 0,
                initial_cols: 0,
                initial_rows: 0,
            };
            let _ = connectrpc_post(
                &input_client,
                &input_daemon_url,
                crate::TERMINAL_SESSION_SERVICE,
                "SendTerminalInput",
                req.encode_to_vec(),
            )
            .await;
        }
        while let Some(data) = key_rx.recv().await {
            if shutdown_input.load(Ordering::Relaxed) {
                break;
            }
            let req = SessionTerminalInput {
                session_token: input_session_token.clone(),
                session_id: input_session_id.clone(),
                data,
                terminal_id: String::new(),
                control_token: String::new(),
                input_offset: 0,
                mode: StreamReplayMode::Tail as i32,
                from_offset: 0,
                initial_cols: 0,
                initial_rows: 0,
            };
            let _ = connectrpc_post(
                &input_client,
                &input_daemon_url,
                crate::TERMINAL_SESSION_SERVICE,
                "SendTerminalInput",
                req.encode_to_vec(),
            )
            .await;
        }
    });

    let _raw = RawMode::enable();

    let shutdown_stdin = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        let mut stdin = std::io::stdin();
        loop {
            if shutdown_stdin.load(Ordering::Relaxed) {
                break;
            }
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => {
                    shutdown_stdin.store(true, Ordering::Relaxed);
                    break;
                }
                Ok(n) => {
                    let _ = key_tx.blocking_send(buf[..n].to_vec());
                }
            }
        }
    });

    // Parse and forward envelope-framed output to stdout.
    // Use resp.chunk() to read raw bytes without needing the reqwest `stream` feature.
    let mut stdout = std::io::stdout();
    let mut buf = Vec::<u8>::new();
    'outer: loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match tokio::time::timeout(std::time::Duration::from_millis(100), resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                buf.extend_from_slice(chunk.as_ref());
                // Parse all complete envelope frames from buf.
                loop {
                    if buf.len() < 5 {
                        break;
                    }
                    let flags = buf[0];
                    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                    if buf.len() < 5 + len {
                        break;
                    }
                    let payload = buf[5..5 + len].to_vec();
                    buf.drain(0..5 + len);
                    if flags & 0x02 != 0 {
                        // end-stream flag
                        shutdown.store(true, Ordering::Relaxed);
                        break 'outer;
                    }
                    if let Ok(out) = SessionTerminalOutput::decode(payload.as_slice()) {
                        if !out.data.is_empty() {
                            let _ = stdout.write_all(&out.data);
                            let _ = stdout.flush();
                        }
                    }
                }
            }
            Ok(Ok(None)) => break, // stream ended
            Ok(Err(e)) => {
                log::warn!(target: "tddy_terminal_rpc::pty_relay", "stream error: {}", e);
                break;
            }
            Err(_) => {} // timeout — check shutdown and loop
        }
    }

    Ok(())
}

/// POST to connectrpc streaming endpoint. Returns the raw reqwest Response for streaming reads.
async fn connectrpc_post_streaming(
    client: &reqwest::Client,
    base: &str,
    service: &str,
    method: &str,
    body: Vec<u8>,
) -> anyhow::Result<reqwest::Response> {
    // Wrap body in a connect envelope frame (flags=0x00, then 4-byte big-endian length).
    let mut framed = Vec::with_capacity(5 + body.len());
    framed.push(0x00u8);
    framed.extend_from_slice(&(body.len() as u32).to_be_bytes());
    framed.extend_from_slice(&body);

    let url = format!("{}/rpc/{}/{}", base.trim_end_matches('/'), service, method);
    let resp = client
        .post(&url)
        .header("content-type", "application/connect+proto")
        .body(framed)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("POST {}: {}", url, e))?;
    if !resp.status().is_success() {
        anyhow::bail!("POST {} → HTTP {}", url, resp.status());
    }
    Ok(resp)
}

// ---------------------------------------------------------------------------
// Terminal helpers
// ---------------------------------------------------------------------------

/// The LiveKit mode's name for the same OSC resize sequence every other mode sends.
///
/// The delegation is a compile-time identity, so it needs no test of its own — asserting
/// `encode_resize() == encode_resize_osc()` would stay green through any change to either. The
/// format itself is pinned by `encode_resize_osc_uses_format_expected_by_daemon`.
fn encode_resize() -> Option<Vec<u8>> {
    encode_resize_osc()
}

fn encode_resize_osc() -> Option<Vec<u8>> {
    let (rows, cols) = crate::local_terminal::terminal_size();
    Some(format!("\x1b]resize;{};{}\x07", cols, rows).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_start_session_request_sets_sandbox_flag() {
        // Given
        let config = PtyRelayConfig {
            dir: ".".into(),
            livekit_url: None,
            livekit_api_key: "devkey".into(),
            livekit_api_secret: "secret".into(),
            livekit_room: None,
            client_identity: "pty-relay-client".into(),
            server_identity: None,
            daemon_identity: None,
            daemon_url: "http://127.0.0.1:8899".into(),
            session_token: None,
            project_id: Some("proj-1".into()),
            agent: None,
            model: "claude-opus-4-8".into(),
            session_type: "claude-cli".into(),
            initial_prompt: None,
            permission_mode: None,
            sandbox: true,
            session_id: None,
            cmd: vec![],
        };

        // When
        let req = build_start_session_request(&config, "tok");

        // Then
        assert!(
            req.sandbox,
            "StartSession must set sandbox=true when --sandbox is passed"
        );
        assert_eq!(req.project_id, "proj-1");
        assert_eq!(req.session_type, "claude-cli");
    }

    #[test]
    fn encode_resize_osc_uses_format_expected_by_daemon() {
        // When
        let bytes = encode_resize_osc().expect("encode_resize_osc must return Some");

        // Then
        assert!(
            bytes.starts_with(b"\x1b]resize;"),
            "resize must use OSC format \\x1b]resize;… but got: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        assert!(
            bytes.ends_with(b"\x07"),
            "resize must terminate with BEL (\\x07) but got: {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }
}
