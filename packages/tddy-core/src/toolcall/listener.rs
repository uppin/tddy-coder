//! Unix domain socket listener for tddy-tools relay, served over `tddy-rpc`/`tddy-stdio` framing.

use super::build::BuildListQuery;
use super::{
    build_executor, store_submit_result, transition_handler, ApproveRequestWire, AskRequestWire,
    BuildListRequestWire, BuildOptions, BuildRequestWire, InvokeActionRequestWire,
    ListActionsRequestWire, SpawnChildRequestWire, SubmitRequestWire, ToolCallRequest,
    ToolCallResponse, TransitionRelayOutcome, TransitionRequestWire,
};
use crate::session_actions::{
    classify_session_actions_exit_code, derive_repo_key, invoke_action_core, list_action_summaries,
    repo_actions_root, DiscoveryQuery,
};
use async_trait::async_trait;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tokio::net::UnixListener;

/// RPC service name this listener hosts over `tddy-stdio` — used only for logging/error context;
/// a single connection only ever hosts this one service.
const TOOLCALL_SERVICE_NAME: &str = "tddy.toolcall.ToolcallService";

/// Per-session capability to materialize a planned-PR node into a child coding session.
///
/// Bound per instance on the daemon's PR-stack orchestrator listener (mirroring
/// [`ToolcallRpcService::with_transition_handler`]). Because the socket is per-session, an
/// orchestrator can only spawn children for its own stack. `spawn_child` returns the new child
/// session id, or an error message surfaced verbatim to the agent.
#[async_trait]
pub trait ChildSpawnHandler: Send + Sync {
    async fn spawn_child(&self, node_id: &str) -> Result<String, String>;
}

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
    tddy_data_dir: PathBuf,
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
    let tddy_data_dir = Arc::new(tddy_data_dir);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            let listener = UnixListener::bind(&socket_path_cleanup).expect("bind socket");
            path_tx.send(socket_path_cleanup.clone()).ok();
            accept_loop(listener, tx, session_dir, repo_root, tddy_data_dir).await;
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
    _tddy_data_dir: PathBuf,
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
    tddy_data_dir: Arc<PathBuf>,
) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let service = ToolcallRpcService::new(
            tx.clone(),
            Arc::clone(&session_dir),
            Arc::clone(&repo_root),
            Arc::clone(&tddy_data_dir),
        );
        let (reader, writer) = stream.into_split();
        let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(reader, writer, service);
        tokio::spawn(endpoint.run());
    }
}

/// Serves the toolcall relay's request/response verbs (`Submit`/`Ask`/`Approve`/`ListActions`/
/// `InvokeAction`/`Build`/`BuildList`) over `tddy-rpc`, one instance per accepted connection. The
/// wire *payloads* are unchanged JSON (the same `*Wire` structs and [`ToolCallResponse`] shapes
/// the old newline-delimited protocol used) — only the framing/dispatch that carries them changed,
/// from a raw JSON line to a `tddy-rpc` unary call keyed by RPC method name.
pub struct ToolcallRpcService {
    tx: std::sync::mpsc::SyncSender<ToolCallRequest>,
    session_dir: Arc<Option<PathBuf>>,
    repo_root: Arc<Option<PathBuf>>,
    tddy_data_dir: Arc<PathBuf>,
    /// Per-instance `transition` handler. When set, it takes precedence over the process-global
    /// registry — this is how a daemon serving many concurrent sessions routes each session's
    /// `transition` to that session's own `WorkflowController` instead of a single shared handler.
    transition_handler: Option<Arc<dyn crate::toolcall::TransitionHandler>>,
    /// Per-instance `spawn-child` handler. Present only on a PR-stack orchestrator's listener.
    child_spawn_handler: Option<Arc<dyn ChildSpawnHandler>>,
}

