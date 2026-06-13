//! `pty-relay` subcommand: spawn a command in a PTY and relay stdin/stdout, OR connect to an
//! existing daemon session via LiveKit, OR start a new session and connect — all via the same
//! Rust `RpcClient` path the web UI uses.
//!
//! Local PTY mode (default):
//!   tddy-tools pty-relay -- claude --model claude-opus-4-8
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
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
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

    // -- Local PTY mode -------------------------------------------------------

    /// Command and arguments to relay in local PTY mode (after `--`).
    #[arg(last = true)]
    pub cmd: Vec<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run_pty_relay(args: PtyRelayArgs) -> Result<()> {
    if args.daemon_identity.is_some() || args.server_identity.is_some() {
        #[cfg(feature = "livekit")]
        return run_session(args).await;
        #[cfg(not(feature = "livekit"))]
        {
            let _ = args;
            anyhow::bail!(
                "session mode requires the 'livekit' cargo feature.\nRebuild with: cargo build -p tddy-tools --features livekit"
            );
        }
    }
    if args.cmd.is_empty() {
        anyhow::bail!(
            "no command provided. Modes:\n  local PTY:     pty-relay -- <cmd> [args...]\n  daemon session: pty-relay --daemon-identity <id> --project-id <id> [--livekit-url ws://...]"
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
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| anyhow::anyhow!("openpty: {}", e))?;

    let mut cmd = CommandBuilder::new(&args.cmd[0]);
    for arg in &args.cmd[1..] { cmd.arg(arg); }
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let mut child = pair.slave.spawn_command(cmd)
        .map_err(|e| anyhow::anyhow!("spawn: {}", e))?;
    drop(pair.slave);

    let master = Arc::new(Mutex::new(pair.master));
    let _raw = RawMode::enable();

    let master_reader = Arc::clone(&master);
    let reader_thread = std::thread::spawn(move || {
        let reader = master_reader.lock().unwrap().try_clone_reader();
        match reader {
            Err(e) => eprintln!("pty-relay: clone reader: {}", e),
            Ok(mut r) => {
                let mut buf = [0u8; 4096];
                let mut stdout = std::io::stdout();
                loop {
                    match r.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => { let _ = stdout.write_all(&buf[..n]); let _ = stdout.flush(); }
                    }
                }
            }
        }
    });

    let master_writer = Arc::clone(&master);
    let _writer_thread = std::thread::spawn(move || {
        let writer = master_writer.lock().unwrap().take_writer();
        match writer {
            Err(e) => eprintln!("pty-relay: take writer: {}", e),
            Ok(mut w) => {
                let mut buf = [0u8; 256];
                let mut stdin = std::io::stdin();
                loop {
                    match stdin.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => { if w.write_all(&buf[..n]).is_err() { break; } }
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
// Session mode: auth + StartSession, then route to LiveKit or gRPC terminal
// ---------------------------------------------------------------------------

/// Single entry for daemon-backed sessions.
///
/// Terminal connectivity is chosen by what the session returns:
/// - `livekit_server_identity` non-empty + `--livekit-url` provided → LiveKit bidi stream
/// - otherwise → gRPC connectrpc stream
///
/// `--server-identity` skips StartSession and connects directly via LiveKit.
#[cfg(feature = "livekit")]
async fn run_session(args: PtyRelayArgs) -> Result<()> {
    use prost::Message as _;
    use tddy_service::proto::connection::{StartSessionRequest, StartSessionResponse};

    // Connect-only path: no StartSession, just connect to the given LiveKit identity.
    if let Some(server_identity) = args.server_identity.clone() {
        let livekit_url = args.livekit_url.as_deref()
            .ok_or_else(|| anyhow::anyhow!("--server-identity requires --livekit-url"))?;
        return run_livekit_terminal(&args, livekit_url.to_string(), server_identity, None).await;
    }

    // Start-and-connect path.
    let session_token = match args.session_token.as_deref().filter(|s| !s.is_empty()) {
        Some(t) => t.to_string(),
        None => {
            eprintln!("[pty-relay] no --session-token; exchanging via {}", args.daemon_url);
            exchange_stub_session_token(&args.daemon_url).await
                .map_err(|e| anyhow::anyhow!("auto-auth: {}", e))?
        }
    };

    let req = StartSessionRequest {
        session_token: session_token.clone(),
        project_id: args.project_id.clone().unwrap_or_default(),
        agent: args.agent.clone().unwrap_or_default(),
        session_type: args.session_type.clone(),
        model: args.model.clone(),
        ..Default::default()
    };

    eprintln!("[pty-relay] calling StartSession via HTTP {}…", args.daemon_url);
    let resp_bytes = connectrpc_post(
        &reqwest::Client::new(),
        &args.daemon_url,
        "connection.ConnectionService",
        "StartSession",
        req.encode_to_vec(),
    ).await?;

    let resp = StartSessionResponse::decode(resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode StartSessionResponse: {}", e))?;

    eprintln!(
        "[pty-relay] session started: id={} server_identity={}",
        resp.session_id, resp.livekit_server_identity
    );

    if !resp.livekit_server_identity.is_empty() {
        if let Some(livekit_url) = args.livekit_url.as_deref() {
            eprintln!("[pty-relay] connecting via LiveKit");
            return run_livekit_terminal(
                &args, livekit_url.to_string(), resp.livekit_server_identity, Some(session_token),
            ).await;
        }
    }

    eprintln!("[pty-relay] connecting via gRPC");
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

    let room_name = args.livekit_room.as_deref().unwrap_or("tddy-lobby").to_string();
    let client_token = TokenGenerator::new(
        args.livekit_api_key.clone(),
        args.livekit_api_secret.clone(),
        room_name,
        args.client_identity.clone(),
        Duration::from_secs(3600),
    )
    .generate()
    .map_err(|e| anyhow::anyhow!("token: {}", e))?;

    let (raw_room, mut room_events) = Room::connect(&livekit_url, &client_token, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("room connect: {}", e))?;
    let room = Arc::new(raw_room);

    let target: ParticipantIdentity = server_identity.clone().into();
    eprintln!("[pty-relay] waiting for session server participant \"{}\"…", server_identity);
    if !room.remote_participants().contains_key(&target) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(ev) = room_events.recv().await {
                if let RoomEvent::ParticipantConnected(p) = ev {
                    if p.identity().to_string() == server_identity { return; }
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for session server participant"))?;
    }

    eprintln!("[pty-relay] server visible — starting terminal bidi stream");

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
                eprintln!("[pty-relay] start_bidi_stream: {}", e);
                shutdown_bidi.store(true, Ordering::Relaxed);
                return;
            }
        };
        let _ = sender.send(TerminalInput { data: vec![] }.encode_to_vec(), false).await;
        if let Some(resize) = encode_resize() {
            let _ = sender.send(TerminalInput { data: resize }.encode_to_vec(), false).await;
        }
        loop {
            if shutdown_bidi.load(Ordering::Relaxed) { break; }
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
                        Some(Err(e)) => { eprintln!("[pty-relay] recv: {}", e); break; }
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
            if shutdown_stdin.load(Ordering::Relaxed) { break; }
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => { shutdown_stdin.store(true, Ordering::Relaxed); break; }
                Ok(n) => { let _ = key_tx.blocking_send(buf[..n].to_vec()); }
            }
        }
    });

    let mut stdout = std::io::stdout();
    loop {
        if shutdown.load(Ordering::Relaxed) { break; }
        match tokio::time::timeout(Duration::from_millis(50), output_rx.recv()).await {
            Ok(Some(bytes)) => { let _ = stdout.write_all(&bytes); let _ = stdout.flush(); }
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
#[cfg(feature = "livekit")]
async fn exchange_stub_session_token(daemon_url: &str) -> anyhow::Result<String> {
    use prost::Message as _;
    use tddy_service::proto::auth::{ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse};

    let client = reqwest::Client::new();

    let url_req_bytes = GetAuthUrlRequest {}.encode_to_vec();
    let url_resp_bytes = connectrpc_post(&client, daemon_url, "auth.AuthService", "GetAuthUrl", url_req_bytes).await?;
    let url_resp = GetAuthUrlResponse::decode(url_resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode GetAuthUrlResponse: {}", e))?;

    // Stub URL: http://<host>/auth/callback?code=test-code&state=<uuid>
    let query = url_resp.authorize_url.splitn(2, '?').nth(1).unwrap_or("");
    let mut code = String::new();
    let mut state = url_resp.state.clone();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k { "code" => code = v.to_string(), "state" => state = v.to_string(), _ => {} }
        }
    }
    if code.is_empty() {
        anyhow::bail!(
            "stub authorize URL has no ?code= — is github.stub=true in daemon config? URL: {}",
            url_resp.authorize_url
        );
    }

    let exchange_req_bytes = ExchangeCodeRequest { code, state }.encode_to_vec();
    let exchange_resp_bytes = connectrpc_post(&client, daemon_url, "auth.AuthService", "ExchangeCode", exchange_req_bytes).await?;
    let exchange_resp = ExchangeCodeResponse::decode(exchange_resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode ExchangeCodeResponse: {}", e))?;

    eprintln!("[pty-relay] authenticated as: {}", exchange_resp.user.map(|u| u.login).unwrap_or_default());
    Ok(exchange_resp.session_token)
}

#[cfg(feature = "livekit")]
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
    Ok(resp.bytes().await.map_err(|e| anyhow::anyhow!("read response: {}", e))?.to_vec())
}

// ---------------------------------------------------------------------------
// gRPC terminal path for claude-cli sessions (no LiveKit)
// ---------------------------------------------------------------------------

/// Connect to a claude-cli session's terminal via the daemon's connectrpc HTTP endpoint.
/// Uses `StreamTerminalOutput` (server-streaming) for output and `SendTerminalInput`
/// (unary) for input — the same path the web UI's `GhosttyTerminalGrpc` uses.
#[cfg(feature = "livekit")]
async fn run_grpc_terminal(daemon_url: &str, session_id: &str, session_token: &str) -> anyhow::Result<()> {
    use prost::Message as _;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tddy_service::proto::connection::{SessionTerminalInput, SessionTerminalOutput, StreamTerminalOutputRequest};

    let http_client = reqwest::Client::new();

    // Output: open the streaming request and parse connect-protocol envelope frames.
    let stream_req = StreamTerminalOutputRequest {
        session_token: session_token.to_string(),
        session_id: session_id.to_string(),
    };
    let mut resp = connectrpc_post_streaming(
        &http_client,
        daemon_url,
        "connection.ConnectionService",
        "StreamTerminalOutput",
        stream_req.encode_to_vec(),
    ).await?;

    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let shutdown = Arc::new(AtomicBool::new(false));

    // Input: dedicated task sends keystrokes as unary SendTerminalInput calls.
    let input_client = http_client.clone();
    let input_daemon_url = daemon_url.to_string();
    let input_session_id = session_id.to_string();
    let input_session_token = session_token.to_string();
    let shutdown_input = Arc::clone(&shutdown);
    tokio::spawn(async move {
        while let Some(data) = key_rx.recv().await {
            if shutdown_input.load(Ordering::Relaxed) { break; }
            let req = SessionTerminalInput {
                session_token: input_session_token.clone(),
                session_id: input_session_id.clone(),
                data,
            };
            let _ = connectrpc_post(
                &input_client,
                &input_daemon_url,
                "connection.ConnectionService",
                "SendTerminalInput",
                req.encode_to_vec(),
            ).await;
        }
    });

    let _raw = RawMode::enable();

    let shutdown_stdin = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        let mut stdin = std::io::stdin();
        loop {
            if shutdown_stdin.load(Ordering::Relaxed) { break; }
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => { shutdown_stdin.store(true, Ordering::Relaxed); break; }
                Ok(n) => { let _ = key_tx.blocking_send(buf[..n].to_vec()); }
            }
        }
    });

    // Parse and forward envelope-framed output to stdout.
    // Use resp.chunk() to read raw bytes without needing the reqwest `stream` feature.
    let mut stdout = std::io::stdout();
    let mut buf = Vec::<u8>::new();
    'outer: loop {
        if shutdown.load(Ordering::Relaxed) { break; }
        match tokio::time::timeout(std::time::Duration::from_millis(100), resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                buf.extend_from_slice(chunk.as_ref());
                // Parse all complete envelope frames from buf.
                loop {
                    if buf.len() < 5 { break; }
                    let flags = buf[0];
                    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                    if buf.len() < 5 + len { break; }
                    let payload = buf[5..5 + len].to_vec();
                    buf.drain(0..5 + len);
                    if flags & 0x02 != 0 { // end-stream flag
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
            Ok(Err(e)) => { eprintln!("[pty-relay] stream error: {}", e); break; }
            Err(_) => {} // timeout — check shutdown and loop
        }
    }

    Ok(())
}

/// POST to connectrpc streaming endpoint. Returns the raw reqwest Response for streaming reads.
#[cfg(feature = "livekit")]
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
    let (rows, cols) = terminal_size_or_default();
    Some(format!("\x1b]resize;{};{}\x07", cols, rows).into_bytes())
}

fn terminal_size_or_default() -> (u16, u16) {
    #[cfg(unix)]
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0
            && ws.ws_row > 0 && ws.ws_col > 0
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
        Self { #[cfg(unix)] saved: unsafe { std::mem::zeroed() } }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reproduces: pty-relay encodes resize as DECSLPP xterm format `\x1b[8;{rows};{cols}t`
    /// but VirtualTUI's `parse_resize_from_buf` expects OSC format `\x1b]resize;{cols};{rows}\x07`.
    /// With the wrong format the TUI never receives valid dimensions, stays at the 80x24 default,
    /// and renders separator lines and the status bar at the wrong width — producing doubled
    /// separators and a split `● high · /effort` line when viewed in a wider relay terminal.
    #[test]
    fn test_encode_resize_uses_osc_format_matching_virtual_tui() {
        let bytes = encode_resize().expect("encode_resize must return Some");
        // VirtualTUI parse_resize_from_buf expects: \x1b]resize;{cols};{rows}\x07
        assert!(
            bytes.starts_with(b"\x1b]resize;"),
            "resize must use OSC format \\x1b]resize;… (cols;rows\\x07) \
             but pty_relay produced: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        assert!(
            bytes.ends_with(b"\x07"),
            "resize must terminate with BEL (\\x07) but got: {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }
}
