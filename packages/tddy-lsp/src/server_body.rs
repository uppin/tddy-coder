//! The long-running task body that owns one language-server process for its whole
//! lifetime. Unlike a run-to-completion process body, it streams stdout/stdin
//! incrementally and never returns until cancelled (or the child exits).

use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tddy_task::{TaskBody, TaskContext, TaskStatus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use crate::allowlist::LaunchSpec;
use crate::client::LspClient;
use crate::spawn_observer::{ProcessOutcome, ProcessStart, ProcessToken, SpawnObserver};

/// The purpose a language server started by this crate is recorded under.
///
/// The value the JSONL sink reads to label a record `lsp` rather than `engine`.
const LANGUAGE_SERVER: &str = "language-server";

/// How long a wedged server is given to shut down gracefully before its child is
/// force-killed by this body (the registry provides a further SIGTERM→SIGKILL net).
const GRACEFUL_SHUTDOWN: Duration = Duration::from_millis(500);

/// Buffer size for streaming the server's stdout to subscribers.
const STDOUT_CHUNK: usize = 8192;

/// Buffer size for draining the server's stderr into the log.
const STDERR_CHUNK: usize = 8192;

/// Log target for what the server writes to stderr, so it is filterable apart from this crate's
/// own messages.
const STDERR_LOG_TARGET: &str = "tddy_lsp::server_body";

/// A [`TaskBody`] hosting a single language server. Once the `initialize` handshake
/// succeeds, an [`LspClient`] is handed back to the registry via `client_tx`.
pub struct LspServerBody {
    /// How to launch the server.
    pub spec: LaunchSpec,
    /// Workspace root the server operates on (its cwd / `rootUri`).
    pub root_dir: PathBuf,
    /// One-shot used to publish the initialized client back to the registry — or why the server
    /// never came up, so the registry can refuse in those words rather than as a bare exit.
    pub client_tx: oneshot::Sender<Result<Arc<LspClient>, crate::error::LspError>>,
}

/// The body a registry with a [`SpawnObserver`] spawns: the same server, reporting through the
/// observer the registry holds.
///
/// A wrapper rather than a field on [`LspServerBody`], because that struct's fields are public and
/// built literally by callers that start a server with no host watching. The observer arrives at
/// [`LspServerBody::run_with`] as an argument instead.
pub(crate) struct ObservedServerBody {
    body: LspServerBody,
    observer: Option<Arc<dyn SpawnObserver>>,
}

impl ObservedServerBody {
    pub(crate) fn new(body: LspServerBody, observer: Option<Arc<dyn SpawnObserver>>) -> Self {
        Self { body, observer }
    }
}

#[async_trait]
impl TaskBody for ObservedServerBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let ObservedServerBody { body, observer } = *self;
        body.run_with(ctx, observer).await
    }
}

#[async_trait]
impl TaskBody for LspServerBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        // No observer: this body was spawned directly, as a caller that starts a server with no
        // host watching does.
        (*self).run_with(ctx, None).await
    }
}

impl LspServerBody {
    /// Run the server, reporting its start and its end to `observer` when there is one.
    async fn run_with(
        self,
        ctx: TaskContext,
        observer: Option<Arc<dyn SpawnObserver>>,
    ) -> TaskStatus {
        let LspServerBody {
            spec,
            root_dir,
            client_tx,
        } = self;

        // Spawn the child language server.
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        // Only chdir into the root if it actually exists; a real workspace root does,
        // but callers may key servers by a logical root that is not a live directory.
        if root_dir.is_dir() {
            command.current_dir(&root_dir);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                // TODO(reshape-anchors-outline): implement — send the typed refusal over `client_tx`
                // (`ServerNotFound` / `ServerNotStarted`, changeset R1) before returning.
                return TaskStatus::Failed {
                    message: format!(
                        "failed to spawn language server '{}': {}",
                        spec.program, err
                    ),
                };
            }
        };

        // The server exists: report it before anything waits on it, so a start with no end is the
        // record of a server that never came back.
        let record = observer.as_ref().map(|observer| {
            let start = ProcessStart {
                purpose: LANGUAGE_SERVER,
                program: spec.program.clone(),
                args: spec.args.clone(),
                cwd: root_dir.is_dir().then(|| root_dir.clone()),
                env_names: spec.env.iter().map(|(name, _)| name.clone()).collect(),
                pid: child.id(),
                operation: None,
            };
            (Arc::clone(observer), observer.started(&start))
        });

        if let Some(pid) = child.id() {
            ctx.register_child_pid(pid);
        }