impl ToolcallRpcService {
    pub fn new(
        tx: std::sync::mpsc::SyncSender<ToolCallRequest>,
        session_dir: Arc<Option<PathBuf>>,
        repo_root: Arc<Option<PathBuf>>,
        tddy_data_dir: Arc<PathBuf>,
    ) -> Self {
        Self::with_transition_handler(tx, session_dir, repo_root, tddy_data_dir, None)
    }

    /// Like [`Self::new`], but binds a per-instance `transition` handler that takes precedence over
    /// the process-global registry (see [`crate::toolcall::transition_handler`]).
    pub fn with_transition_handler(
        tx: std::sync::mpsc::SyncSender<ToolCallRequest>,
        session_dir: Arc<Option<PathBuf>>,
        repo_root: Arc<Option<PathBuf>>,
        tddy_data_dir: Arc<PathBuf>,
        transition_handler: Option<Arc<dyn crate::toolcall::TransitionHandler>>,
    ) -> Self {
        Self {
            tx,
            session_dir,
            repo_root,
            tddy_data_dir,
            transition_handler,
            child_spawn_handler: None,
        }
    }

    /// Bind a per-instance `spawn-child` handler (builder-style; keeps existing constructors intact).
    pub fn with_child_spawn_handler(
        mut self,
        child_spawn_handler: Option<Arc<dyn ChildSpawnHandler>>,
    ) -> Self {
        self.child_spawn_handler = child_spawn_handler;
        self
    }

    async fn dispatch(&self, method: &str, payload: &[u8]) -> Result<ToolCallResponse, Status> {
        if !matches!(
            method,
            "Submit"
                | "ListActions"
                | "InvokeAction"
                | "Build"
                | "BuildList"
                | "Ask"
                | "Approve"
                | "Transition"
                | "SpawnChild"
        ) {
            toolcall_log(&format!("[error] unknown method: {}", method));
            return Err(Status::not_found(format!(
                "unknown toolcall method: {TOOLCALL_SERVICE_NAME}/{method}"
            )));
        }

        let request: serde_json::Value = serde_json::from_slice(payload)
            .map_err(|e| Status::invalid_argument(format!("invalid JSON: {e}")))?;

        match method {
            "Submit" => self.handle_submit(request),
            "ListActions" => self.handle_list_actions(request).await,
            "InvokeAction" => self.handle_invoke_action(request).await,
            "Build" => Ok(handle_build_request("build", request).await),
            "BuildList" => Ok(handle_build_request("build-list", request).await),
            "Ask" => self.handle_ask(request).await,
            "Approve" => self.handle_approve(request).await,
            "Transition" => self.handle_transition(request),
            "SpawnChild" => self.handle_spawn_child(request).await,
            _ => unreachable!("checked above"),
        }
    }

    /// transition: handled directly in the listener (self-contained; no presenter round-trip
    /// needed). The per-instance [`Self::transition_handler`] takes precedence — for a daemon that
    /// serves many sessions, each session's listener carries its own `WorkflowController`. When no
    /// per-instance handler is bound, it falls back to the process-global
    /// [`crate::toolcall::transition_handler`] registered by the in-process agent-driven runner.
    /// Subagent calls carry `parent_tool_use_id`, making them provisional.
    fn handle_transition(&self, request: serde_json::Value) -> Result<ToolCallResponse, Status> {
        let wire: TransitionRequestWire = serde_json::from_value(request)
            .map_err(|e| Status::invalid_argument(format!("invalid transition request: {e}")))?;
        let provisional = wire.is_provisional();
        toolcall_log(&format!(
            "[transition] to={} provisional={}",
            wire.to, provisional
        ));
        let Some(handler) = self.transition_handler.clone().or_else(transition_handler) else {
            return Ok(ToolCallResponse::TransitionRejected {
                reason: "no active workflow controller; `transition` is only available in \
                         agent-driven sessions"
                    .to_string(),
            });
        };
        Ok(match handler.handle_transition(&wire.to, provisional) {
            TransitionRelayOutcome::Committed { instructions } => {
                ToolCallResponse::TransitionOk { instructions }
            }
            TransitionRelayOutcome::Provisional { to } => {
                ToolCallResponse::TransitionProvisional { to }
            }
            TransitionRelayOutcome::Rejected { reason } => {
                ToolCallResponse::TransitionRejected { reason }
            }
        })
    }

