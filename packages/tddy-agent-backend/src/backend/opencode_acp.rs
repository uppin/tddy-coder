//! OpenCode as a tool-session agent: `opencode acp` driven over the Agent Client Protocol, the same
//! way [`super::CodexAcpBackend`] drives `codex-acp` and [`super::ClaudeAcpBackend`] drives
//! `claude-agent-acp`.
//!
//! # Provider routes
//!
//! On a provider route the agent reads its provider from the configuration file `OPENCODE_CONFIG`
//! names, and that file reads the account's key from the environment. The route's files are written
//! owner-only into a directory the worker owns, so each file lives as long as the agent child and is
//! removed when the worker ends.
//!
//! The route's arguments (`--model <provider>/<model>`) are not passed to `opencode acp`. The
//! generated configuration already selects that model on the provider it declares. The flag could
//! not be checked against a real `opencode acp` here: the `opencode` on the development machine is
//! a wrapper that exits with status 127 when no OpenCode is on `PATH`, so `opencode acp --help` never
//! ran. Passing an argument the agent may not take would fail the whole launch.

use super::routed::route_child_env;
use super::{BackendModels, InvokeRequest, InvokeResponse, ProgressSink, SessionMode};
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;
use crate::stream::ProgressEvent;
use agent_client_protocol::{self as acp, Agent as _, Client};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// The variable `opencode` reads its configuration file's path from.
const OPENCODE_CONFIG_ENV: &str = "OPENCODE_CONFIG";

/// A route's files hold the account's endpoint, so only the owner may read the directory or them.
const OWNER_ONLY_DIR: u32 = 0o700;
const OWNER_ONLY_FILE: u32 = 0o600;

/// Accumulator for the session notifications of one prompt turn.
#[derive(Debug, Default)]
struct OpenCodeAcpAccumulator {
    output: String,
    progress_sink: Option<ProgressSink>,
}

/// A command from the backend to the worker thread.
#[allow(clippy::large_enum_variant)]
enum OpenCodeAcpCommand {
    Prompt {
        request: InvokeRequest,
        response_tx: oneshot::Sender<Result<InvokeResponse, BackendError>>,
    },
    ListModels {
        response_tx: oneshot::Sender<Result<BackendModels, BackendError>>,
    },
}

impl OpenCodeAcpCommand {
    /// Answer a command the agent could not be started for.
    fn fail(self, error: BackendError) {
        match self {
            Self::Prompt { response_tx, .. } => {
                let _ = response_tx.send(Err(error));
            }
            Self::ListModels { response_tx } => {
                let _ = response_tx.send(Err(error));
            }
        }
    }
}

// TODO(opencode-acp): this client and the worker below mirror codex_acp.rs. Share them when the ACP
// backends are consolidated, rather than keeping three copies.
/// Client that accumulates the agent's session notifications and answers its permission requests.
struct TddyOpenCodeAcpClient {
    accumulator: Arc<Mutex<OpenCodeAcpAccumulator>>,
}

