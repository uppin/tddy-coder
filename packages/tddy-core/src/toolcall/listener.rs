//! Unix domain socket listener for tddy-tools relay.

use super::build::BuildListQuery;
use super::{
    build_executor, store_submit_result, ApproveRequestWire, AskRequestWire, BuildListRequestWire,
    BuildOptions, BuildRequestWire, InvokeActionRequestWire, ListActionsRequestWire,
    SubmitRequestWire, ToolCallRequest, ToolCallResponse,
};
use crate::session_actions::{
    classify_session_actions_exit_code, derive_repo_key, invoke_action_core, list_action_summaries,
    repo_actions_root, DiscoveryQuery,
};
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

static TOOLCALL_LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

fn toolcall_log(msg: &str) {
    let path = match TOOLCALL_LOG_PATH.lock().ok().and_then(|g| g.clone()) {
        Some(p) => p,
        None => return,
    };
    let now = chrono::Local::now().format("%H:%M:%S%.3f");
    let line = format!("{} {}\n", now, msg);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Set the log file path for toolcall debug logging.
pub fn set_toolcall_log_dir(log_dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(log_dir);
    let path = log_dir.join("toolcall.log");
    if let Ok(mut guard) = TOOLCALL_LOG_PATH.lock() {
        *guard = Some(path);
    }
}

/// Start the tool call listener. Returns (socket_path, receiver).
/// Caller must pass socket_path via TDDY_SOCKET to the agent subprocess.
/// The listener task runs until the process exits.
///
/// `session_dir` and `repo_root` are used to handle `list-actions` and `invoke-action` requests
/// directly in the listener (without involving the presenter) so they work for any session,
/// including remote (`claude-cli`) sessions where the listener runs co-located with the worktree.
#[cfg(unix)]
pub fn start_toolcall_listener(
    session_dir: Option<PathBuf>,
    repo_root: Option<PathBuf>,
) -> Result<
    (
        std::path::PathBuf,
        std::sync::mpsc::Receiver<ToolCallRequest>,
    ),
    std::io::Error,
> {
    let dir = std::env::temp_dir();
    let socket_path = dir.join(format!("tddy-toolcall-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);

    let (tx, rx) = std::sync::mpsc::sync_channel(32);
    let (path_tx, path_rx) = std::sync::mpsc::sync_channel(1);
    let socket_path_cleanup = socket_path.clone();
    let session_dir = Arc::new(session_dir);
    let repo_root = Arc::new(repo_root);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            let listener = UnixListener::bind(&socket_path_cleanup).expect("bind socket");
            path_tx.send(socket_path_cleanup.clone()).ok();
            accept_loop(listener, tx, session_dir, repo_root).await;
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
    _session_dir: Option<PathBuf>,
    _repo_root: Option<PathBuf>,
) -> Result<
    (
        std::path::PathBuf,
        std::sync::mpsc::Receiver<ToolCallRequest>,
    ),
    std::io::Error,
> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Unix socket not available on this platform",
    ))
}