    /// spawn-child: materialize a planned-PR node into a child session via the per-instance
    /// [`ChildSpawnHandler`]. Only bound on a PR-stack orchestrator's listener; without a handler
    /// the verb is rejected rather than silently no-op'ing.
    async fn handle_spawn_child(
        &self,
        request: serde_json::Value,
    ) -> Result<ToolCallResponse, Status> {
        let wire: SpawnChildRequestWire = serde_json::from_value(request)
            .map_err(|e| Status::invalid_argument(format!("invalid spawn-child request: {e}")))?;
        toolcall_log(&format!("[spawn-child] node_id={}", wire.node_id));
        let Some(handler) = self.child_spawn_handler.clone() else {
            return Ok(ToolCallResponse::Error {
                message: "spawn-child is only available in a PR-stack orchestrator session"
                    .to_string(),
            });
        };
        Ok(match handler.spawn_child(&wire.node_id).await {
            Ok(session_id) => ToolCallResponse::SpawnChildOk { session_id },
            Err(message) => ToolCallResponse::Error { message },
        })
    }

    fn handle_submit(&self, request: serde_json::Value) -> Result<ToolCallResponse, Status> {
        let wire: SubmitRequestWire = serde_json::from_value(request)
            .map_err(|e| Status::invalid_argument(format!("invalid submit request: {}", e)))?;
        toolcall_log(&format!(
            "[submit] goal={} data_len={}",
            wire.goal,
            wire.data.to_string().len()
        ));
        let json_str =
            serde_json::to_string(&wire.data).map_err(|e| Status::internal(e.to_string()))?;
        store_submit_result(&wire.goal, &json_str);
        let response = ToolCallResponse::SubmitOk {
            goal: wire.goal.clone(),
        };
        let tool_request = ToolCallRequest::SubmitActivity {
            goal: wire.goal,
            data: wire.data,
        };
        match self.tx.try_send(tool_request) {
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
        Ok(response)
    }

    /// list-actions: handled directly in the listener (self-contained FS op, no presenter needed).
    async fn handle_list_actions(
        &self,
        request: serde_json::Value,
    ) -> Result<ToolCallResponse, Status> {
        let wire: ListActionsRequestWire = serde_json::from_value(request).map_err(|e| {
            Status::invalid_argument(format!("invalid list-actions request: {}", e))
        })?;
        toolcall_log(&format!(
            "[list-actions] path_prefix={:?} query={:?} limit={:?} offset={:?}",
            wire.path_prefix, wire.query, wire.limit, wire.offset
        ));

        let sd = (*self.session_dir).clone();
        let rr = (*self.repo_root).clone();
        let dd = Arc::clone(&self.tddy_data_dir);

        let discovery_query = DiscoveryQuery {
            path_prefix: wire.path_prefix,
            query: wire.query,
            limit: wire.limit,
            offset: wire.offset.unwrap_or(0),
        };

        let result = tokio::task::spawn_blocking(move || {
            // Compute the per-repo store root (if we have a repo root).
            let store_root: Option<PathBuf> = rr.as_ref().map(|r| {
                let canon = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
                let key = derive_repo_key(&canon);
                repo_actions_root(&dd, &key)
            });

            list_action_summaries(sd.as_deref(), rr.as_deref(), &dd, &discovery_query)
                .map(|result| (result, store_root))
        })
        .await;

        Ok(match result {
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
        })
    }

    /// invoke-action: handled directly in the listener (subprocess op, no presenter needed).
    async fn handle_invoke_action(
        &self,
        request: serde_json::Value,
    ) -> Result<ToolCallResponse, Status> {
        let wire: InvokeActionRequestWire = serde_json::from_value(request).map_err(|e| {
            Status::invalid_argument(format!("invalid invoke-action request: {}", e))
        })?;
        toolcall_log(&format!(
            "[invoke-action] action={} data_len={}",
            wire.action,
            wire.data.len()
        ));

        let sd = (*self.session_dir).clone();
        let rr = (*self.repo_root).clone();
        let dd = Arc::clone(&self.tddy_data_dir);

        let result = tokio::task::spawn_blocking(move || {
            let store_root: Option<PathBuf> = rr.as_ref().map(|r| {
                let canon = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
                let key = derive_repo_key(&canon);
                repo_actions_root(&dd, &key)
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

        Ok(match result {
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
        })
    }

    async fn handle_ask(&self, request: serde_json::Value) -> Result<ToolCallResponse, Status> {
        let wire: AskRequestWire = serde_json::from_value(request)
            .map_err(|e| Status::invalid_argument(format!("invalid ask request: {}", e)))?;
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
        self.await_presenter_response(tool_request, response_rx)
            .await
    }

    async fn handle_approve(&self, request: serde_json::Value) -> Result<ToolCallResponse, Status> {
        let wire: ApproveRequestWire = serde_json::from_value(request)
            .map_err(|e| Status::invalid_argument(format!("invalid approve request: {}", e)))?;
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
        self.await_presenter_response(tool_request, response_rx)
            .await
    }

    /// `ask`/`approve` block until the presenter responds via the oneshot channel carried on
    /// `tool_request` — see this module's doc comment.
    async fn await_presenter_response(
        &self,
        tool_request: ToolCallRequest,
        response_rx: tokio::sync::oneshot::Receiver<ToolCallResponse>,
    ) -> Result<ToolCallResponse, Status> {
        self.tx
            .send(tool_request)
            .map_err(|_| Status::internal("channel closed"))?;
        toolcall_log("[wait] waiting for presenter response...");

        Ok(response_rx
            .await
            .unwrap_or_else(|_| ToolCallResponse::Error {
                message: "response channel dropped".to_string(),
            }))
    }
}

#[async_trait]
impl RpcService for ToolcallRpcService {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        toolcall_log(&format!("[recv] {method}"));
        let result = match self.dispatch(method, &message.payload).await {
            Ok(response) => {
                let response_line = response.to_json_line();
                toolcall_log(&format!("[send] {}", response_line));
                Ok(response_line.into_bytes())
            }
            Err(status) => Err(status),
        };
        RpcResult::Unary(result)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tddy_rpc::{RpcMessage, RpcResult, RpcService};

    /// A toolcall RPC service with nobody draining its request channel — enough for `submit`
    /// (acknowledged on the wire immediately, per this module's own doc comment) and the
    /// listener-local verbs (`list-actions`), which never touch the channel at all. Scoped to a
    /// real (empty) repo root — `list_action_summaries` errors when given neither a session dir
    /// nor a repo root to look in at all.
    fn a_toolcall_service() -> (
        ToolcallRpcService,
        std::sync::mpsc::Receiver<ToolCallRequest>,
        tempfile::TempDir,
    ) {
        let (tx, rx) = std::sync::mpsc::sync_channel(32);
        let repo_root = tempfile::tempdir().unwrap();
        let tddy_data_dir = std::env::temp_dir().join(format!(
            "tddy-toolcall-rpc-service-test-{}",
            std::process::id()
        ));
        let service = ToolcallRpcService::new(
            tx,
            Arc::new(None),
            Arc::new(Some(repo_root.path().to_path_buf())),
            Arc::new(tddy_data_dir),
        );
        (service, rx, repo_root)
    }

    /// **toolcall_rpc_service_dispatches_submit_immediately_without_a_presenter**: a `Submit`
    /// call is acknowledged with the submitted goal even though nothing is draining the
    /// service's `ToolCallRequest` channel — matching the existing `submit` behavior
    /// (`submit_relay_no_poll.rs`), now over `tddy-rpc` framing instead of a raw JSON line.
    #[tokio::test]
    async fn toolcall_rpc_service_dispatches_submit_immediately_without_a_presenter() {
        // Given a toolcall RPC service with nobody draining its request channel
        let (service, _rx, _repo_root) = a_toolcall_service();
        let request = RpcMessage::new(
            serde_json::to_vec(&json!({
                "type": "submit",
                "goal": "plan",
                "data": {"prd": "# minimal"},
            }))
            .unwrap(),
            Default::default(),
        );

        // When dispatching a Submit call
        let result = service
            .handle_rpc("tddy.toolcall.ToolcallService", "Submit", &request)
            .await;

        // Then it is acknowledged immediately with the submitted goal
        let RpcResult::Unary(Ok(bytes)) = result else {
            panic!("expected a successful unary Submit response");
        };
        let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["status"], "ok");
        assert_eq!(response["goal"], "plan");
    }

    /// **toolcall_rpc_service_dispatches_list_actions_directly_without_a_presenter**: a
    /// `ListActions` call against an empty repo (no actions defined) returns zero actions — the
    /// listener-local verbs never touch the presenter/request channel.
    #[tokio::test]
    async fn toolcall_rpc_service_dispatches_list_actions_directly_without_a_presenter() {
        // Given a toolcall RPC service
        let (service, _rx, _repo_root) = a_toolcall_service();
        let request = RpcMessage::new(
            serde_json::to_vec(&json!({"type": "list-actions"})).unwrap(),
            Default::default(),
        );

        // When dispatching a ListActions call
        let result = service
            .handle_rpc("tddy.toolcall.ToolcallService", "ListActions", &request)
            .await;

        // Then the (nonexistent) repo yields zero actions
        let RpcResult::Unary(Ok(bytes)) = result else {
            panic!("expected a successful unary ListActions response");
        };
        let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["status"], "ok");
        assert_eq!(response["total"], 0);
    }

    /// A `transition` request with no registered handler is rejected (not a hard error) so the
    /// agent gets an actionable message; with a handler registered it routes through and derives
    /// `provisional` from `parent_tool_use_id`. One test to avoid racing the process-global
    /// registry across parallel tests.
    #[tokio::test]
    async fn toolcall_rpc_service_dispatches_transition_via_registered_handler() {
        use crate::toolcall::{
            clear_transition_handler, register_transition_handler, TransitionHandler,
            TransitionRelayOutcome,
        };

        struct FakeHandler;
        impl TransitionHandler for FakeHandler {
            fn handle_transition(&self, to: &str, provisional: bool) -> TransitionRelayOutcome {
                if provisional {
                    TransitionRelayOutcome::Provisional { to: to.to_string() }
                } else {
                    TransitionRelayOutcome::Committed {
                        instructions: format!("do {to}"),
                    }
                }
            }
        }

        let (service, _rx, _repo_root) = a_toolcall_service();

        // No handler registered yet → rejected with an actionable reason.
        clear_transition_handler();
        let req = RpcMessage::new(
            serde_json::to_vec(&json!({"type":"transition","to":"plan"})).unwrap(),
            Default::default(),
        );
        let RpcResult::Unary(Ok(bytes)) = service
            .handle_rpc("tddy.toolcall.ToolcallService", "Transition", &req)
            .await
        else {
            panic!("expected unary response");
        };
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["status"], "rejected");

        // Orchestrator transition (no parent_tool_use_id) → committed with instructions.
        register_transition_handler(Arc::new(FakeHandler));
        let req = RpcMessage::new(
            serde_json::to_vec(&json!({"type":"transition","to":"plan"})).unwrap(),
            Default::default(),
        );
        let RpcResult::Unary(Ok(bytes)) = service
            .handle_rpc("tddy.toolcall.ToolcallService", "Transition", &req)
            .await
        else {
            panic!("expected unary response");
        };
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["instructions"], "do plan");

        // Subagent transition (parent_tool_use_id present) → provisional.
        let req = RpcMessage::new(
            serde_json::to_vec(
                &json!({"type":"transition","to":"red","parent_tool_use_id":"toolu_123"}),
            )
            .unwrap(),
            Default::default(),
        );
        let RpcResult::Unary(Ok(bytes)) = service
            .handle_rpc("tddy.toolcall.ToolcallService", "Transition", &req)
            .await
        else {
            panic!("expected unary response");
        };
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["provisional"], true);
        assert_eq!(v["to"], "red");

        clear_transition_handler();
    }