#[async_trait::async_trait(?Send)]
impl Client for TddyOpenCodeAcpClient {
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        let mut acc = self
            .accumulator
            .lock()
            .map_err(|_| acp::Error::internal_error())?;
        match args.update {
            acp::SessionUpdate::AgentMessageChunk(chunk) => {
                let text = match chunk.content {
                    acp::ContentBlock::Text(t) => t.text,
                    acp::ContentBlock::Image(_) => "<image>".to_string(),
                    acp::ContentBlock::Audio(_) => "<audio>".to_string(),
                    acp::ContentBlock::ResourceLink(r) => r.uri,
                    acp::ContentBlock::Resource(_) => "<resource>".to_string(),
                    _ => String::new(),
                };
                if !text.is_empty() {
                    acc.output.push_str(&text);
                    if let Some(ref sink) = acc.progress_sink {
                        sink.emit(&ProgressEvent::TaskProgress {
                            description: text,
                            last_tool: None,
                        });
                    }
                }
            }
            acp::SessionUpdate::ToolCall(tc) => {
                if let Some(ref sink) = acc.progress_sink {
                    sink.emit(&ProgressEvent::ToolUse {
                        name: tc.title,
                        detail: None,
                        input_json: tc
                            .raw_input
                            .as_ref()
                            .and_then(|v| serde_json::to_string(v).ok()),
                        call_id: Some(tc.tool_call_id.0.to_string()),
                    });
                }
            }
            acp::SessionUpdate::ToolCallUpdate(upd) => {
                // ACP reports a tool's result as a status change to Completed or Failed on the same
                // tool_call_id, so a ToolResult is emitted only on those states.
                if let (Some(ref sink), Some(status)) = (&acc.progress_sink, upd.fields.status) {
                    let is_error = matches!(status, acp::ToolCallStatus::Failed);
                    if is_error || matches!(status, acp::ToolCallStatus::Completed) {
                        let result_json = upd
                            .fields
                            .raw_output
                            .as_ref()
                            .and_then(|v| serde_json::to_string(v).ok())
                            .unwrap_or_default();
                        sink.emit(&ProgressEvent::ToolResult {
                            call_id: upd.tool_call_id.0.to_string(),
                            result_json,
                            is_error,
                        });
                    }
                }
            }
            acp::SessionUpdate::Plan(plan) => {
                if let Some(ref sink) = acc.progress_sink {
                    for entry in &plan.entries {
                        sink.emit(&ProgressEvent::TaskStarted {
                            description: entry.content.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn request_permission(
        &self,
        _args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        // A headless run approves each tool call once, as the other ACP backends do.
        let allow_id = acp::PermissionOptionId::new("allow-once");
        Ok(acp::RequestPermissionResponse::new(
            acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(allow_id)),
        ))
    }
}

/// The directory a route's files are written into. It is created owner-only, held by the worker
/// that runs the agent, and removed when that worker ends.
struct RouteFiles {
    dir: PathBuf,
    /// The file `OPENCODE_CONFIG` names: the route's first file.
    config: Option<PathBuf>,
}

impl RouteFiles {
    fn write(route: &HarnessRoute) -> io::Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "tddy-opencode-acp-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(name);
        // `create` fails rather than reusing an existing path, so nothing planted there is written to.
        std::fs::DirBuilder::new()
            .mode(OWNER_ONLY_DIR)
            .create(&dir)?;
        // Built before any file is written, so a failed write still removes the directory.
        let mut files = RouteFiles { dir, config: None };
        for file in &route.files {
            let path = files.dir.join(&file.name);
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(OWNER_ONLY_FILE)
                .open(&path)?;
            out.write_all(file.contents.as_bytes())?;
        }
        files.config = route.files.first().map(|first| files.dir.join(&first.name));
        Ok(files)
    }
}

impl Drop for RouteFiles {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            log::warn!(
                "could not remove the OpenCode route files at {}: {e}",
                self.dir.display()
            );
        }
    }
}

/// Runs the OpenCode ACP worker on a dedicated thread with a `LocalSet`.
fn run_opencode_acp_worker(
    agent_path: PathBuf,
    route: Option<HarnessRoute>,
    mut cmd_rx: mpsc::Receiver<OpenCodeAcpCommand>,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("opencode acp worker runtime");
    let local_set = tokio::task::LocalSet::new();
    rt.block_on(local_set.run_until(async move {
        // The route's files are held here until the worker ends, which is when the agent child
        // is no longer reachable through its command channel.
        let (mut child, _route_files) =
            match spawn_opencode_agent(&agent_path, route.as_ref()).await {
                Ok(started) => started,
                Err(e) => {
                    while let Some(cmd) = cmd_rx.recv().await {
                        cmd.fail(e.clone());
                    }
                    return;
                }
            };

        let stdout = child.stdout.take().expect("stdout");
        let stdin = child.stdin.take().expect("stdin");
        let accumulator = Arc::new(Mutex::new(OpenCodeAcpAccumulator::default()));
        let client = TddyOpenCodeAcpClient {
            accumulator: accumulator.clone(),
        };
        let (conn, handle_io) =
            acp::ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |fut| {
                tokio::task::spawn_local(fut);
            });
        tokio::task::spawn_local(handle_io);

        let mut initialized = false;
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                OpenCodeAcpCommand::Prompt {
                    request,
                    response_tx,
                } => {
                    let result = run_prompt(&conn, &accumulator, &mut initialized, request).await;
                    let _ = response_tx.send(result);
                }
                OpenCodeAcpCommand::ListModels { response_tx } => {
                    let result = list_session_models(&conn, &mut initialized).await;
                    let _ = response_tx.send(result);
                }
            }
        }
    }));
}

