//! Unix domain socket listener for tddy-tools relay.

use super::{AskRequestWire, SubmitRequestWire, ToolCallRequest, ToolCallResponse};
use std::sync::mpsc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

/// Start the tool call listener. Returns (socket_path, receiver).
/// Caller must pass socket_path via TDDY_SOCKET to the agent subprocess.
/// The listener task runs until the process exits.
#[cfg(unix)]
pub fn start_toolcall_listener(
) -> Result<(std::path::PathBuf, mpsc::Receiver<ToolCallRequest>), std::io::Error> {
    let dir = std::env::temp_dir();
    let socket_path = dir.join(format!("tddy-toolcall-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);

    let (tx, rx) = mpsc::sync_channel(32);
    let (path_tx, path_rx) = mpsc::sync_channel(1);
    let socket_path_cleanup = socket_path.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            let listener = UnixListener::bind(&socket_path_cleanup).expect("bind socket");
            path_tx.send(socket_path_cleanup.clone()).ok();
            accept_loop(listener, tx).await;
        });
        let _ = std::fs::remove_file(&socket_path_cleanup);
    });

    let socket_path = path_rx
        .recv()
        .map_err(|_| std::io::Error::other("listener thread exited before bind"))?;

    Ok((socket_path, rx))
}

#[cfg(not(unix))]
pub fn start_toolcall_listener(
) -> Result<(std::path::PathBuf, mpsc::Receiver<ToolCallRequest>), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Unix socket not available on this platform",
    ))
}

async fn accept_loop(listener: UnixListener, tx: mpsc::SyncSender<ToolCallRequest>) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, tx).await {
                log::debug!("[toolcall] connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    tx: mpsc::SyncSender<ToolCallRequest>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let line = line.trim();

    let request: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("invalid JSON: {}", e))?;

    let req_type = request.get("type").and_then(|v| v.as_str()).unwrap_or("");

    let (tool_request, response_rx) = if req_type == "submit" {
        let wire: SubmitRequestWire = serde_json::from_value(request)
            .map_err(|e| format!("invalid submit request: {}", e))?;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let tool_request = ToolCallRequest::Submit {
            goal: wire.goal,
            data: wire.data,
            response_tx,
        };
        (tool_request, response_rx)
    } else if req_type == "ask" {
        let wire: AskRequestWire =
            serde_json::from_value(request).map_err(|e| format!("invalid ask request: {}", e))?;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let tool_request = ToolCallRequest::Ask {
            questions: wire.questions,
            response_tx,
        };
        (tool_request, response_rx)
    } else {
        let response = ToolCallResponse::Error {
            message: format!("unknown request type: {}", req_type),
        };
        writer.write_all(response.to_json_line().as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        return Ok(());
    };

    tx.send(tool_request).map_err(|_| "channel closed")?;

    let response = response_rx
        .await
        .unwrap_or_else(|_| ToolCallResponse::Error {
            message: "response channel dropped".to_string(),
        });

    let response_line = response.to_json_line() + "\n";
    writer.write_all(response_line.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}
