//! `pty-relay` subcommand: spawn a command in a PTY and relay stdin/stdout, OR connect to an
//! existing daemon session via gRPC or LiveKit.
//!
//! Local PTY mode (default):
//!   tddy-tools pty-relay -- claude --model claude-opus-4-8
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

use anyhow::Result;
use clap::Args;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

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

    /// Model for claude-cli sessions (e.g. claude-opus-4-8).
    #[arg(long, default_value = "claude-opus-4-8")]
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

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run_pty_relay(args: PtyRelayArgs) -> Result<()> {
    if args.session_id.is_some() && args.project_id.is_some() {
        anyhow::bail!("use either --session-id (connect) or --project-id (start), not both");
    }
    if args.session_id.is_some() {
        return run_grpc_connect_only(args).await;
    }
    if args.project_id.is_some() {
        return run_grpc_start_and_connect(args).await;
    }
    if args.daemon_identity.is_some() || args.server_identity.is_some() {
        #[cfg(feature = "livekit")]
        return run_livekit_session(args).await;
        #[cfg(not(feature = "livekit"))]
        {
            let _ = args;
            anyhow::bail!(
                "LiveKit session mode requires the 'livekit' cargo feature.\n\
                 For gRPC terminal attach use --project-id or --session-id instead.\n\
                 Rebuild with: cargo build -p tddy-tools --features livekit"
            );
        }
    }
    if args.cmd.is_empty() {
        anyhow::bail!(
            "no command provided. Modes:\n\
              local PTY:              pty-relay -- <cmd> [args...]\n\
              start sandbox + attach: pty-relay --daemon-url URL --project-id ID --sandbox\n\
              attach existing:        pty-relay --daemon-url URL --session-id ID [--session-token TOKEN]\n\
              LiveKit session:        pty-relay --daemon-identity ID --project-id ID [--livekit-url ws://...]"
        );
    }
    tokio::task::spawn_blocking(move || run_local_pty(args)).await?
}

// ---------------------------------------------------------------------------
// Local PTY mode  (blocking — runs inside spawn_blocking)
// ---------------------------------------------------------------------------

