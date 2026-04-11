//! ACP backend for OpenAI Codex via the **`codex-acp`** subprocess (Agent Client Protocol).
//!
//! Spawns `codex-acp` on stdio (or **`tddy-acp-stub`** in tests). Resume uses ACP **`load_session`**
//! with the Codex thread id stored as **`codex_thread_id`**. When the agent reports an auth-related
//! error and [`InvokeRequest::session_dir`] is set, runs **`codex login`** with the same
//! **`BROWSER`** / **`TDDY_CODEX_OAUTH_OUT`** path as [`super::CodexBackend`] so tddy-web can poll
//! [`super::CODEX_OAUTH_AUTHORIZE_URL_FILENAME`].

use super::{InvokeRequest, InvokeResponse, ProgressSink, SessionMode};
use crate::error::BackendError;
use crate::stream::ProgressEvent;
use agent_client_protocol::{self as acp, Agent as _, Client};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[derive(Debug, Default)]
struct CodexAcpAccumulator {
    output: String,
    session_id: Option<String>,
    questions: Vec<super::ClarificationQuestion>,
    progress_sink: Option<ProgressSink>,
}

#[allow(clippy::large_enum_variant)]
enum CodexAcpCommand {
    Prompt {
        request: InvokeRequest,
        response_tx: oneshot::Sender<Result<InvokeResponse, BackendError>>,
    },
    #[allow(dead_code)]
    Shutdown,
}

struct TddyCodexAcpClient {
    accumulator: Arc<Mutex<CodexAcpAccumulator>>,
    auto_approve: Arc<Mutex<bool>>,
}

