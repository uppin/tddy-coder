//! The long-running task body that owns one language-server process for its whole
//! lifetime. Unlike a run-to-completion process body, it streams stdout/stdin
//! incrementally and never returns until cancelled (or the child exits).

use std::path::PathBuf;
use std::process::Stdio;
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

/// How long a wedged server is given to shut down gracefully before its child is
/// force-killed by this body (the registry provides a further SIGTERM→SIGKILL net).
const GRACEFUL_SHUTDOWN: Duration = Duration::from_millis(500);

/// Buffer size for streaming the server's stdout to subscribers.
const STDOUT_CHUNK: usize = 8192;

/// A [`TaskBody`] hosting a single language server. Once the `initialize` handshake
/// succeeds, an [`LspClient`] is handed back to the registry via `client_tx`.
pub struct LspServerBody {
    /// How to launch the server.
    pub spec: LaunchSpec,
    /// Workspace root the server operates on (its cwd / `rootUri`).
    pub root_dir: PathBuf,
    /// One-shot used to publish the initialized client back to the registry.
    pub client_tx: oneshot::Sender<Arc<LspClient>>,
}

#[async_trait]
impl TaskBody for LspServerBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let LspServerBody {
            spec,
            root_dir,
            client_tx,
        } = *self;

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
                return TaskStatus::Failed {
                    message: format!(
                        "failed to spawn language server '{}': {}",
                        spec.program, err
                    ),
                };
            }
        };

        if let Some(pid) = child.id() {
            ctx.register_child_pid(pid);
        }

        let out_channel = match ctx.channel("0") {
            Some(channel) => channel,
            None => {
                let _ = child.start_kill();
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

        // Complete the LSP handshake and publish the client to the registry.
        let root_uri = format!("file://{}", root_dir.display());
        let client = match LspClient::initialize(stdin_tx, out_channel.subscribe(), &root_uri).await
        {
            Ok(client) => Arc::new(client),
            Err(err) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                stdin_task.abort();
                stdout_task.abort();
                return TaskStatus::Failed {
                    message: format!("language server initialize failed: {err}"),
                };
            }
        };
        let _ = client_tx.send(Arc::clone(&client));

        // Run until cancelled or the child exits on its own.
        let cancel = ctx.cancel_token();
        tokio::select! {
            _ = cancel.cancelled() => {}
            result = child.wait() => {
                stdin_task.abort();
                stdout_task.abort();
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
        let _ = child.wait().await;
        stdin_task.abort();
        stdout_task.abort();
        TaskStatus::Cancelled
    }
}
