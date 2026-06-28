//! Host-side `SessionChannel` driver with local terminal PTY proxy.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use tddy_daemon::sandbox_session::relay_egress_request;
use tddy_daemon::tool_engine;
use tddy_service::proto::connection::ExecuteToolResponse;
use tddy_service::tonic_sandbox::session_frame::Payload as SessionPayload;
use tddy_service::tonic_sandbox::{HostPoll, SandboxInput, SessionFrame, SubscribeTerminal};
use tddy_task::TaskRegistry;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Connect to the in-jail sandbox gRPC server and relay stdin/stdout until disconnect.
pub async fn run_terminal_bridge(
    ready_marker: &Path,
    session_id: &str,
    worktree_path: &Path,
    task_registry: TaskRegistry,
) -> Result<()> {
    eprintln!("connecting SessionChannel on loopback…");
    let mut client = tddy_sandbox_darwin::connect_sandbox_client(ready_marker)
        .await
        .context("connect sandbox gRPC")?;

    let (host_tx, host_rx) = mpsc::channel(64);
    let host_stream = ReceiverStream::new(host_rx);
    let mut session = client
        .session_channel(host_stream)
        .await
        .context("open SessionChannel")?
        .into_inner();

    let (rows, cols) = terminal_size_or_default();
    host_tx
        .send(SessionFrame {
            payload: Some(SessionPayload::SubscribeTerminal(SubscribeTerminal {
                session_id: session_id.to_string(),
                terminal_id: "main".to_string(),
                initial_cols: cols as u32,
                initial_rows: rows as u32,
            })),
        })
        .await
        .context("subscribe terminal")?;

    log::info!(
        target: "tddy_sandbox_app::bridge",
        "SessionChannel open session_id={session_id} terminal={cols}x{rows}"
    );
    eprintln!("terminal bridge active (Ctrl-C or Ctrl-D to disconnect)");

    let shutdown = Arc::new(AtomicBool::new(false));
    let worktree = worktree_path.to_path_buf();
    let session_id_out = session_id.to_string();
    let host_tx_reader = host_tx.clone();
    let shutdown_reader = Arc::clone(&shutdown);

    tokio::spawn(async move {
        while let Some(Ok(frame)) = session.next().await {
            if shutdown_reader.load(Ordering::Relaxed) {
                break;
            }
            match frame.payload {
                Some(SessionPayload::ToolRequest(req)) => {
                    log::debug!(
                        target: "tddy_sandbox_app::bridge",
                        "tool request session={session_id_out} tool={}",
                        req.tool_name
                    );
                    let outcome = tool_engine::execute_tool(
                        &worktree,
                        &req.tool_name,
                        &req.args_json,
                        &task_registry,
                        &session_id_out,
                    )
                    .await;
                    let resp = ExecuteToolResponse {
                        result_json: outcome.result_json,
                        is_error: outcome.is_error,
                        error_message: outcome.error_message,
                        job_id: outcome.job_id,
                        job_running: outcome.job_running,
                    };
                    let _ = host_tx_reader
                        .send(SessionFrame {
                            payload: Some(SessionPayload::ToolResponse(resp)),
                        })
                        .await;
                }
                Some(SessionPayload::EgressRequest(req)) => {
                    log::debug!(
                        target: "tddy_sandbox_app::bridge",
                        "egress request session={session_id_out} url={}",
                        req.url
                    );
                    let resp = relay_egress_request(req).await;
                    let _ = host_tx_reader
                        .send(SessionFrame {
                            payload: Some(SessionPayload::EgressResponse(resp)),
                        })
                        .await;
                }
                Some(SessionPayload::TerminalOutput(out)) => {
                    if !out.data.is_empty() {
                        let mut stdout = std::io::stdout();
                        let _ = stdout.write_all(&out.data);
                        let _ = stdout.flush();
                    }
                }
                _ => {}
            }
        }
        shutdown_reader.store(true, Ordering::Relaxed);
    });

    let _raw = RawMode::enable();
    let host_tx_input = host_tx.clone();
    let session_id_in = session_id.to_string();
    let shutdown_input = Arc::clone(&shutdown);

    let input_task = tokio::spawn(async move {
        let mut poll = tokio::time::interval(Duration::from_millis(25));
        loop {
            tokio::select! {
                _ = poll.tick() => {
                    if shutdown_input.load(Ordering::Relaxed) {
                        break;
                    }
                    if host_tx_input
                        .send(SessionFrame {
                            payload: Some(SessionPayload::HostPoll(HostPoll {})),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    let shutdown_stdin = Arc::clone(&shutdown);
    let host_tx_stdin = host_tx.clone();
    let session_id_stdin = session_id_in.clone();
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
                    if n == 1 && buf[0] == 0x03 {
                        log::info!(target: "tddy_sandbox_app::bridge", "Ctrl-C — disconnecting");
                        shutdown_stdin.store(true, Ordering::Relaxed);
                        break;
                    }
                    let data = buf[..n].to_vec();
                    let session_id = session_id_stdin.clone();
                    let tx = host_tx_stdin.clone();
                    if tx
                        .blocking_send(SessionFrame {
                            payload: Some(SessionPayload::TerminalInput(SandboxInput {
                                session_id,
                                terminal_id: "main".to_string(),
                                data,
                            })),
                        })
                        .is_err()
                    {
                        shutdown_stdin.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
    });

    // Block until stdin EOF, Ctrl-C, or the sandbox closes the channel.
    while !shutdown.load(Ordering::Relaxed) {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            res = tokio::signal::ctrl_c() => {
                match res {
                    Ok(()) => {
                        log::info!(target: "tddy_sandbox_app::bridge", "Ctrl-C — shutting down");
                        shutdown.store(true, Ordering::Relaxed);
                    }
                    Err(e) => {
                        log::warn!(target: "tddy_sandbox_app::bridge", "ctrl_c listener: {e}");
                    }
                }
                break;
            }
        }
    }
    shutdown.store(true, Ordering::Relaxed);
    input_task.abort();
    Ok(())
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
