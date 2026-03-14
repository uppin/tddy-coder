//! DaemonService — TddyRemote implementation for headless daemon mode.
//!
//! Implements GetSession, ListSessions, and Stream (StartSession flow).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use tddy_core::ViewConnection;
use tddy_tui::run_virtual_tui;

use tokio::sync::mpsc as tokio_mpsc;
use tonic::{Request, Response, Status};

use tddy_core::output::{create_session_dir_under, parse_planning_response, PlanningOutput};
use tddy_core::read_changeset;
use tddy_core::workflow::graph::{ElicitationEvent, ExecutionStatus};
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{setup_worktree_for_session, SharedBackend, WorkflowEngine};

use crate::convert::{plan_approval_to_server_message, workflow_event_to_server_message};
use crate::gen::{
    client_message, server_message, tddy_remote_server::TddyRemote, GetSessionRequest,
    GetSessionResponse, ListSessionsRequest, ListSessionsResponse, ServerMessage, SessionCreated,
    SessionInfo, WorkflowComplete,
};

// --- Constants ---

const STREAM_CHANNEL_CAPACITY: usize = 64;
const DAEMON_SESSION_TEMP_DIR: &str = "tddy-daemon-session";
const DEFAULT_BRANCH_SUGGESTION: &str = "feature/impl";
const DEFAULT_WORKTREE_SUGGESTION: &str = "feature-impl";
const CTX_FEATURE_INPUT: &str = "feature_input";
const CTX_OUTPUT_DIR: &str = "output_dir";
const CTX_SESSION_BASE: &str = "session_base";
const CTX_SESSION_ID: &str = "session_id";
const CTX_PLAN_DIR: &str = "plan_dir";
const CTX_WORKTREE_DIR: &str = "worktree_dir";
const PRD_FILENAME: &str = "PRD.md";
const CHANGESET_FILENAME: &str = "changeset.yaml";

// --- Helpers ---

/// Receive next client message. On Ok(None) or Err, returns None and may send error to tx.
async fn recv_client_message(
    client_stream: &mut tonic::codec::Streaming<crate::gen::ClientMessage>,
    tx: &tokio_mpsc::Sender<Result<ServerMessage, Status>>,
) -> Option<crate::gen::ClientMessage> {
    match client_stream.message().await {
        Ok(Some(m)) => Some(m),
        Ok(None) => None,
        Err(e) => {
            let _ = tx
                .send(Err(Status::internal(format!("stream error: {}", e))))
                .await;
            None
        }
    }
}

/// Construct and send WorkflowComplete ServerMessage.
async fn send_workflow_complete(
    tx: &tokio_mpsc::Sender<Result<ServerMessage, Status>>,
    ok: bool,
    message: String,
) {
    let _ = tx
        .send(Ok(ServerMessage {
            event: Some(server_message::Event::WorkflowComplete(WorkflowComplete {
                ok,
                message,
            })),
        }))
        .await;
}

/// Spawn a thread that forwards workflow events from rx to tx.
/// This avoids holding the non-Send mpsc::Receiver in async code.
fn spawn_event_forwarder(
    rx: mpsc::Receiver<tddy_core::WorkflowEvent>,
    tx: tokio_mpsc::Sender<Result<ServerMessage, Status>>,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Handle::current();
        while let Ok(ev) = rx.recv() {
            if let Some(msg) = workflow_event_to_server_message(ev) {
                let _ = rt.block_on(tx.send(Ok(msg)));
            }
        }
    });
}

/// Daemon gRPC service. Reads session state from disk; runs workflow on Stream.
pub struct DaemonService {
    sessions_base: PathBuf,
    backend: SharedBackend,
    view_connection_factory: Option<Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>>,
}

impl DaemonService {
    pub fn new(sessions_base: PathBuf, backend: SharedBackend) -> Self {
        Self {
            sessions_base,
            backend,
            view_connection_factory: None,
        }
    }

    /// Use per-connection VirtualTui for StreamTerminalIO (when LiveKit/presenter enabled).
    pub fn with_view_connection_factory(
        mut self,
        factory: Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>,
    ) -> Self {
        self.view_connection_factory = Some(factory);
        self
    }

    /// Derive session status from changeset state.
    fn status_from_state(state: &str) -> &'static str {
        match state {
            "Init" | "Planned" | "AcceptanceTestsReady" | "RedTestsReady" => "Active",
            "GreenComplete"
            | "DemoComplete"
            | "Evaluated"
            | "ValidateComplete"
            | "ValidateRefactorComplete"
            | "RefactorComplete"
            | "DocsUpdated" => "Completed",
            "Failed" => "Failed",
            _ => "Active",
        }
    }
}

