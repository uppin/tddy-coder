//! DaemonService — TddyRemote implementation for headless daemon mode.
//!
//! Implements GetSession, ListSessions, and Stream (StartSession flow).

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::mpsc;

use futures_util::{Stream, StreamExt};
use tokio::sync::mpsc as tokio_mpsc;
use tonic::{Request, Response, Status};

use tddy_core::output::{create_session_dir_under, SESSIONS_SUBDIR};
use tddy_core::read_changeset;
use tddy_core::session_lifecycle::validate_session_id_segment;
use tddy_core::workflow::graph::{ElicitationEvent, ExecutionStatus};
use tddy_core::workflow::session::workflow_engine_storage_dir;
use tddy_core::{
    setup_worktree_for_session_with_optional_chain_base, SharedBackend, WorkflowEngine,
    WorkflowRecipe,
};
use tddy_workflow_recipes::{parse_planning_response_with_base, PlanningOutput};
use tddy_workflow_recipes::{workflow_recipe_and_manifest_from_cli_name, SessionArtifactManifest};

use crate::convert::{
    session_document_approval_to_server_message, workflow_event_to_server_message,
};
use crate::gen::{
    client_message, server_message, tddy_remote_server::TddyRemote, GetSessionRequest,
    GetSessionResponse, ListSessionsRequest, ListSessionsResponse, ServerMessage, SessionCreated,
    SessionInfo, WorkflowComplete,
};

// --- Constants ---

const STREAM_CHANNEL_CAPACITY: usize = 64;
const DEFAULT_BRANCH_SUGGESTION: &str = "feature/impl";
const DEFAULT_WORKTREE_SUGGESTION: &str = "feature-impl";
const CTX_FEATURE_INPUT: &str = "feature_input";
const CTX_OUTPUT_DIR: &str = "output_dir";
const CTX_SESSION_BASE: &str = "session_base";
const CTX_SESSION_ID: &str = "session_id";
const CTX_SESSION_DIR: &str = "session_dir";
const CTX_WORKTREE_DIR: &str = "worktree_dir";
const CHANGESET_FILENAME: &str = "changeset.yaml";

// --- Helpers ---

/// The client-message source `DaemonStreamHandler` reads from — boxed so the *same* handler and
/// state machine serve both the plain-gRPC transport (a real `tonic::codec::Streaming`, which
/// already implements `Stream`) and the LiveKit/RpcService transport (a `tddy_rpc::Streaming`,
/// fed via a byte-roundtrip adapter — see `livekit_transport` below). Everything else about
/// `DaemonStreamHandler` (the session/workflow state machine, `tx`, disk I/O) is transport-
/// agnostic already and needs no change.
type ClientMessageStream =
    Pin<Box<dyn Stream<Item = Result<crate::gen::ClientMessage, Status>> + Send>>;