/// Start `opencode acp`. On a route the child gets the route's environment and `OPENCODE_CONFIG`
/// naming the route's first file. The route's files are returned with the child, for its lifetime.
async fn spawn_opencode_agent(
    agent_path: &Path,
    route: Option<&HarnessRoute>,
) -> Result<(tokio::process::Child, Option<RouteFiles>), BackendError> {
    let route_files = route.map(RouteFiles::write).transpose().map_err(|e| {
        BackendError::InvocationFailed(format!("failed to write the OpenCode route files: {e}"))
    })?;

    let mut cmd = tokio::process::Command::new(agent_path);
    cmd.arg("acp");
    if let (Some(route), Some(files)) = (route, &route_files) {
        cmd.envs(route_child_env(route));
        if let Some(config) = &files.config {
            cmd.env(OPENCODE_CONFIG_ENV, config);
        }
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            let p = agent_path.display();
            BackendError::BinaryNotFound(format!(
                "{p} — install OpenCode on PATH, or set TDDY_OPENCODE_CLI / --opencode-cli-path / coder-config `opencode_cli_path`"
            ))
        } else {
            BackendError::InvocationFailed(e.to_string())
        }
    })?;
    Ok((child, route_files))
}

fn lock_accumulator(
    accumulator: &Mutex<OpenCodeAcpAccumulator>,
) -> Result<MutexGuard<'_, OpenCodeAcpAccumulator>, BackendError> {
    accumulator.lock().map_err(|_| {
        BackendError::InvocationFailed("OpenCode ACP accumulator lock poisoned".to_string())
    })
}

async fn ensure_initialized(
    conn: &acp::ClientSideConnection,
    initialized: &mut bool,
) -> Result<(), BackendError> {
    if *initialized {
        return Ok(());
    }
    let init_req = acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
        acp::Implementation::new("tddy-coder", env!("CARGO_PKG_VERSION")).title("TDDY Coder"),
    );
    conn.initialize(init_req)
        .await
        .map_err(|e| BackendError::InvocationFailed(format!("ACP initialize failed: {e}")))?;
    *initialized = true;
    Ok(())
}

/// One prompt turn. A request that resumes a session loads it; any other request starts one.
async fn run_prompt(
    conn: &acp::ClientSideConnection,
    accumulator: &Mutex<OpenCodeAcpAccumulator>,
    initialized: &mut bool,
    request: InvokeRequest,
) -> Result<InvokeResponse, BackendError> {
    let prompt = super::codex::merge_codex_prompt(&request)?;
    ensure_initialized(conn, initialized).await?;
    *lock_accumulator(accumulator)? = OpenCodeAcpAccumulator {
        output: String::new(),
        progress_sink: request.progress_sink.clone(),
    };

    let cwd = request
        .working_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let session_id = match &request.session {
        Some(SessionMode::Resume(id)) => {
            conn.load_session(acp::LoadSessionRequest::new(
                acp::SessionId::new(id.clone()),
                cwd,
            ))
            .await
            .map_err(|e| BackendError::InvocationFailed(format!("ACP load_session failed: {e}")))?;
            acp::SessionId::new(id.clone())
        }
        _ => {
            conn.new_session(acp::NewSessionRequest::new(cwd))
                .await
                .map_err(|e| {
                    BackendError::InvocationFailed(format!("ACP new_session failed: {e}"))
                })?
                .session_id
        }
    };
    let session_text = session_id.0.to_string();

    conn.prompt(acp::PromptRequest::new(session_id, vec![prompt.into()]))
        .await
        .map_err(|e| BackendError::InvocationFailed(format!("ACP prompt failed: {e}")))?;
    let output = lock_accumulator(accumulator)?.output.clone();
    Ok(InvokeResponse {
        output,
        exit_code: 0,
        session_id: Some(session_text),
        questions: vec![],
        raw_stream: None,
        stderr: None,
    })
}