#[tonic::async_trait]
impl TddyRemote for DaemonService {
    type StreamStream = tokio_stream::wrappers::ReceiverStream<Result<ServerMessage, Status>>;
    type StreamTerminalStream =
        tokio_stream::wrappers::ReceiverStream<Result<crate::gen::TerminalOutput, Status>>;
    type StreamTerminalIOStream =
        tokio_stream::wrappers::ReceiverStream<Result<crate::gen::TerminalOutput, Status>>;

    async fn stream(
        &self,
        request: Request<tonic::codec::Streaming<crate::gen::ClientMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let (tx, rx) = tokio_mpsc::channel(STREAM_CHANNEL_CAPACITY);
        let handler = DaemonStreamHandler::new(
            self.sessions_base.clone(),
            self.backend.clone(),
            request.into_inner(),
            tx,
        );
        tokio::spawn(handler.run());
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn stream_terminal(
        &self,
        _request: Request<crate::gen::StreamTerminalRequest>,
    ) -> Result<Response<Self::StreamTerminalStream>, Status> {
        let (tx, rx) = tokio_mpsc::channel(64);
        drop(tx);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn stream_terminal_io(
        &self,
        request: Request<tonic::codec::Streaming<crate::gen::TerminalInput>>,
    ) -> Result<Response<Self::StreamTerminalIOStream>, Status> {
        let (tx, rx) = tokio_mpsc::channel(64);
        let mut client_stream = request.into_inner();

        if let Some(ref factory) = self.view_connection_factory {
            if let Some(conn) = factory() {
                let (output_tx, mut output_rx) = tokio_mpsc::channel(64);
                let (input_tx, input_rx) = tokio_mpsc::channel(64);
                let shutdown = Arc::new(AtomicBool::new(false));
                let shutdown_clone = shutdown.clone();

                run_virtual_tui(conn, output_tx, input_rx, shutdown_clone);

                tokio::spawn(async move {
                    while let Ok(Some(msg)) = client_stream.message().await {
                        if !msg.data.is_empty() {
                            let _ = input_tx.send(msg.data).await;
                        }
                    }
                    shutdown.store(true, Ordering::Relaxed);
                });

                tokio::spawn(async move {
                    while let Some(bytes) = output_rx.recv().await {
                        if tx
                            .send(Ok(crate::gen::TerminalOutput { data: bytes }))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            } else {
                drop(tx);
            }
        } else {
            drop(tx);
        }

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let session_id = request.into_inner().session_id;
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }

        let plan_dir = self.sessions_base.join(&session_id);
        let changeset = read_changeset(&plan_dir)
            .map_err(|e| Status::not_found(format!("session not found: {} — {}", session_id, e)))?;

        let status = Self::status_from_state(&changeset.state.current);
        let plan_dir_str = plan_dir.to_string_lossy().to_string();
        let worktree = changeset.worktree.clone().unwrap_or_default();
        let branch = changeset.branch.clone().unwrap_or_default();

        Ok(Response::new(GetSessionResponse {
            session: Some(SessionInfo {
                session_id: session_id.clone(),
                status: status.to_string(),
                plan_dir: plan_dir_str,
                worktree,
                branch,
            }),
        }))
    }

    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let mut sessions = Vec::new();

        if !self.sessions_base.exists() {
            return Ok(Response::new(ListSessionsResponse { sessions }));
        }

        let entries = std::fs::read_dir(&self.sessions_base)
            .map_err(|e| Status::internal(format!("read sessions dir: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| Status::internal(format!("read dir entry: {}", e)))?;
            let path = entry.path();
            if path.is_dir() {
                let changeset_path = path.join(CHANGESET_FILENAME);
                if changeset_path.exists() {
                    if let Ok(changeset) = read_changeset(&path) {
                        let session_id = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        let status = Self::status_from_state(&changeset.state.current);
                        sessions.push(SessionInfo {
                            session_id,
                            status: status.to_string(),
                            plan_dir: path.to_string_lossy().to_string(),
                            worktree: changeset.worktree.clone().unwrap_or_default(),
                            branch: changeset.branch.clone().unwrap_or_default(),
                        });
                    }
                }
            }
        }

        Ok(Response::new(ListSessionsResponse { sessions }))
    }
}

#[derive(Clone)]
enum DaemonStreamState {
    Idle,
    WaitingApprovePlan,
    PlanComplete,
}

/// Handles the bidirectional stream: receives ClientMessage, runs workflow, sends ServerMessage.
struct DaemonStreamHandler {
    sessions_base: PathBuf,
    backend: SharedBackend,
    client_stream: tonic::codec::Streaming<crate::gen::ClientMessage>,
    tx: tokio_mpsc::Sender<Result<ServerMessage, Status>>,
    state: DaemonStreamState,
    session_id: Option<String>,
    plan_dir: Option<PathBuf>,
    repo_root: Option<PathBuf>,
    engine: Option<WorkflowEngine>,
}

impl DaemonStreamHandler {
    fn new(
        sessions_base: PathBuf,
        backend: SharedBackend,
        client_stream: tonic::codec::Streaming<crate::gen::ClientMessage>,
        tx: tokio_mpsc::Sender<Result<ServerMessage, Status>>,
    ) -> Self {
        Self {
            sessions_base,
            backend,
            client_stream,
            tx,
            state: DaemonStreamState::Idle,
            session_id: None,
            plan_dir: None,
            repo_root: None,
            engine: None,
        }
    }

    async fn run(mut self) {
        loop {
            if matches!(&self.state, DaemonStreamState::PlanComplete) {
                break;
            }

            let msg = match recv_client_message(&mut self.client_stream, &self.tx).await {
                Some(m) => m,
                None => break,
            };

            let should_break = match &self.state {
                DaemonStreamState::Idle => {
                    if let Some(client_message::Intent::StartSession(start)) = msg.intent {
                        self.handle_start_session(&start).await
                    } else {
                        false
                    }
                }
                DaemonStreamState::WaitingApprovePlan => {
                    if let Some(client_message::Intent::ApprovePlan(_)) = msg.intent {
                        if self.handle_approve_plan().await {
                            break;
                        }
                    }
                    false
                }
                DaemonStreamState::PlanComplete => unreachable!(),
            };

            if should_break {
                break;
            }
        }
    }

    /// Returns true if the run loop should break (e.g. on Error).
    async fn handle_start_session(&mut self, start: &crate::gen::StartSession) -> bool {
        let prompt = start.prompt.clone();
        let repo = if start.repo_root.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            PathBuf::from(&start.repo_root)
        };

        std::fs::create_dir_all(&self.sessions_base)
            .map_err(|e| Status::internal(format!("create sessions base: {}", e)))
            .ok();

        let sid = uuid::Uuid::new_v4().to_string();
        let plan = create_session_dir_under(&self.sessions_base, &sid)
            .map_err(|e| Status::internal(format!("create session dir: {}", e)))
            .unwrap();

        let init_cs = tddy_core::changeset::Changeset {
            initial_prompt: Some(prompt.clone()),
            ..tddy_core::changeset::Changeset::default()
        };
        let _ = tddy_core::changeset::write_changeset(&plan, &init_cs);

        self.session_id = Some(sid.clone());
        self.plan_dir = Some(plan.clone());
        self.repo_root = Some(repo.clone());

        let storage_dir = std::env::temp_dir().join(DAEMON_SESSION_TEMP_DIR);
        std::fs::create_dir_all(&storage_dir).ok();

        let (event_tx, rx) = mpsc::channel();
        spawn_event_forwarder(rx, self.tx.clone());
        let hooks = Arc::new(TddWorkflowHooks::with_event_tx(event_tx));
        let eng = WorkflowEngine::new(self.backend.clone(), storage_dir, Some(hooks));
        self.engine = Some(eng);

        let mut ctx = std::collections::HashMap::new();
        ctx.insert(CTX_FEATURE_INPUT.to_string(), serde_json::json!(prompt));
        ctx.insert(
            CTX_OUTPUT_DIR.to_string(),
            serde_json::to_value(repo.clone()).unwrap(),
        );
        ctx.insert(
            CTX_SESSION_BASE.to_string(),
            serde_json::to_value(self.sessions_base.clone()).unwrap(),
        );
        ctx.insert(CTX_SESSION_ID.to_string(), serde_json::json!(sid.clone()));
        ctx.insert(
            CTX_PLAN_DIR.to_string(),
            serde_json::to_value(plan.clone()).unwrap(),
        );

        let _ = self
            .tx
            .send(Ok(ServerMessage {
                event: Some(server_message::Event::SessionCreated(SessionCreated {
                    session_id: sid.clone(),
                })),
            }))
            .await;

        let rt = tokio::runtime::Handle::current();
        let result = rt
            .block_on(self.engine.as_ref().unwrap().run_goal("plan", ctx))
            .map_err(|e| Status::internal(format!("run_goal: {}", e)))
            .unwrap();

        match result.status {
            ExecutionStatus::ElicitationNeeded {
                event: ElicitationEvent::PlanApproval { prd_content },
            } => {
                let _ = self
                    .tx
                    .send(Ok(plan_approval_to_server_message(prd_content)))
                    .await;
                self.state = DaemonStreamState::WaitingApprovePlan;
            }
            ExecutionStatus::Completed => {
                self.state = DaemonStreamState::PlanComplete;
            }
            ExecutionStatus::Error(msg) => {
                send_workflow_complete(&self.tx, false, msg).await;
                return true;
            }
            _ => {
                self.state = DaemonStreamState::PlanComplete;
            }
        }

        false
    }

    /// After plan approval, create worktree from origin/master and run the rest of the workflow.
    /// Returns true if the run loop should break (e.g. on error).
    async fn handle_approve_plan(&mut self) -> bool {
        let plan_dir_path = self.plan_dir.as_ref().unwrap();
        let repo = self.repo_root.as_ref().unwrap();

        let planning: PlanningOutput = self
            .engine
            .as_ref()
            .and_then(|e| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(e.get_session(self.session_id.as_ref().unwrap()))
                    .ok()
                    .flatten()
            })
            .and_then(|s| s.context.get_sync::<String>("output"))
            .and_then(|o| parse_planning_response(&o).ok())
            .unwrap_or_else(|| {
                let prd =
                    std::fs::read_to_string(plan_dir_path.join(PRD_FILENAME)).unwrap_or_default();
                PlanningOutput {
                    prd,
                    name: None,
                    discovery: None,
                    demo_plan: None,
                    branch_suggestion: None,
                    worktree_suggestion: None,
                }
            });

        let mut cs = read_changeset(plan_dir_path).unwrap_or_default();
        if cs.branch_suggestion.is_none() {
            cs.branch_suggestion = planning
                .branch_suggestion
                .clone()
                .or_else(|| Some(DEFAULT_BRANCH_SUGGESTION.to_string()));
        }
        if cs.worktree_suggestion.is_none() {
            cs.worktree_suggestion = planning
                .worktree_suggestion
                .clone()
                .or_else(|| planning.name.clone())
                .or_else(|| Some(DEFAULT_WORKTREE_SUGGESTION.to_string()));
        }
        let _ = tddy_core::changeset::write_changeset(plan_dir_path, &cs);

        let worktree_path = match setup_worktree_for_session(repo, plan_dir_path) {
            Ok(p) => p,
            Err(e) => {
                send_workflow_complete(&self.tx, false, e).await;
                return true;
            }
        };

        let eng = self.engine.as_ref().unwrap();
        let sid = self.session_id.as_ref().unwrap();
        let mut updates = std::collections::HashMap::new();
        updates.insert(
            CTX_WORKTREE_DIR.to_string(),
            serde_json::to_value(worktree_path.clone()).unwrap(),
        );
        let rt = tokio::runtime::Handle::current();
        if rt
            .block_on(eng.update_session_context(sid, updates))
            .is_err()
        {
            send_workflow_complete(&self.tx, false, "update session context".to_string()).await;
            return true;
        }

        let tx = self.tx.clone();
        rt.block_on(Self::run_session_until_complete(&tx, eng, sid));
        true
    }

    async fn run_session_until_complete(
        tx: &tokio_mpsc::Sender<Result<ServerMessage, Status>>,
        eng: &WorkflowEngine,
        sid: &str,
    ) {
        let rt = tokio::runtime::Handle::current();
        let mut result = rt
            .block_on(eng.run_session(sid))
            .map_err(|e| Status::internal(format!("run_session: {}", e)))
            .unwrap();

        loop {
            match &result.status {
                ExecutionStatus::Completed => {
                    send_workflow_complete(tx, true, "Workflow complete".to_string()).await;
                    break;
                }
                ExecutionStatus::Error(msg) => {
                    send_workflow_complete(tx, false, msg.clone()).await;
                    break;
                }
                ExecutionStatus::ElicitationNeeded { .. }
                | ExecutionStatus::WaitingForInput { .. } => {
                    let _ = tx
                        .send(Err(Status::unimplemented(
                            "daemon does not support clarification after worktree",
                        )))
                        .await;
                    break;
                }
                ExecutionStatus::Paused { .. } => {
                    result = rt
                        .block_on(eng.run_session(sid))
                        .map_err(|e| Status::internal(format!("run_session: {}", e)))
                        .unwrap();
                }
            }
        }
    }
}