    /// A per-instance transition handler on the service is used instead of the process-global
    /// registry — the seam that lets each concurrent daemon session route `transition` to its own
    /// `WorkflowController` rather than a single shared handler. This test deliberately never touches
    /// the process-global registry: `handle_transition` short-circuits on the per-instance handler
    /// (`self.transition_handler…or_else(global)`), so the global is never consulted here and there
    /// is no race with the sibling test that owns the global registry.
    #[tokio::test]
    async fn transition_routes_to_the_per_instance_handler() {
        use crate::toolcall::{TransitionHandler, TransitionRelayOutcome};

        /// Answers with its own label so the test can tell the per-instance handler was invoked.
        struct Labelled(&'static str);
        impl TransitionHandler for Labelled {
            fn handle_transition(&self, to: &str, _provisional: bool) -> TransitionRelayOutcome {
                TransitionRelayOutcome::Committed {
                    instructions: format!("{}:{to}", self.0),
                }
            }
        }

        // Given a service constructed with a per-instance handler via `with_transition_handler`.
        let (tx, _rx) = std::sync::mpsc::sync_channel(32);
        let repo_root = tempfile::tempdir().unwrap();
        let tddy_data_dir =
            std::env::temp_dir().join(format!("tddy-per-instance-handler-{}", std::process::id()));
        let service = ToolcallRpcService::with_transition_handler(
            tx,
            Arc::new(None),
            Arc::new(Some(repo_root.path().to_path_buf())),
            Arc::new(tddy_data_dir),
            Some(Arc::new(Labelled("per-instance")) as Arc<dyn TransitionHandler>),
        );

        // When dispatching a transition
        let req = RpcMessage::new(
            serde_json::to_vec(&json!({"type":"transition","to":"plan"})).unwrap(),
            Default::default(),
        );
        let RpcResult::Unary(Ok(bytes)) = service
            .handle_rpc("tddy.toolcall.ToolcallService", "Transition", &req)
            .await
        else {
            panic!("expected unary response");
        };

        // Then the per-instance handler answered.
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(
            v["instructions"], "per-instance:plan",
            "the per-instance handler must be used"
        );
    }

    /// **toolcall_rpc_service_returns_an_error_for_an_unknown_method**: an unrecognized method
    /// name reports an error rather than silently no-op'ing — mirroring the old protocol's
    /// `unknown request type` handling (see `handle_connection`'s final `else` branch above).
    #[tokio::test]
    async fn toolcall_rpc_service_returns_an_error_for_an_unknown_method() {
        // Given a toolcall RPC service
        let (service, _rx, _repo_root) = a_toolcall_service();
        let request = RpcMessage::new(Vec::new(), Default::default());

        // When dispatching an unrecognized method
        let result = service
            .handle_rpc("tddy.toolcall.ToolcallService", "DoesNotExist", &request)
            .await;

        // Then it reports an error naming the unknown method, rather than silently no-op'ing
        let RpcResult::Unary(Err(status)) = result else {
            panic!("expected a unary error for an unknown method");
        };
        assert!(
            status.message().contains("DoesNotExist"),
            "expected the error to name the unknown method, got: {}",
            status.message()
        );
    }
}