#[async_trait::async_trait(?Send)]
impl Client for TddyCodexAcpClient {
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

fn acp_error_suggests_retry_oauth(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("auth")
        || m.contains("401")
        || m.contains("unauthorized")
        || m.contains("login")
        || m.contains("credential")
        || m.contains("not signed")
}

fn invoke_codex_oauth_login_blocking(
    codex_bin: PathBuf,
    session_dir: PathBuf,
) -> Result<(), BackendError> {
    let backend = super::CodexBackend::with_path(codex_bin);
    let mut child = backend.spawn_oauth_login(&session_dir)?;
    let status = child
        .wait()
        .map_err(|e| BackendError::InvocationFailed(format!("codex login wait: {e}")))?;
    if !status.success() {
        return Err(BackendError::InvocationFailed(format!(
            "codex login exited with status {:?}",
            status.code()
        )));
    }
    Ok(())
}

async fn run_oauth_login_via_codex_cli(
    codex_cli_path: PathBuf,
    session_dir: PathBuf,
) -> Result<(), BackendError> {
    tokio::task::spawn_blocking(move || {
        invoke_codex_oauth_login_blocking(codex_cli_path, session_dir)
    })
    .await
    .map_err(|e| BackendError::InvocationFailed(format!("codex login join: {e}")))?
}

/// Runs the Codex ACP worker on a dedicated thread with `LocalSet`.
fn run_codex_acp_worker(
    agent_path: PathBuf,
    agent_args: Vec<String>,
    codex_cli_path: PathBuf,
    mut cmd_rx: mpsc::Receiver<CodexAcpCommand>,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("codex acp worker runtime");
    let local_set = tokio::task::LocalSet::new();
    rt.block_on(local_set.run_until(async move {
        let mut child = match spawn_codex_acp_agent(&agent_path, &agent_args).await {
            Ok(c) => c,
            Err(e) => {
                while let Some(cmd) = cmd_rx.recv().await {
                    if let CodexAcpCommand::Prompt { response_tx, .. } = cmd {
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

        let accumulator = Arc::new(Mutex::new(CodexAcpAccumulator::default()));
        let auto_approve_flag = Arc::new(Mutex::new(true));
        let client = TddyCodexAcpClient {
            accumulator: accumulator.clone(),
            auto_approve: auto_approve_flag.clone(),
        };

        let (conn, handle_io) = acp::ClientSideConnection::new(client, outgoing, incoming, |fut| {
            tokio::task::spawn_local(fut);
        });

        tokio::task::spawn_local(handle_io);

        let mut initialized = false;

        'cmd_loop: while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                CodexAcpCommand::Prompt {
                    request,
                    response_tx,
                } => {
                    let auto_approve = true;
                    if let Ok(mut acc) = accumulator.lock() {
                        *acc = CodexAcpAccumulator {
                            output: String::new(),
                            session_id: None,
                            questions: vec![],
                            progress_sink: request.progress_sink.clone(),
                        };
                    }
                    if let Ok(mut ap) = auto_approve_flag.lock() {
                        *ap = auto_approve;
                    }

                    let merged_prompt = match super::codex::merge_codex_prompt(&request) {
                        Ok(s) => s,
                        Err(e) => {
                            let _ = response_tx.send(Err(e));
                            continue;
                        }
                    };

                    if !initialized {
                        let init_req = acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                            .client_info(
                                acp::Implementation::new("tddy-coder", env!("CARGO_PKG_VERSION"))
                                    .title("TDDY Coder"),
                            );
                        if let Err(e) = conn.initialize(init_req).await {
                            let _ = response_tx.send(Err(BackendError::InvocationFailed(format!(
                                "ACP initialize failed: {e}"
                            ))));
                            continue;
                        }
                        initialized = true;
                    }

                    let cwd = request
                        .working_dir
                        .clone()
                        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

                    let session_dir = request.session_dir.clone();
                    let codex_cli = codex_cli_path.clone();

                    let acp_session_id: acp::SessionId = match &request.session {
                        Some(SessionMode::Resume(thread_id)) => {
                            let load_req = || {
                                acp::LoadSessionRequest::new(
                                    acp::SessionId::new(thread_id.clone()),
                                    cwd.clone(),
                                )
                            };
                            let mut oauth_tried = false;
                            loop {
                                match conn.load_session(load_req()).await {
                                    Ok(_) => break acp::SessionId::new(thread_id.clone()),
                                    Err(e) => {
                                        let msg = e.to_string();
                                        if !oauth_tried
                                            && session_dir.is_some()
                                            && acp_error_suggests_retry_oauth(&msg)
                                        {
                                            if let Some(ref sd) = session_dir {
                                                if let Err(err) = run_oauth_login_via_codex_cli(
                                                    codex_cli.clone(),
                                                    sd.clone(),
                                                )
                                                .await
                                                {
                                                    let _ = response_tx.send(Err(err));
                                                    continue 'cmd_loop;
                                                }
                                            }
                                            oauth_tried = true;
                                            continue;
                                        }
                                        let _ =
                                            response_tx.send(Err(BackendError::InvocationFailed(
                                                format!("ACP load_session failed: {e}"),
                                            )));
                                        continue 'cmd_loop;
                                    }
                                }
                            }
                        }
                        _ => {
                            let new_req = || acp::NewSessionRequest::new(cwd.clone());
                            let mut oauth_tried = false;
                            loop {
                                match conn.new_session(new_req()).await {
                                    Ok(r) => break r.session_id,
                                    Err(e) => {
                                        let msg = e.to_string();
                                        if !oauth_tried
                                            && session_dir.is_some()
                                            && acp_error_suggests_retry_oauth(&msg)
                                        {
                                            if let Some(ref sd) = session_dir {
                                                if let Err(err) = run_oauth_login_via_codex_cli(
                                                    codex_cli.clone(),
                                                    sd.clone(),
                                                )
                                                .await
                                                {
                                                    let _ = response_tx.send(Err(err));
                                                    continue 'cmd_loop;
                                                }
                                            }
                                            oauth_tried = true;
                                            continue;
                                        }
                                        let _ =
                                            response_tx.send(Err(BackendError::InvocationFailed(
                                                format!("ACP new_session failed: {e}"),
                                            )));
                                        continue 'cmd_loop;
                                    }
                                }
                            }
                        }
                    };

                    let acp_session_id_str = acp_session_id.0.to_string();
                    let prompt_req =
                        acp::PromptRequest::new(acp_session_id, vec![merged_prompt.into()]);
                    match conn.prompt(prompt_req).await {
                        Ok(_) => {
                            let acc = accumulator.lock().map(|a| CodexAcpAccumulator {
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
                                "ACP prompt failed: {e}"
                            ))));
                        }
                    }
                }
                CodexAcpCommand::Shutdown => break,
            }
        }
    }));
}