/// The models the agent advertises for a new session.
async fn list_session_models(
    conn: &acp::ClientSideConnection,
    initialized: &mut bool,
) -> Result<BackendModels, BackendError> {
    ensure_initialized(conn, initialized).await?;
    let cwd = std::env::current_dir().unwrap_or_default();
    let session = conn
        .new_session(acp::NewSessionRequest::new(cwd))
        .await
        .map_err(|e| BackendError::InvocationFailed(format!("ACP new_session failed: {e}")))?;
    super::acp_models_from_session_state(session.models.as_ref())
}

/// `opencode acp` behind [`CodingBackend`](super::CodingBackend).
///
/// The worker thread starts the agent on its first command rather than at construction, so a route
/// set by [`OpenCodeAcpBackend::with_provider_route`] is known before the agent exists.
pub struct OpenCodeAcpBackend {
    agent_path: PathBuf,
    /// The provider route the agent runs on (`with_provider_route`); `None` for the person's own setup.
    provider_route: Option<HarnessRoute>,
    worker: OnceLock<mpsc::Sender<OpenCodeAcpCommand>>,
}

impl std::fmt::Debug for OpenCodeAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenCodeAcpBackend")
            .field("agent_path", &self.agent_path)
            .finish()
    }
}

impl OpenCodeAcpBackend {
    /// Default executable on `PATH`.
    pub const DEFAULT_CLI_BINARY: &'static str = "opencode";

    #[must_use]
    pub fn new() -> Self {
        Self::with_agent_path(PathBuf::from(Self::DEFAULT_CLI_BINARY))
    }

    /// Run the given binary (`opencode`, or a stub agent in tests) as `<binary> acp`.
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        Self {
            agent_path: path,
            provider_route: None,
            worker: OnceLock::new(),
        }
    }

    /// Run the agent on `route`. A worker started before the route was set runs the person's own
    /// setup: dropping its channel ends it, and the next command starts one on the route.
    pub(super) fn set_provider_route(&mut self, route: HarnessRoute) {
        self.provider_route = Some(route);
        self.worker = OnceLock::new();
    }

    /// The worker's command channel, starting the worker on first use.
    fn commands(&self) -> &mpsc::Sender<OpenCodeAcpCommand> {
        self.worker.get_or_init(|| {
            let (cmd_tx, cmd_rx) = mpsc::channel(16);
            let agent_path = self.agent_path.clone();
            let route = self.provider_route.clone();
            std::thread::spawn(move || run_opencode_acp_worker(agent_path, route, cmd_rx));
            cmd_tx
        })
    }
}

impl Default for OpenCodeAcpBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for OpenCodeAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.commands()
            .send(OpenCodeAcpCommand::Prompt {
                request,
                response_tx,
            })
            .await
            .map_err(|_| {
                BackendError::InvocationFailed("OpenCode ACP worker channel closed".to_string())
            })?;
        response_rx.await.map_err(|_| {
            BackendError::InvocationFailed("OpenCode ACP worker dropped response".to_string())
        })?
    }

    fn name(&self) -> &str {
        "opencode"
    }

    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.commands()
            .send(OpenCodeAcpCommand::ListModels { response_tx })
            .await
            .map_err(|_| {
                BackendError::InvocationFailed("OpenCode ACP worker channel closed".to_string())
            })?;
        response_rx.await.map_err(|_| {
            BackendError::InvocationFailed("OpenCode ACP worker dropped response".to_string())
        })?
    }
}
