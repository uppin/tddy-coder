//! ACP backend that communicates with an ACP agent over stdio.
//!
//! Spawns a subprocess (e.g. tddy-acp-stub for tests, or bunx claude-agent-acp for production)
//! and speaks JSON-RPC 2.0 over stdin/stdout using the agent-client-protocol SDK.

use super::{InvokeRequest, InvokeResponse, ProgressSink, SessionMode};
use crate::error::BackendError;
use crate::stream::ProgressEvent;
use agent_client_protocol::{self as acp, Agent as _, Client};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Accumulator for ACP session notifications during a prompt turn.
#[derive(Debug, Default)]
struct AcpAccumulator {
    output: String,
    session_id: Option<String>,
    questions: Vec<super::ClarificationQuestion>,
    progress_sink: Option<ProgressSink>,
}

/// Command sent from ClaudeAcpBackend to the worker thread.
#[allow(clippy::large_enum_variant)]
enum AcpCommand {
    Prompt {
        request: InvokeRequest,
        response_tx: oneshot::Sender<Result<InvokeResponse, BackendError>>,
    },
    #[allow(dead_code)]
    Shutdown,
}

/// Client implementation that accumulates session notifications and handles permission requests.
struct TddyAcpClient {
    accumulator: Arc<Mutex<AcpAccumulator>>,
    auto_approve: Arc<Mutex<bool>>,
}

#[async_trait::async_trait(?Send)]
impl Client for TddyAcpClient {
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        if let Ok(mut acc) = self.accumulator.lock() {
            acc.session_id
                .get_or_insert_with(|| args.session_id.0.to_string());
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
                        });
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
        }
        Ok(())
    }

    async fn request_permission(
        &self,
        _args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        let approve = self.auto_approve.lock().map(|g| *g).unwrap_or(true);
        if approve {
            let allow_id = acp::PermissionOptionId::new("allow-once");
            Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                    allow_id,
                )),
            ))
        } else {
            Err(acp::Error::method_not_found())
        }
    }
}

/// Runs the ACP worker on a dedicated thread with LocalSet.
fn run_acp_worker(
    agent_path: PathBuf,
    agent_args: Vec<String>,
    mut cmd_rx: mpsc::Receiver<AcpCommand>,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("acp worker runtime");
    let local_set = tokio::task::LocalSet::new();
    rt.block_on(local_set.run_until(async move {
        let mut child = match spawn_agent(&agent_path, &agent_args).await {
            Ok(c) => c,
            Err(e) => {
                while let Some(cmd) = cmd_rx.recv().await {
                    if let AcpCommand::Prompt { response_tx, .. } = cmd {
                        let _ = response_tx.send(Err(e.clone()));
                    }
                }
                return;
            }
        };

        let stdout = child.stdout.take().expect("stdout");
        let stdin = child.stdin.take().expect("stdin");
        let outgoing = stdin.compat_write();
        let incoming = stdout.compat();

        let accumulator = Arc::new(Mutex::new(AcpAccumulator::default()));
        let auto_approve_flag = Arc::new(Mutex::new(true));
        let client = TddyAcpClient {
            accumulator: accumulator.clone(),
            auto_approve: auto_approve_flag.clone(),
        };

        let (conn, handle_io) = acp::ClientSideConnection::new(client, outgoing, incoming, |fut| {
            tokio::task::spawn_local(fut);
        });

        tokio::task::spawn_local(handle_io);

        let mut sessions: HashMap<String, String> = HashMap::new();
        let mut initialized = false;

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                AcpCommand::Prompt {
                    request,
                    response_tx,
                } => {
                    let auto_approve = true;
                    if let Ok(mut acc) = accumulator.lock() {
                        *acc = AcpAccumulator {
                            output: String::new(),
                            session_id: None,
                            questions: vec![],
                            progress_sink: request.progress_sink.clone(),
                        };
                    }
                    if let Ok(mut ap) = auto_approve_flag.lock() {
                        *ap = auto_approve;
                    }

                    if !initialized {
                        let init_req = acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                            .client_info(
                                acp::Implementation::new("tddy-coder", env!("CARGO_PKG_VERSION"))
                                    .title("TDDY Coder"),
                            );
                        if let Err(e) = conn.initialize(init_req).await {
                            let _ = response_tx.send(Err(BackendError::InvocationFailed(format!(
                                "ACP initialize failed: {}",
                                e
                            ))));
                            continue;
                        }
                        initialized = true;
                    }

                    let cwd = request
                        .working_dir
                        .clone()
                        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                    let acp_session_id = match &request.session {
                        Some(SessionMode::Resume(id)) => sessions.get(id).cloned(),
                        _ => None,
                    };
                    let acp_session_id = match acp_session_id {
                        Some(sid) => acp::SessionId::new(sid),
                        None => {
                            let new_req = acp::NewSessionRequest::new(cwd);
                            match conn.new_session(new_req).await {
                                Ok(r) => {
                                    let sid = r.session_id.0.clone();
                                    if let Some(SessionMode::Fresh(id)) = &request.session {
                                        sessions.insert(id.clone(), sid.to_string());
                                    }
                                    acp::SessionId::new(sid)
                                }
                                Err(e) => {
                                    let _ = response_tx.send(Err(BackendError::InvocationFailed(
                                        format!("ACP new_session failed: {}", e),
                                    )));
                                    continue;
                                }
                            }
                        }
                    };

                    let acp_session_id_str = acp_session_id.0.to_string();
                    let prompt_blocks: Vec<acp::ContentBlock> = vec![request.prompt.clone().into()];
                    let prompt_req = acp::PromptRequest::new(acp_session_id, prompt_blocks);
                    match conn.prompt(prompt_req).await {
                        Ok(_) => {
                            let acc = accumulator.lock().map(|a| AcpAccumulator {
                                output: a.output.clone(),
                                session_id: a.session_id.clone(),
                                questions: a.questions.clone(),
                                progress_sink: None,
                            });
                            let (output, session_id, questions) = match acc {
                                Ok(a) => (
                                    a.output,
                                    a.session_id.or_else(|| Some(acp_session_id_str.clone())),
                                    a.questions,
                                ),
                                Err(_) => (String::new(), Some(acp_session_id_str), vec![]),
                            };
                            let resp = InvokeResponse {
                                output,
                                exit_code: 0,
                                session_id,
                                questions,
                                raw_stream: None,
                                stderr: None,
                            };
                            let _ = response_tx.send(Ok(resp));
                        }
                        Err(e) => {
                            let _ = response_tx.send(Err(BackendError::InvocationFailed(format!(
                                "ACP prompt failed: {}",
                                e
                            ))));
                        }
                    }
                }
                AcpCommand::Shutdown => break,
            }
        }
    }));
}