fn run_local_pty(args: PtyRelayArgs) -> Result<()> {
    let cwd = args.dir.canonicalize().unwrap_or(args.dir);
    let (rows, cols) = terminal_size_or_default();

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| anyhow::anyhow!("openpty: {}", e))?;

    let mut cmd = CommandBuilder::new(&args.cmd[0]);
    for arg in &args.cmd[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| anyhow::anyhow!("spawn: {}", e))?;
    drop(pair.slave);

    let master = Arc::new(Mutex::new(pair.master));
    let _raw = RawMode::enable();

    let master_reader = Arc::clone(&master);
    let reader_thread = std::thread::spawn(move || {
        let reader = master_reader.lock().unwrap().try_clone_reader();
        match reader {
            Err(e) => log::warn!(target: "tddy_tools::pty_relay", "clone reader: {}", e),
            Ok(mut r) => {
                let mut buf = [0u8; 4096];
                let mut stdout = std::io::stdout();
                loop {
                    match r.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let _ = stdout.write_all(&buf[..n]);
                            let _ = stdout.flush();
                        }
                    }
                }
            }
        }
    });

    let master_writer = Arc::clone(&master);
    let _writer_thread = std::thread::spawn(move || {
        let writer = master_writer.lock().unwrap().take_writer();
        match writer {
            Err(e) => log::warn!(target: "tddy_tools::pty_relay", "take writer: {}", e),
            Ok(mut w) => {
                let mut buf = [0u8; 256];
                let mut stdin = std::io::stdin();
                loop {
                    match stdin.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if w.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    let _ = child.wait();
    let _ = reader_thread.join();
    Ok(())
}

// ---------------------------------------------------------------------------
// Daemon gRPC terminal (StartSession + attach, or connect-only)
// ---------------------------------------------------------------------------

fn build_start_session_request(args: &PtyRelayArgs, session_token: &str) -> tddy_service::proto::connection::StartSessionRequest {
    use tddy_service::proto::connection::StartSessionRequest;

    StartSessionRequest {
        session_token: session_token.to_string(),
        project_id: args.project_id.clone().unwrap_or_default(),
        agent: args.agent.clone().unwrap_or_default(),
        session_type: args.session_type.clone(),
        model: args.model.clone(),
        initial_prompt: args.initial_prompt.clone().unwrap_or_default(),
        permission_mode: args.permission_mode.clone().unwrap_or_default(),
        sandbox: args.sandbox,
        ..Default::default()
    }
}

async fn resolve_session_token(args: &PtyRelayArgs) -> Result<String> {
    match args.session_token.as_deref().filter(|s| !s.is_empty()) {
        Some(t) => Ok(t.to_string()),
        None => {
            log::info!(
                target: "tddy_tools::pty_relay",
                "no --session-token; exchanging via {}",
                args.daemon_url
            );
            exchange_stub_session_token(&args.daemon_url)
                .await
                .map_err(|e| anyhow::anyhow!("auto-auth: {e}"))
        }
    }
}

async fn run_grpc_connect_only(args: PtyRelayArgs) -> Result<()> {
    let session_id = args
        .session_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("--session-id must be non-empty"))?;
    let session_token = resolve_session_token(&args).await?;
    log::info!(
        target: "tddy_tools::pty_relay",
        "connecting via gRPC to session {session_id}"
    );
    run_grpc_terminal(&args.daemon_url, session_id, &session_token).await
}

async fn run_grpc_start_and_connect(args: PtyRelayArgs) -> Result<()> {
    use prost::Message as _;
    use tddy_service::proto::connection::StartSessionResponse;

    let project_id = args
        .project_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("--project-id must be non-empty"))?;
    let session_token = resolve_session_token(&args).await?;
    let req = build_start_session_request(&args, &session_token);

    log::info!(
        target: "tddy_tools::pty_relay",
        "calling StartSession via HTTP {} (project_id={project_id}, sandbox={})…",
        args.daemon_url,
        args.sandbox
    );
    let resp_bytes = connectrpc_post(
        &reqwest::Client::new(),
        &args.daemon_url,
        "connection.ConnectionService",
        "StartSession",
        req.encode_to_vec(),
    )
    .await?;

    let resp = StartSessionResponse::decode(resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode StartSessionResponse: {e}"))?;

    log::info!(
        target: "tddy_tools::pty_relay",
        "session started: id={} sandbox={}",
        resp.session_id,
        args.sandbox
    );
    eprintln!("session_id={}", resp.session_id);

    run_grpc_terminal(&args.daemon_url, &resp.session_id, &session_token).await
}

// ---------------------------------------------------------------------------
// Session mode: auth + StartSession, then route to LiveKit or gRPC terminal
// ---------------------------------------------------------------------------

/// LiveKit-backed start/connect (--daemon-identity / --server-identity).
#[cfg(feature = "livekit")]
async fn run_livekit_session(args: PtyRelayArgs) -> Result<()> {
    use prost::Message as _;
    use tddy_service::proto::connection::StartSessionResponse;

    // Connect-only path: no StartSession, just connect to the given LiveKit identity.
    if let Some(server_identity) = args.server_identity.clone() {
        let livekit_url = args
            .livekit_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--server-identity requires --livekit-url"))?;
        return run_livekit_terminal(&args, livekit_url.to_string(), server_identity, None).await;
    }

    // Start-and-connect path.
    let session_token = match args.session_token.as_deref().filter(|s| !s.is_empty()) {
        Some(t) => t.to_string(),
        None => {
            log::info!(target: "tddy_tools::pty_relay", "no --session-token; exchanging via {}", args.daemon_url);
            exchange_stub_session_token(&args.daemon_url)
                .await
                .map_err(|e| anyhow::anyhow!("auto-auth: {}", e))?
        }
    };

    let req = build_start_session_request(&args, &session_token);

    log::info!(target: "tddy_tools::pty_relay", "calling StartSession via HTTP {}…", args.daemon_url);
    let resp_bytes = connectrpc_post(
        &reqwest::Client::new(),
        &args.daemon_url,
        "connection.ConnectionService",
        "StartSession",
        req.encode_to_vec(),
    )
    .await?;

    let resp = StartSessionResponse::decode(resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode StartSessionResponse: {}", e))?;

    log::info!(
        target: "tddy_tools::pty_relay",
        "session started: id={} server_identity={}",
        resp.session_id, resp.livekit_server_identity
    );

    if !resp.livekit_server_identity.is_empty() {
        if let Some(livekit_url) = args.livekit_url.as_deref() {
            log::info!(target: "tddy_tools::pty_relay", "connecting via LiveKit");
            return run_livekit_terminal(
                &args,
                livekit_url.to_string(),
                resp.livekit_server_identity,
                Some(session_token),
            )
            .await;
        }
    }

    log::info!(target: "tddy_tools::pty_relay", "connecting via gRPC");
    run_grpc_terminal(&args.daemon_url, &resp.session_id, &session_token).await
}

/// Connect to a running session's terminal via LiveKit bidi stream.
#[cfg(feature = "livekit")]
async fn run_livekit_terminal(
    args: &PtyRelayArgs,
    livekit_url: String,
    server_identity: String,
    _session_token: Option<String>,
) -> Result<()> {
    use livekit::prelude::*;
    use prost::Message as _;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tddy_livekit::{RpcClient, TokenGenerator};
    use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};

    let room_name = args
        .livekit_room
        .as_deref()
        .unwrap_or("tddy-lobby")
        .to_string();
    let client_token = TokenGenerator::new(
        args.livekit_api_key.clone(),
        args.livekit_api_secret.clone(),
        room_name,
        args.client_identity.clone(),
        Duration::from_secs(3600),
    )
    .generate()
    .map_err(|e| anyhow::anyhow!("token: {}", e))?;

    let (raw_room, mut room_events) =
        Room::connect(&livekit_url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("room connect: {}", e))?;
    let room = Arc::new(raw_room);

    let target: ParticipantIdentity = server_identity.clone().into();
    log::info!(target: "tddy_tools::pty_relay", "waiting for session server participant \"{}\"…", server_identity);
    if !room.remote_participants().contains_key(&target) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(ev) = room_events.recv().await {
                if let RoomEvent::ParticipantConnected(p) = ev {
                    if p.identity().to_string() == server_identity {
                        return;
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for session server participant"))?;
    }

    log::info!(target: "tddy_tools::pty_relay", "server visible — starting terminal bidi stream");

    let rpc_events_term = room.subscribe();
    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_bidi = Arc::clone(&shutdown);

    tokio::spawn(async move {
        let client = RpcClient::new_shared(room, server_identity, rpc_events_term);
        let bidi = client.start_bidi_stream("terminal.TerminalService", "StreamTerminalIO");
        let (mut sender, mut rx) = match bidi {
            Ok(p) => p,
            Err(e) => {
                log::error!(target: "tddy_tools::pty_relay", "start_bidi_stream: {}", e);
                shutdown_bidi.store(true, Ordering::Relaxed);
                return;
            }
        };
        let _ = sender
            .send(TerminalInput { data: vec![] }.encode_to_vec(), false)
            .await;
        if let Some(resize) = encode_resize() {
            let _ = sender
                .send(TerminalInput { data: resize }.encode_to_vec(), false)
                .await;
        }
        loop {
            if shutdown_bidi.load(Ordering::Relaxed) {
                break;
            }
            tokio::select! {
                bytes = key_rx.recv() => {
                    match bytes {
                        Some(bytes) => {
                            if sender.send(TerminalInput { data: bytes }.encode_to_vec(), false).await.is_err() { break; }
                        }
                        None => break,
                    }
                }
                chunk = rx.recv() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            if let Ok(out) = TerminalOutput::decode(&bytes[..]) {
                                let _ = output_tx.send(out.data);
                            }
                        }
                        Some(Err(e)) => { log::warn!(target: "tddy_tools::pty_relay", "recv: {}", e); break; }
                        None => break, // bidi stream closed (server side)
                    }
                }
            }
        }
        shutdown_bidi.store(true, Ordering::Relaxed);
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

    let mut stdout = std::io::stdout();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(50), output_rx.recv()).await {
            Ok(Some(bytes)) => {
                let _ = stdout.write_all(&bytes);
                let _ = stdout.flush();
            }
            Ok(None) => break,
            Err(_timeout) => {}
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Stub auth: call daemon's auth.AuthService to exchange a session token
// ---------------------------------------------------------------------------

/// Calls GetAuthUrl (which for stub auth embeds `?code=<code>&state=<uuid>` in the URL),
/// then ExchangeCode to get a session token without any manual browser interaction.
async fn exchange_stub_session_token(daemon_url: &str) -> anyhow::Result<String> {
    use prost::Message as _;
    use tddy_service::proto::auth::{
        ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    };

    let client = reqwest::Client::new();

    let url_req_bytes = GetAuthUrlRequest {}.encode_to_vec();
    let url_resp_bytes = connectrpc_post(
        &client,
        daemon_url,
        "auth.AuthService",
        "GetAuthUrl",
        url_req_bytes,
    )
    .await?;
    let url_resp = GetAuthUrlResponse::decode(url_resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode GetAuthUrlResponse: {}", e))?;

    // Stub URL: http://<host>/auth/callback?code=test-code&state=<uuid>
    let query = url_resp
        .authorize_url
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("");
    let mut code = String::new();
    let mut state = url_resp.state.clone();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "code" => code = v.to_string(),
                "state" => state = v.to_string(),
                _ => {}
            }
        }
    }
    if code.is_empty() {
        anyhow::bail!(
            "stub authorize URL has no ?code= — is github.stub=true in daemon config? URL: {}",
            url_resp.authorize_url
        );
    }

    let exchange_req_bytes = ExchangeCodeRequest { code, state }.encode_to_vec();
    let exchange_resp_bytes = connectrpc_post(
        &client,
        daemon_url,
        "auth.AuthService",
        "ExchangeCode",
        exchange_req_bytes,
    )
    .await?;
    let exchange_resp = ExchangeCodeResponse::decode(exchange_resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode ExchangeCodeResponse: {}", e))?;

    log::info!(target: "tddy_tools::pty_relay", "authenticated as: {}", exchange_resp.user.map(|u| u.login).unwrap_or_default());
    Ok(exchange_resp.session_token)
}

async fn connectrpc_post(
    client: &reqwest::Client,
    base: &str,
    service: &str,
    method: &str,
    body: Vec<u8>,
) -> anyhow::Result<Vec<u8>> {
    let url = format!("{}/rpc/{}/{}", base.trim_end_matches('/'), service, method);
    let resp = client
        .post(&url)
        .header("content-type", "application/proto")
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("POST {}: {}", url, e))?;
    if !resp.status().is_success() {
        anyhow::bail!("POST {} → HTTP {}", url, resp.status());
    }
    Ok(resp
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("read response: {}", e))?
        .to_vec())
}

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
    use prost::Message as _;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tddy_service::proto::connection::{
        SessionTerminalInput, SessionTerminalOutput, StreamTerminalOutputRequest,
    };

    let http_client = reqwest::Client::new();

    let (rows, cols) = terminal_size_or_default();

    // Output: open the streaming request and parse connect-protocol envelope frames.
    let stream_req = StreamTerminalOutputRequest {
        session_token: session_token.to_string(),
        session_id: session_id.to_string(),
        terminal_id: String::new(),
        initial_cols: cols as u32,
        initial_rows: rows as u32,
    };
    let mut resp = connectrpc_post_streaming(
        &http_client,
        daemon_url,
        "connection.ConnectionService",
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
            };
            let _ = connectrpc_post(
                &input_client,
                &input_daemon_url,
                "connection.ConnectionService",
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
            };
            let _ = connectrpc_post(
                &input_client,
                &input_daemon_url,
                "connection.ConnectionService",
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
                log::warn!(target: "tddy_tools::pty_relay", "stream error: {}", e);
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

#[cfg(feature = "livekit")]
fn encode_resize() -> Option<Vec<u8>> {
    encode_resize_osc()
}

fn encode_resize_osc() -> Option<Vec<u8>> {
    let (rows, cols) = terminal_size_or_default();
    Some(format!("\x1b]resize;{};{}\x07", cols, rows).into_bytes())
}

fn terminal_size_or_default() -> (u16, u16) {
    #[cfg(unix)]
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0
            && ws.ws_row > 0
            && ws.ws_col > 0
        {
            return (ws.ws_row, ws.ws_col);
        }
    }
    (24, 220)
}

struct RawMode {
    #[cfg(unix)]
    saved: libc::termios,
}

impl RawMode {
    fn enable() -> Self {
        #[cfg(unix)]
        unsafe {
            let mut saved: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut saved) == 0 {
                let mut raw = saved;
                libc::cfmakeraw(&mut raw);
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
                return Self { saved };
            }
        }
        Self {
            #[cfg(unix)]
            saved: unsafe { std::mem::zeroed() },
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_start_session_request_sets_sandbox_flag() {
        // Given
        let args = PtyRelayArgs {
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
        let req = build_start_session_request(&args, "tok");

        // Then
        assert!(req.sandbox, "StartSession must set sandbox=true when --sandbox is passed");
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

#[cfg(all(test, feature = "livekit"))]
mod livekit_tests {
    use super::*;

    #[test]
    fn encode_resize_delegates_to_osc_format() {
        // When / Then
        assert_eq!(encode_resize(), encode_resize_osc());
    }
}