/// Receive next client message. On Ok(None) or Err, returns None and may send error to tx.
async fn recv_client_message(
    client_stream: &mut ClientMessageStream,
    tx: &tokio_mpsc::Sender<Result<ServerMessage, Status>>,
) -> Option<crate::gen::ClientMessage> {
    match client_stream.next().await {
        Some(Ok(m)) => Some(m),
        Some(Err(e)) => {
            let _ = tx
                .send(Err(Status::internal(format!("stream error: {}", e))))
                .await;
            None
        }
        None => None,
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
    runtime: tokio::runtime::Handle,
) {
    std::thread::spawn(move || {
        while let Ok(ev) = rx.recv() {
            if let Some(msg) = workflow_event_to_server_message(ev) {
                let _ = runtime.block_on(tx.send(Ok(msg)));
            }
        }
    });
}

/// Validates `StartSession.repo_root` for the TddyRemote bidirectional stream.
/// Must be non-empty after trim (absolute path to the git repository root).
pub(crate) fn resolve_start_session_repo(repo_root: &str) -> Result<PathBuf, String> {
    let t = repo_root.trim();
    if t.is_empty() {
        return Err(
            "StartSession.repo_root is required: set the absolute path to the git repository root \
             (the project directory, same as used for tddy-coder --daemon cwd / .session.yaml repo_path)"
                .to_string(),
        );
    }
    Ok(PathBuf::from(t))
}

/// A session this `DaemonService` is already bound to — set when the daemon spawned this exact
/// `tddy-coder --daemon` process for a session that `ConnectionService.StartSession` (and
/// `run_with_args`) already created on disk, complete with a `changeset.yaml` that has `recipe`
/// set. Browser clients (the PR-Stack Chat Screen) never send a `StartSession` intent over
/// `TddyRemote.Stream` for such a session — they send `SubmitFeatureInput` directly, the same
/// first message `TddyRemoteService`'s protocol uses. See `DaemonStreamHandler::run`.
#[derive(Clone)]
struct BoundSession {
    session_id: String,
    repo_root: PathBuf,
}

/// Daemon gRPC service. Reads session state from disk; runs workflow on Stream.
#[derive(Clone)]
pub struct DaemonService {
    tddy_data_dir: PathBuf,
    backend: SharedBackend,
    workflow_recipe: std::sync::Arc<dyn WorkflowRecipe>,
    artifact_manifest: std::sync::Arc<dyn SessionArtifactManifest>,
    bound_session: Option<BoundSession>,
}

impl DaemonService {
    pub fn new(tddy_data_dir: PathBuf, backend: SharedBackend) -> Self {
        let (workflow_recipe, artifact_manifest) =
            workflow_recipe_and_manifest_from_cli_name("tdd").expect("tdd always resolves");
        Self::with_workflow_recipe(tddy_data_dir, backend, workflow_recipe, artifact_manifest)
    }

    /// Use a specific workflow recipe (e.g. TDD, bug-fix) and matching session-artifact manifest.
    /// Default [`new`] uses [`workflow_recipe_and_manifest_from_cli_name`] with `"tdd"`.
    pub fn with_workflow_recipe(
        tddy_data_dir: PathBuf,
        backend: SharedBackend,
        workflow_recipe: std::sync::Arc<dyn WorkflowRecipe>,
        artifact_manifest: std::sync::Arc<dyn SessionArtifactManifest>,
    ) -> Self {
        Self {
            tddy_data_dir,
            backend,
            workflow_recipe,
            artifact_manifest,
            bound_session: None,
        }
    }

    /// A `DaemonService` bound to a session that already exists on disk — used by `run_daemon`,
    /// which is always spawned for one specific `session_id` that `ConnectionService.StartSession`
    /// already created (changeset + `.session.yaml` already written). `Stream`'s first message is
    /// expected to be `SubmitFeatureInput`, not `StartSession` — see [`BoundSession`].
    pub fn for_session(
        tddy_data_dir: PathBuf,
        backend: SharedBackend,
        session_id: String,
        repo_root: PathBuf,
    ) -> Self {
        let mut service = Self::new(tddy_data_dir, backend);
        service.bound_session = Some(BoundSession {
            session_id,
            repo_root,
        });
        service
    }
}

#[tonic::async_trait]
impl TddyRemote for DaemonService {
    type StreamStream = tokio_stream::wrappers::ReceiverStream<Result<ServerMessage, Status>>;

    async fn stream(
        &self,
        request: Request<tonic::codec::Streaming<crate::gen::ClientMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let (tx, rx) = tokio_mpsc::channel(STREAM_CHANNEL_CAPACITY);
        let handler = DaemonStreamHandler::new(
            self.tddy_data_dir.clone(),
            self.backend.clone(),
            self.workflow_recipe.clone(),
            self.artifact_manifest.clone(),
            Box::pin(request.into_inner()),
            tx,
            self.bound_session.clone(),
        );
        tokio::spawn(handler.run());
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let session_id = request.into_inner().session_id;
        validate_session_id_segment(&session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let session_dir = self.tddy_data_dir.join(SESSIONS_SUBDIR).join(&session_id);
        let changeset = read_changeset(&session_dir)
            .map_err(|e| Status::not_found(format!("session not found: {} — {}", session_id, e)))?;

        let status = self
            .workflow_recipe
            .status_for_state(&changeset.state.current);
        let session_dir_str = session_dir.to_string_lossy().to_string();
        let worktree = changeset.worktree.clone().unwrap_or_default();
        let branch = changeset.branch.clone().unwrap_or_default();

        Ok(Response::new(GetSessionResponse {
            session: Some(SessionInfo {
                session_id: session_id.clone(),
                status: status.to_string(),
                session_dir: session_dir_str,
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

        let sessions_root = self.tddy_data_dir.join(SESSIONS_SUBDIR);
        if !sessions_root.exists() {
            return Ok(Response::new(ListSessionsResponse { sessions }));
        }

        let entries = std::fs::read_dir(&sessions_root)
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
                        let status = self
                            .workflow_recipe
                            .status_for_state(&changeset.state.current);
                        sessions.push(SessionInfo {
                            session_id,
                            status: status.to_string(),
                            session_dir: path.to_string_lossy().to_string(),
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

/// Second transport for the same `DaemonService`: an `RpcService`-compatible impl so it can be
/// registered in a LiveKit `MultiRpcService` (the plain-gRPC impl above only satisfies tonic's
/// server trait, which `MultiRpcService` cannot wrap — this is the gap that left `run_daemon`'s
/// pr-stack sessions unable to receive chat messages at all). Mirrors
/// `crate::service::livekit_transport`'s pattern for `TddyRemoteService`.
///
/// `get_session`/`list_sessions` delegate to the existing tonic-trait methods above via a byte
/// roundtrip at the request/response boundary — no disk-reading logic is duplicated. `stream`
/// cannot delegate the same way (`tonic::codec::Streaming` isn't constructible from arbitrary
/// data), so it builds its own `DaemonStreamHandler` directly — the *only* reason
/// `DaemonStreamHandler.client_stream` was changed from a concrete `tonic::codec::Streaming` to
/// a boxed `ClientMessageStream`: both transports' streams already implement `Stream`, so the
/// same handler and state machine serve either one unmodified.
mod livekit_transport {
    use futures_util::StreamExt;
    use prost::Message;
    use tokio_stream::wrappers::ReceiverStream;

    use crate::gen::{
        tddy_remote_server::TddyRemote as TonicTddyRemote, GetSessionRequest as TonicGetSessionRequest,
        ListSessionsRequest as TonicListSessionsRequest,
        ListSessionsResponse as TonicListSessionsResponse,
    };
    use crate::proto::tddy_remote::{
        ClientMessage, GetSessionRequest, GetSessionResponse, ListSessionsRequest,
        ListSessionsResponse, ServerMessage, TddyRemote,
    };

    use super::{ClientMessageStream, DaemonService, DaemonStreamHandler, STREAM_CHANNEL_CAPACITY};

    /// Both message types are compiled from the same .proto and are therefore wire-compatible —
    /// re-encoding one and decoding the other is exact, and avoids duplicating conversion logic
    /// or business logic between the two independently-codegen'd type sets.
    fn roundtrip<From: Message, To: Message + Default>(msg: From) -> To {
        To::decode(&msg.encode_to_vec()[..])
            .expect("message is wire-compatible across both TddyRemote codegen passes")
    }

    fn tonic_status_to_rpc(status: tonic::Status) -> tddy_rpc::Status {
        use tonic::Code;
        let msg = status.message().to_string();
        match status.code() {
            Code::NotFound => tddy_rpc::Status::not_found(msg),
            Code::InvalidArgument => tddy_rpc::Status::invalid_argument(msg),
            Code::DeadlineExceeded => tddy_rpc::Status::deadline_exceeded(msg),
            Code::Unimplemented => tddy_rpc::Status::unimplemented(msg),
            Code::Unauthenticated => tddy_rpc::Status::unauthenticated(msg),
            Code::PermissionDenied => tddy_rpc::Status::permission_denied(msg),
            Code::FailedPrecondition => tddy_rpc::Status::failed_precondition(msg),
            _ => tddy_rpc::Status::internal(msg),
        }
    }

    #[async_trait::async_trait]
    impl TddyRemote for DaemonService {
        type StreamStream = ReceiverStream<Result<ServerMessage, tddy_rpc::Status>>;

        async fn stream(
            &self,
            request: tddy_rpc::Request<tddy_rpc::Streaming<ClientMessage>>,
        ) -> Result<tddy_rpc::Response<Self::StreamStream>, tddy_rpc::Status> {
            let input: ClientMessageStream = Box::pin(request.into_inner().map(|item| {
                item.map(roundtrip)
                    .map_err(|e: tddy_rpc::Status| tonic::Status::internal(e.message().to_string()))
            }));

            let (tonic_tx, mut tonic_rx) = tokio::sync::mpsc::channel(STREAM_CHANNEL_CAPACITY);
            let handler = DaemonStreamHandler::new(
                self.tddy_data_dir.clone(),
                self.backend.clone(),
                self.workflow_recipe.clone(),
                self.artifact_manifest.clone(),
                input,
                tonic_tx,
                self.bound_session.clone(),
            );
            tokio::spawn(handler.run());

            let (out_tx, out_rx) = tokio::sync::mpsc::channel(STREAM_CHANNEL_CAPACITY);
            tokio::spawn(async move {
                while let Some(item) = tonic_rx.recv().await {
                    let converted = match item {
                        Ok(msg) => Ok(roundtrip(msg)),
                        Err(status) => Err(tonic_status_to_rpc(status)),
                    };
                    if out_tx.send(converted).await.is_err() {
                        break;
                    }
                }
            });

            Ok(tddy_rpc::Response::new(ReceiverStream::new(out_rx)))
        }

        async fn get_session(
            &self,
            request: tddy_rpc::Request<GetSessionRequest>,
        ) -> Result<tddy_rpc::Response<GetSessionResponse>, tddy_rpc::Status> {
            let tonic_req = tonic::Request::new(roundtrip::<_, TonicGetSessionRequest>(
                request.into_inner(),
            ));
            let tonic_resp = <DaemonService as TonicTddyRemote>::get_session(self, tonic_req)
                .await
                .map_err(tonic_status_to_rpc)?;
            Ok(tddy_rpc::Response::new(roundtrip(tonic_resp.into_inner())))
        }

        async fn list_sessions(
            &self,
            request: tddy_rpc::Request<ListSessionsRequest>,
        ) -> Result<tddy_rpc::Response<ListSessionsResponse>, tddy_rpc::Status> {
            let tonic_req = tonic::Request::new(roundtrip::<_, TonicListSessionsRequest>(
                request.into_inner(),
            ));
            let tonic_resp = <DaemonService as TonicTddyRemote>::list_sessions(self, tonic_req)
                .await
                .map_err(tonic_status_to_rpc)?;
            Ok(tddy_rpc::Response::new(roundtrip::<
                TonicListSessionsResponse,
                ListSessionsResponse,
            >(tonic_resp.into_inner())))
        }
    }
}

#[derive(Clone)]
enum DaemonStreamState {
    Idle,
    WaitingSessionDocumentApproval,
    PlanComplete,
}

/// Handles the bidirectional stream: receives ClientMessage, runs workflow, sends ServerMessage.
struct DaemonStreamHandler {
    tddy_data_dir: PathBuf,
    backend: SharedBackend,
    workflow_recipe: std::sync::Arc<dyn WorkflowRecipe>,
    artifact_manifest: std::sync::Arc<dyn SessionArtifactManifest>,
    client_stream: ClientMessageStream,
    tx: tokio_mpsc::Sender<Result<ServerMessage, Status>>,
    state: DaemonStreamState,
    session_id: Option<String>,
    session_dir: Option<PathBuf>,
    repo_root: Option<PathBuf>,
    engine: Option<WorkflowEngine>,
    bound_session: Option<BoundSession>,
}

impl DaemonStreamHandler {
    fn new(
        tddy_data_dir: PathBuf,
        backend: SharedBackend,
        workflow_recipe: std::sync::Arc<dyn WorkflowRecipe>,
        artifact_manifest: std::sync::Arc<dyn SessionArtifactManifest>,
        client_stream: ClientMessageStream,
        tx: tokio_mpsc::Sender<Result<ServerMessage, Status>>,
        bound_session: Option<BoundSession>,
    ) -> Self {
        Self {
            tddy_data_dir,
            backend,
            workflow_recipe,
            artifact_manifest,
            client_stream,
            tx,
            state: DaemonStreamState::Idle,
            session_id: None,
            session_dir: None,
            repo_root: None,
            engine: None,
            bound_session,
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
                    } else if let (Some(client_message::Intent::SubmitFeatureInput(input)), true) =
                        (&msg.intent, self.bound_session.is_some())
                    {
                        self.handle_submit_feature_input_for_bound_session(input.text.clone())
                            .await
                    } else {
                        false
                    }
                }
                DaemonStreamState::WaitingSessionDocumentApproval => {
                    if let Some(client_message::Intent::ApproveSessionDocument(_)) = msg.intent {
                        if self.handle_approve_session_document().await {
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
        let recipe_key = start.recipe.trim();
        let recipe_key = if recipe_key.is_empty() {
            "tdd"
        } else {
            recipe_key
        };
        log::info!(
            "handle_start_session: recipe from client={:?} (normalized={})",
            start.recipe,
            recipe_key
        );

        let (workflow_recipe, artifact_manifest) =
            match workflow_recipe_and_manifest_from_cli_name(recipe_key) {
                Ok(pair) => pair,
                Err(msg) => {
                    send_workflow_complete(&self.tx, false, msg).await;
                    return true;
                }
            };

        self.workflow_recipe = workflow_recipe.clone();
        self.artifact_manifest = artifact_manifest.clone();

        let repo = match resolve_start_session_repo(&start.repo_root) {
            Ok(p) => p,
            Err(e) => {
                send_workflow_complete(&self.tx, false, e).await;
                return true;
            }
        };

        if let Err(e) = std::fs::create_dir_all(&self.tddy_data_dir) {
            send_workflow_complete(&self.tx, false, format!("create sessions base: {}", e)).await;
            return true;
        }

        let sid = uuid::Uuid::now_v7().to_string();
        let plan = create_session_dir_under(&self.tddy_data_dir, &sid)
            .map_err(|e| Status::internal(format!("create session dir: {}", e)))
            .unwrap();

        let init_cs = tddy_core::changeset::Changeset {
            initial_prompt: Some(prompt.clone()),
            recipe: Some(recipe_key.to_string()),
            ..tddy_core::changeset::Changeset::default()
        };
        let _ = tddy_core::changeset::write_changeset(&plan, &init_cs);
        log::debug!(
            "handle_start_session: wrote changeset.yaml with recipe={}",
            recipe_key
        );

        // std::env::Args is not Send; bind the resolved Option<String> before the `if let` below
        // so no non-Send temporary is live across the error branch's `.await`.
        let invoked_tool_path = std::env::args().next();
        if let Err(e) = tddy_core::write_initial_tool_session_metadata(
            &plan,
            tddy_core::InitialToolSessionMetadataOpts {
                project_id: String::new(),
                repo_path: Some(repo.display().to_string()),
                pid: Some(std::process::id()),
                tool: invoked_tool_path,
                livekit_room: None,
                previous_session_id: None,
                session_type: None,
                model: None,
                activity_status: None,
                hook_token: None,
                sandbox: None,
            },
        ) {
            send_workflow_complete(&self.tx, false, format!("write session metadata: {}", e)).await;
            return true;
        }

        self.run_goal_for_session(sid, plan, repo, workflow_recipe, prompt, true)
            .await
    }

    /// `SubmitFeatureInput` arriving as the first message on a fresh `Stream` call, for a
    /// `DaemonService` bound to a session that already exists on disk (see [`BoundSession`]).
    /// Unlike [`handle_start_session`](Self::handle_start_session), this never creates a new
    /// session directory or overwrites the already-recorded `recipe` — it loads the existing
    /// changeset, records the submitted prompt into it, and runs the same workflow-engine
    /// kickoff against the session that was already there.
    async fn handle_submit_feature_input_for_bound_session(&mut self, text: String) -> bool {
        let bound = self
            .bound_session
            .clone()
            .expect("caller only invokes this when bound_session is Some");

        let plan = match create_session_dir_under(&self.tddy_data_dir, &bound.session_id) {
            Ok(p) => p,
            Err(e) => {
                send_workflow_complete(&self.tx, false, format!("open session dir: {}", e)).await;
                return true;
            }
        };

        let mut cs = read_changeset(&plan).unwrap_or_default();
        let recipe_key = cs.recipe.clone().unwrap_or_else(|| "tdd".to_string());
        let (workflow_recipe, artifact_manifest) =
            match workflow_recipe_and_manifest_from_cli_name(&recipe_key) {
                Ok(pair) => pair,
                Err(msg) => {
                    send_workflow_complete(&self.tx, false, msg).await;
                    return true;
                }
            };
        self.workflow_recipe = workflow_recipe.clone();
        self.artifact_manifest = artifact_manifest;

        cs.initial_prompt = Some(text.clone());
        if let Err(e) = tddy_core::changeset::write_changeset(&plan, &cs) {
            send_workflow_complete(&self.tx, false, format!("write changeset: {}", e)).await;
            return true;
        }

        self.run_goal_for_session(
            bound.session_id.clone(),
            plan,
            bound.repo_root.clone(),
            workflow_recipe,
            text,
            false,
        )
        .await
    }

    /// Shared tail of [`handle_start_session`](Self::handle_start_session) and
    /// [`handle_submit_feature_input_for_bound_session`](Self::handle_submit_feature_input_for_bound_session):
    /// set up the workflow engine and run the recipe's start goal against `plan`/`repo`, driving
    /// `self.state` from the result. `announce_session_created` sends a `SessionCreated` event
    /// first — only meaningful for a session the caller didn't already know the id of.
    async fn run_goal_for_session(
        &mut self,
        sid: String,
        plan: PathBuf,
        repo: PathBuf,
        workflow_recipe: std::sync::Arc<dyn WorkflowRecipe>,
        prompt: String,
        announce_session_created: bool,
    ) -> bool {
        self.session_id = Some(sid.clone());
        self.session_dir = Some(plan.clone());
        self.repo_root = Some(repo.clone());

        let workflow_storage = workflow_engine_storage_dir(&plan);
        if let Err(e) = std::fs::create_dir_all(&workflow_storage) {
            send_workflow_complete(&self.tx, false, format!("create workflow storage: {}", e))
                .await;
            return true;
        }

        let (event_tx, rx) = mpsc::channel();
        let rt_handle = tokio::runtime::Handle::current();
        spawn_event_forwarder(rx, self.tx.clone(), rt_handle);
        let hooks = workflow_recipe.create_hooks(Some(event_tx));
        let eng = WorkflowEngine::new(
            workflow_recipe.clone(),
            self.backend.clone(),
            workflow_storage,
            Some(hooks),
        );
        self.engine = Some(eng);

        let mut ctx = std::collections::HashMap::new();
        ctx.insert(CTX_FEATURE_INPUT.to_string(), serde_json::json!(prompt));
        ctx.insert(
            CTX_OUTPUT_DIR.to_string(),
            serde_json::to_value(repo.clone()).unwrap(),
        );
        ctx.insert(
            CTX_SESSION_BASE.to_string(),
            serde_json::to_value(self.tddy_data_dir.clone()).unwrap(),
        );
        ctx.insert(CTX_SESSION_ID.to_string(), serde_json::json!(sid.clone()));
        ctx.insert(
            CTX_SESSION_DIR.to_string(),
            serde_json::to_value(plan.clone()).unwrap(),
        );
        let conv_path = plan.join("logs").join("conversation.jsonl");
        if let Some(parent) = conv_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        ctx.insert(
            "conversation_output_path".to_string(),
            serde_json::to_value(conv_path).unwrap(),
        );

        if announce_session_created {
            let _ = self
                .tx
                .send(Ok(ServerMessage {
                    event: Some(server_message::Event::SessionCreated(SessionCreated {
                        session_id: sid.clone(),
                    })),
                }))
                .await;
        }

        let plan_goal = workflow_recipe.start_goal();
        let result = self
            .engine
            .as_ref()
            .unwrap()
            .run_goal(&plan_goal, ctx)
            .await
            .map_err(|e| Status::internal(format!("run_goal: {}", e)))
            .unwrap();

        match result.status {
            ExecutionStatus::ElicitationNeeded {
                event: ElicitationEvent::DocumentApproval { content },
            } => {
                let _ = self
                    .tx
                    .send(Ok(session_document_approval_to_server_message(content)))
                    .await;
                self.state = DaemonStreamState::WaitingSessionDocumentApproval;
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

    /// After session-document approval, create worktree from origin/master and run the rest of the workflow.
    /// Returns true if the run loop should break (e.g. on error).
    async fn handle_approve_session_document(&mut self) -> bool {
        let session_dir_path = self.session_dir.as_ref().unwrap();
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
            .and_then(|o| parse_planning_response_with_base(&o, session_dir_path).ok())
            .unwrap_or_else(|| {
                let prd = self
                    .workflow_recipe
                    .read_primary_session_document_utf8(session_dir_path)
                    .unwrap_or_default();
                log::debug!(
                    "[daemon] approve_session_document fallback planning from primary session document"
                );
                PlanningOutput {
                    prd,
                    name: None,
                    discovery: None,
                    demo_plan: None,
                    branch_suggestion: None,
                    worktree_suggestion: None,
                }
            });

        let mut cs = read_changeset(session_dir_path).unwrap_or_default();
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
        if cs.workflow.as_ref().and_then(|w| w.branch_worktree_intent)
            == Some(tddy_core::changeset::BranchWorktreeIntent::NewBranchFromBase)
        {
            if let Some(ref b) = cs.branch_suggestion {
                if !b.trim().is_empty() {
                    cs.workflow
                        .get_or_insert_with(Default::default)
                        .new_branch_name = Some(b.clone());
                }
            }
        }
        let _ = tddy_core::changeset::write_changeset(session_dir_path, &cs);

        let chain_opt = cs.worktree_integration_base_ref.as_deref();
        let worktree_path = match setup_worktree_for_session_with_optional_chain_base(
            repo,
            session_dir_path,
            chain_opt,
        ) {
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

#[cfg(test)]
mod start_session_repo_tests {
    use super::resolve_start_session_repo;
    use std::path::PathBuf;

    #[test]
    fn empty_repo_root_is_rejected() {
        // Given
        let blank = "";
        let whitespace_only = "   ";

        // When / Then
        assert!(
            resolve_start_session_repo(blank).is_err(),
            "empty string must be rejected"
        );
        assert!(
            resolve_start_session_repo(whitespace_only).is_err(),
            "whitespace-only string must be rejected"
        );
    }

    #[test]
    fn non_empty_repo_root_is_accepted() {
        // Given
        let p = PathBuf::from("/tmp/tddy-test-repo-root");

        // When / Then
        assert_eq!(
            resolve_start_session_repo("/tmp/tddy-test-repo-root").unwrap(),
            p
        );
    }
}