async fn accept_loop(
    listener: UnixListener,
    tx: std::sync::mpsc::SyncSender<ToolCallRequest>,
    session_dir: Arc<Option<PathBuf>>,
    repo_root: Arc<Option<PathBuf>>,
) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let tx = tx.clone();
        let sd = Arc::clone(&session_dir);
        let rr = Arc::clone(&repo_root);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, tx, sd, rr).await {
                toolcall_log(&format!("[error] connection error: {}", e));
                log::debug!("[toolcall] connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    tx: std::sync::mpsc::SyncSender<ToolCallRequest>,
    session_dir: Arc<Option<PathBuf>>,
    repo_root: Arc<Option<PathBuf>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let line = line.trim();

    toolcall_log(&format!("[recv] {}", line));

    let request: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("invalid JSON: {}", e))?;

    let req_type = request
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if req_type == "submit" {
        let wire: SubmitRequestWire = serde_json::from_value(request)
            .map_err(|e| format!("invalid submit request: {}", e))?;
        toolcall_log(&format!(
            "[submit] goal={} data_len={}",
            wire.goal,
            wire.data.to_string().len()
        ));
        let json_str = serde_json::to_string(&wire.data).map_err(|e| e.to_string())?;
        store_submit_result(&wire.goal, &json_str);
        let response = ToolCallResponse::SubmitOk {
            goal: wire.goal.clone(),
        };
        let response_line = response.to_json_line();
        toolcall_log(&format!("[send] {}", response_line));
        writer
            .write_all(format!("{}\n", response_line).as_bytes())
            .await?;
        writer.flush().await?;
        let tool_request = ToolCallRequest::SubmitActivity {
            goal: wire.goal,
            data: wire.data,
        };
        match tx.try_send(tool_request) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                toolcall_log(
                    "[warn] presenter queue full after submit; result stored, activity log may be delayed",
                );
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                toolcall_log(
                    "[warn] presenter channel closed after submit; result stored, activity log skipped",
                );
            }
        }
        return Ok(());
    }

    // list-actions: handled directly in the listener (self-contained FS op, no presenter needed).
    if req_type == "list-actions" {
        let wire: ListActionsRequestWire = serde_json::from_value(request)
            .map_err(|e| format!("invalid list-actions request: {}", e))?;
        toolcall_log(&format!(
            "[list-actions] path_prefix={:?} query={:?} limit={:?} offset={:?}",
            wire.path_prefix, wire.query, wire.limit, wire.offset
        ));

        let sd = (*session_dir).clone();
        let rr = (*repo_root).clone();

        let discovery_query = DiscoveryQuery {
            path_prefix: wire.path_prefix,
            query: wire.query,
            limit: wire.limit,
            offset: wire.offset.unwrap_or(0),
        };

        let result = tokio::task::spawn_blocking(move || {
            // Compute the per-repo store root (if we have a repo root).
            let store_root: Option<PathBuf> = rr.as_ref().and_then(|r| {
                let canon = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
                let key = derive_repo_key(&canon);
                crate::output::tddy_data_dir_path()
                    .ok()
                    .map(|d| repo_actions_root(&d, &key))
            });

            list_action_summaries(sd.as_deref(), rr.as_deref(), &discovery_query)
                .map(|result| (result, store_root))
        })
        .await;

        let response = match result {
            Ok(Ok((list_result, _store_root))) => {
                let actions_json = serde_json::to_value(&list_result.actions)
                    .unwrap_or(serde_json::Value::Array(vec![]));
                ToolCallResponse::ActionsList {
                    actions: actions_json,
                    total: list_result.total,
                }
            }
            Ok(Err(e)) => {
                toolcall_log(&format!("[list-actions] error: {}", e));
                ToolCallResponse::Error {
                    message: e.to_string(),
                }
            }
            Err(e) => {
                toolcall_log(&format!("[list-actions] task panic: {}", e));
                ToolCallResponse::Error {
                    message: format!("list-actions task failed: {}", e),
                }
            }
        };

        let response_line = response.to_json_line();
        toolcall_log(&format!("[send] {}", response_line));
        writer
            .write_all(format!("{}\n", response_line).as_bytes())
            .await?;
        writer.flush().await?;
        return Ok(());
    }

    // invoke-action: handled directly in the listener (subprocess op, no presenter needed).
    if req_type == "invoke-action" {
        let wire: InvokeActionRequestWire = serde_json::from_value(request)
            .map_err(|e| format!("invalid invoke-action request: {}", e))?;
        toolcall_log(&format!(
            "[invoke-action] action={} data_len={}",
            wire.action,
            wire.data.len()
        ));

        let sd = (*session_dir).clone();
        let rr = (*repo_root).clone();

        let result = tokio::task::spawn_blocking(move || {
            let store_root: Option<PathBuf> = rr.as_ref().and_then(|r| {
                let canon = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
                let key = derive_repo_key(&canon);
                crate::output::tddy_data_dir_path()
                    .ok()
                    .map(|d| repo_actions_root(&d, &key))
            });

            invoke_action_core(
                sd.as_deref(),
                store_root.as_deref(),
                rr.as_deref(),
                &wire.action,
                &wire.data,
            )
        })
        .await;

        let response = match result {
            Ok(Ok(record)) => ToolCallResponse::ActionInvokeOk { record },
            Ok(Err(e)) => {
                toolcall_log(&format!("[invoke-action] error: {}", e));
                ToolCallResponse::ActionInvokeError {
                    exit_code: classify_session_actions_exit_code(&e),
                    message: e.to_string(),
                }
            }
            Err(e) => {
                toolcall_log(&format!("[invoke-action] task panic: {}", e));
                ToolCallResponse::ActionInvokeError {
                    exit_code: 1,
                    message: format!("invoke-action task failed: {}", e),
                }
            }
        };

        let response_line = response.to_json_line();
        toolcall_log(&format!("[send] {}", response_line));
        writer
            .write_all(format!("{}\n", response_line).as_bytes())
            .await?;
        writer.flush().await?;
        return Ok(());
    }

    // build-list / build: served by the registered BuildExecutor (set by tddy-coder).
    // tddy-core has no tddy-build dependency — only this extension point.
    if req_type == "build-list" || req_type == "build" {
        let response = handle_build_request(&req_type, request).await;
        let response_line = response.to_json_line();
        toolcall_log(&format!("[send] {}", response_line));
        writer
            .write_all(format!("{}\n", response_line).as_bytes())
            .await?;
        writer.flush().await?;
        return Ok(());
    }

    let (tool_request, response_rx) = if req_type == "ask" {
        let wire: AskRequestWire =
            serde_json::from_value(request).map_err(|e| format!("invalid ask request: {}", e))?;
        toolcall_log(&format!(
            "[ask] {} question(s): {}",
            wire.questions.len(),
            wire.questions
                .iter()
                .map(|q| q.question.chars().take(80).collect::<String>())
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let tool_request = ToolCallRequest::Ask {
            questions: wire.questions,
            response_tx,
        };
        (tool_request, response_rx)
    } else if req_type == "approve" {
        let wire: ApproveRequestWire = serde_json::from_value(request)
            .map_err(|e| format!("invalid approve request: {}", e))?;
        toolcall_log(&format!(
            "[approve] tool={} input_len={}",
            wire.tool_name,
            wire.input.to_string().len()
        ));
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let tool_request = ToolCallRequest::Approve {
            tool_name: wire.tool_name,
            input: wire.input,
            response_tx,
        };
        (tool_request, response_rx)
    } else {
        toolcall_log(&format!("[error] unknown request type: {}", req_type));
        let response = ToolCallResponse::Error {
            message: format!("unknown request type: {}", req_type),
        };
        writer.write_all(response.to_json_line().as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        return Ok(());
    };

    tx.send(tool_request).map_err(|_| "channel closed")?;
    toolcall_log("[wait] waiting for presenter response...");

    let response = response_rx
        .await
        .unwrap_or_else(|_| ToolCallResponse::Error {
            message: "response channel dropped".to_string(),
        });

    let response_line = response.to_json_line();
    toolcall_log(&format!("[send] {}", response_line));

    writer
        .write_all(format!("{}\n", response_line).as_bytes())
        .await?;
    writer.flush().await?;

    Ok(())
}

/// Serve a `build-list` / `build` request via the registered [`BuildExecutor`].
/// Returns a descriptive error when no executor has been registered.
async fn handle_build_request(req_type: &str, request: serde_json::Value) -> ToolCallResponse {
    let Some(executor) = build_executor() else {
        return ToolCallResponse::Error {
            message: "build support not enabled".to_string(),
        };
    };

    let is_list = req_type == "build-list";
    let result = tokio::task::spawn_blocking(move || {
        if is_list {
            let wire: BuildListRequestWire = serde_json::from_value(request)
                .map_err(|e| format!("invalid build-list request: {}", e))?;
            executor.build_list(
                std::path::Path::new(&wire.repo_dir),
                &BuildListQuery {
                    query: wire.query,
                    limit: wire.limit,
                    offset: wire.offset.unwrap_or(0),
                },
            )
        } else {
            let wire: BuildRequestWire = serde_json::from_value(request)
                .map_err(|e| format!("invalid build request: {}", e))?;
            executor.build(
                std::path::Path::new(&wire.repo_dir),
                &wire.target,
                &BuildOptions {
                    no_cache: wire.no_cache,
                    dry_run: wire.dry_run,
                },
            )
        }
    })
    .await;

    match result {
        Ok(Ok(value)) => ToolCallResponse::BuildJson { value },
        Ok(Err(message)) => ToolCallResponse::Error { message },
        Err(e) => ToolCallResponse::Error {
            message: format!("build task failed: {}", e),
        },
    }
}