async fn spawn_codex_acp_agent(
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
            let p = agent_path.display();
            BackendError::BinaryNotFound(format!(
                "{p} — install the Codex ACP stdio agent on PATH, place `codex-acp` next to your `codex` binary, or set TDDY_CODEX_ACP_CLI / --codex-acp-cli-path / coder-config `codex_acp_cli_path` (see github.com/zed-industries/codex-acp or your Codex CLI distribution)"
            ))
        } else {
            BackendError::InvocationFailed(e.to_string())
        }
    })
}

/// Default `codex-acp` executable name on `PATH` for [`CodexAcpBackend::new`].
pub const DEFAULT_CODEX_ACP_BINARY: &str = "codex-acp";

/// Backend that invokes **`codex-acp`** over ACP (stdio JSON-RPC).
pub struct CodexAcpBackend {
    agent_path: PathBuf,
    agent_args: Vec<String>,
    cmd_tx: mpsc::Sender<CodexAcpCommand>,
}

impl std::fmt::Debug for CodexAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexAcpBackend")
            .field("agent_path", &self.agent_path)
            .field("agent_args", &self.agent_args)
            .finish()
    }
}

fn resolve_codex_cli_for_oauth() -> PathBuf {
    std::env::var_os("TDDY_CODEX_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(super::CodexBackend::DEFAULT_CLI_BINARY))
}

impl Default for CodexAcpBackend {
    fn default() -> Self {
        let agent = std::env::var_os("TDDY_CODEX_ACP_CLI")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CODEX_ACP_BINARY));
        let agent_path = agent.clone();
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let codex_cli = resolve_codex_cli_for_oauth();
        std::thread::spawn(move || {
            run_codex_acp_worker(agent, Vec::new(), codex_cli, cmd_rx);
        });
        Self {
            agent_path,
            agent_args: Vec::new(),
            cmd_tx,
        }
    }
}

impl CodexAcpBackend {
    /// Spawn **`codex-acp`** from `PATH` (or **`TDDY_CODEX_ACP_CLI`**), using **`codex`** for OAuth capture.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn the given ACP agent binary (e.g. **`tddy-acp-stub`** in integration tests).
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        let agent_path = path.clone();
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let codex_cli = resolve_codex_cli_for_oauth();
        std::thread::spawn(move || {
            run_codex_acp_worker(path, Vec::new(), codex_cli, cmd_rx);
        });
        Self {
            agent_path,
            agent_args: Vec::new(),
            cmd_tx,
        }
    }

    /// Like [`Self::with_agent_path`] but uses a specific **`codex`** binary for `codex login` OAuth relay.
    #[must_use]
    pub fn with_agent_and_codex_paths(agent_path: PathBuf, codex_cli_path: PathBuf) -> Self {
        let display_path = agent_path.clone();
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        std::thread::spawn(move || {
            run_codex_acp_worker(agent_path, Vec::new(), codex_cli_path, cmd_rx);
        });
        Self {
            agent_path: display_path,
            agent_args: Vec::new(),
            cmd_tx,
        }
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for CodexAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.cmd_tx
            .send(CodexAcpCommand::Prompt {
                request,
                response_tx,
            })
            .await
            .map_err(|_| {
                BackendError::InvocationFailed("Codex ACP worker channel closed".to_string())
            })?;
        response_rx.await.map_err(|_| {
            BackendError::InvocationFailed("Codex ACP worker dropped response".to_string())
        })?
    }

    fn name(&self) -> &str {
        "codex-acp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn acp_error_suggests_retry_oauth_detects_auth() {
        assert!(acp_error_suggests_retry_oauth("authentication required"));
        assert!(acp_error_suggests_retry_oauth("HTTP 401"));
        assert!(!acp_error_suggests_retry_oauth("syntax error"));
    }

    #[test]
    fn writes_https_oauth_url_to_session_file() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-codex-acp-oauth-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        let path = tmp.join(crate::backend::CODEX_OAUTH_AUTHORIZE_URL_FILENAME);
        fs::write(&path, "https://auth.example.com/o?x=1\n").expect("write");
        let got = fs::read_to_string(&path).expect("read");
        assert!(got.trim().starts_with("https://"));
    }
}
