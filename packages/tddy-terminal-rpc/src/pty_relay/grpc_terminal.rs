use crate::local_terminal::RawMode;
use crate::pty_relay::daemon_http::connectrpc_post;
use crate::pty_relay::encode_resize_osc;
use std::io::{Read, Write};
use std::sync::Arc;

/// Connect to a claude-cli session's terminal via the daemon's connectrpc HTTP endpoint.
/// Uses `StreamTerminalOutput` (server-streaming) for output and `SendTerminalInput`
/// (unary) for input — the same path the web UI's `GhosttyTerminalGrpc` uses.
pub(super) async fn run_grpc_terminal(
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
                request_snapshot: false,
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
                request_snapshot: false,
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