async fn spawn_agent(
    agent_path: &PathBuf,
    agent_args: &[String],
) -> Result<tokio::process::Child, BackendError> {
    let mut cmd = tokio::process::Command::new(agent_path);
    for arg in agent_args {
        cmd.arg(arg);
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BackendError::BinaryNotFound(agent_path.to_string_lossy().to_string())
        } else {
            BackendError::InvocationFailed(e.to_string())
        }
    })
}

/// Backend that invokes an ACP agent subprocess via the Agent Client Protocol.
pub struct ClaudeAcpBackend {
    agent_path: PathBuf,
    agent_args: Vec<String>,
    cmd_tx: mpsc::Sender<AcpCommand>,
}

impl std::fmt::Debug for ClaudeAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeAcpBackend")
            .field("agent_path", &self.agent_path)
            .field("agent_args", &self.agent_args)
            .finish()
    }
}

impl Default for ClaudeAcpBackend {
    fn default() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        std::thread::spawn(move || {
            run_acp_worker(
                PathBuf::from("bunx"),
                vec!["claude-agent-acp".to_string()],
                cmd_rx,
            );
        });
        Self {
            agent_path: PathBuf::from("bunx"),
            agent_args: vec!["claude-agent-acp".to_string()],
            cmd_tx,
        }
    }
}

impl ClaudeAcpBackend {
    /// Create a new backend using the default agent (bunx claude-agent-acp).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a backend that spawns the given binary (for tests with tddy-acp-stub).
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        std::thread::spawn(move || {
            run_acp_worker(path, Vec::new(), cmd_rx);
        });
        Self {
            agent_path: PathBuf::from(""),
            agent_args: Vec::new(),
            cmd_tx,
        }
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for ClaudeAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.cmd_tx
            .send(AcpCommand::Prompt {
                request,
                response_tx,
            })
            .await
            .map_err(|_| BackendError::InvocationFailed("ACP worker channel closed".to_string()))?;
        response_rx.await.map_err(|_| {
            BackendError::InvocationFailed("ACP worker dropped response".to_string())
        })?
    }

    fn name(&self) -> &str {
        "claude-acp"
    }
}