        let out_channel = match ctx.channel("0") {
            Some(channel) => channel,
            None => {
                let _ = child.start_kill();
                // TODO(reshape-anchors-outline): implement — send the typed refusal over `client_tx`
                // (`ServerNotFound` / `ServerNotStarted`, changeset R1) before returning.
                report_end(&record, child.wait().await);
                return TaskStatus::Failed {
                    message: "language server task is missing its output channel".to_string(),
                };
            }
        };

        // Bridge an internal stdin queue to the child's stdin.
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<Bytes>();
        let child_stdin = child.stdin.take();
        let stdin_task = tokio::spawn(async move {
            let Some(mut stdin) = child_stdin else {
                return;
            };
            while let Some(chunk) = stdin_rx.recv().await {
                if stdin.write_all(&chunk).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Stream the child's stdout to channel subscribers incrementally.
        let child_stdout = child.stdout.take();
        let out_for_reader = Arc::clone(&out_channel);
        let stdout_task = tokio::spawn(async move {
            let Some(mut stdout) = child_stdout else {
                return;
            };
            let mut buf = [0u8; STDOUT_CHUNK];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => out_for_reader.write(Bytes::copy_from_slice(&buf[..n])),
                    Err(_) => break,
                }
            }
        });

        // Drain the child's stderr into the log. Piping it without reading it fills the pipe
        // buffer (~64 KiB), at which point the server blocks mid-write and stops answering
        // requests — a diagnostic channel becoming a deadlock. It goes to the log rather than to
        // this process's stderr because a consumer of this crate may be speaking a protocol there.
        let child_stderr = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let Some(mut stderr) = child_stderr else {
                return;
            };
            let mut buf = [0u8; STDERR_CHUNK];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]);
                        log::debug!(target: STDERR_LOG_TARGET, "{}", text.trim_end());
                    }
                    Err(_) => break,
                }
            }
        });

        // Complete the LSP handshake and publish the client to the registry.
        let root_uri = format!("file://{}", root_dir.display());
        let client = match LspClient::initialize(
            stdin_tx,
            out_channel.subscribe(),
            &root_uri,
            spec.capabilities.clone(),
            spec.initialization_options.clone(),
        )
        .await
        {
            Ok(client) => Arc::new(client),
            Err(err) => {
                // TODO(reshape-anchors-outline): implement — send the typed refusal over `client_tx`
                // (`ServerNotFound` / `ServerNotStarted`, changeset R1) before returning.
                let _ = child.start_kill();
                report_end(&record, child.wait().await);
                stdin_task.abort();
                stdout_task.abort();
                stderr_task.abort();
                return TaskStatus::Failed {
                    message: format!("language server initialize failed: {err}"),
                };
            }
        };
        let _ = client_tx.send(Ok(Arc::clone(&client)));

        // Run until cancelled or the child exits on its own.
        let cancel = ctx.cancel_token();
        tokio::select! {
            _ = cancel.cancelled() => {}
            result = child.wait() => {
                stdin_task.abort();
                stdout_task.abort();
                stderr_task.abort();
                if let Ok(status) = &result {
                    report_end(&record, Ok(*status));
                }
                if ctx.is_cancelled() {
                    return TaskStatus::Cancelled;
                }
                return match result {
                    Ok(exit) => TaskStatus::Completed { exit_code: exit.code() },
                    Err(err) => TaskStatus::Failed {
                        message: format!("language server wait failed: {err}"),
                    },
                };
            }
        }

        // Cancellation requested: attempt graceful shutdown (bounded so a wedged
        // server can't stall us), then ensure the child is gone.
        let _ = tokio::time::timeout(GRACEFUL_SHUTDOWN, client.shutdown()).await;
        let _ = child.start_kill();
        report_end(&record, child.wait().await);
        stdin_task.abort();
        stdout_task.abort();
        stderr_task.abort();
        TaskStatus::Cancelled
    }
}

/// Tell the observer how the child ended, when one is watching and the wait produced a status.
fn report_end(
    record: &Option<(Arc<dyn SpawnObserver>, ProcessToken)>,
    status: std::io::Result<ExitStatus>,
) {
    if let (Some((observer, token)), Ok(status)) = (record, status) {
        observer.ended(*token, &outcome_of(status));
    }
}

/// How a process ended, from the status it was waited on with.
fn outcome_of(status: ExitStatus) -> ProcessOutcome {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ProcessOutcome::Signalled { signal };
        }
    }
    ProcessOutcome::Exited {
        code: status.code().unwrap_or(-1),
    }
}
