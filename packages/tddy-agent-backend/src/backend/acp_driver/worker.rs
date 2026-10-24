//! The driver's worker thread. It owns the agent child and the `ClientSideConnection`; both are
//! `?Send`, so they stay on one thread with a current-thread runtime and a `LocalSet`. Requests
//! arrive as [`Command`]s over a channel, and each runs on its own local task, so the loop keeps
//! receiving while a prompt is in flight: a cancel reaches a turn that is still running.

use std::future::Future;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Waker};

use agent_client_protocol as acp;
use tokio::sync::{mpsc, oneshot};
use tokio::task::{spawn_local, LocalSet};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::client::{DriverClient, Turns};
use super::session::AgentConnection;
use super::{
    AcpAgentCommand, AcpAgentHooks, AcpSession, AcpTurnEnd, AcpUpdateSink, OpenSession,
    PermissionHandler,
};
use crate::backend::{BackendModels, ProgressSink};
use crate::error::BackendError;

/// Where a request's answer goes back.
pub(super) type Reply<T> = oneshot::Sender<Result<T, BackendError>>;

/// A request to the worker, carrying the channel its answer is sent on.
pub(super) enum Command {
    OpenSession {
        open: OpenSession,
        reply: Reply<AcpSession>,
    },
    Prompt {
        session_id: String,
        prompt: String,
        updates: Option<AcpUpdateSink>,
        progress: Option<ProgressSink>,
        reply: Reply<AcpTurnEnd>,
    },
    Cancel {
        session_id: String,
        reply: Reply<()>,
    },
    ListModels {
        reply: Reply<BackendModels>,
    },
}

/// What the worker starts from: the child to spawn, the handlers its requests use, the channel its
/// requests arrive on, and the places it reports back to its creator (whether the child started, and
/// the child's pid).
pub(super) struct Setup {
    pub(super) command: AcpAgentCommand,
    pub(super) permissions: Arc<dyn PermissionHandler>,
    pub(super) hooks: Arc<dyn AcpAgentHooks>,
    pub(super) commands: mpsc::Receiver<Command>,
    pub(super) started: std::sync::mpsc::SyncSender<Result<(), BackendError>>,
    pub(super) pid: Arc<AtomicU32>,
}

/// The worker thread's body: serve requests until the driver is dropped.
pub(super) fn run(setup: Setup) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = setup
                .started
                .send(Err(BackendError::InvocationFailed(format!(
                    "could not start the ACP driver runtime: {e}"
                ))));
            return;
        }
    };
    runtime.block_on(LocalSet::new().run_until(serve(setup)));
}

async fn serve(setup: Setup) {
    let Setup {
        command,
        permissions,
        hooks,
        mut commands,
        started,
        pid,
    } = setup;
    let mut child = match spawn_agent(&command) {
        Ok(child) => child,
        Err(error) => {
            let _ = started.send(Err(error));
            return;
        }
    };
    let (Some(child_pid), Some(stdin), Some(stdout)) =
        (child.id(), child.stdin.take(), child.stdout.take())
    else {
        let _ = started.send(Err(BackendError::InvocationFailed(
            "the ACP agent started without its pid or stdio pipes".to_string(),
        )));
        return;
    };
    pid.store(child_pid, Ordering::SeqCst);
    let (end_child, child_ended) = oneshot::channel();
    let supervisor = spawn_local(supervise(child, child_ended, Arc::clone(&pid)));

    let turns = Turns::default();
    let client = DriverClient::new(turns.clone(), permissions);
    let (conn, io) =
        acp::ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), run_or_spawn);
    spawn_local(async move {
        let _ = io.await;
    });
    let agent = Rc::new(AgentConnection::new(conn, turns, hooks));
    let _ = started.send(Ok(()));

    while let Some(command) = commands.recv().await {
        dispatch(Rc::clone(&agent), command);
    }
    // Every driver handle is gone: end the child and wait until it is reaped.
    let _ = end_child.send(());
    let _ = supervisor.await;
}

/// Start a future the connection spawns for one incoming message.
///
/// The library delivers an agent's `session/update` on a task of its own, while a prompt's response
/// wakes the prompt directly, so the turn could end before its last update was recorded. Polling the
/// future once here, at once, closes that gap: a notification handler completes on that poll, so the
/// update is recorded in the order the agent sent it, before the response that follows it. A future
/// still pending after the poll is handed to the local set, which polls it again with a real waker.
/// A future must tolerate a poll that finds nothing new, so the placeholder waker loses nothing.
fn run_or_spawn(fut: impl Future<Output = ()> + 'static) {
    let mut fut = Box::pin(fut);
    let mut cx = Context::from_waker(Waker::noop());
    if fut.as_mut().poll(&mut cx).is_pending() {
        spawn_local(fut);
    }
}

/// Run one request on its own local task, so the loop returns to receiving at once.
fn dispatch(agent: Rc<AgentConnection>, command: Command) {
    spawn_local(async move {
        match command {
            Command::OpenSession { open, reply } => {
                let _ = reply.send(agent.open_session(open).await);
            }
            Command::Prompt {
                session_id,
                prompt,
                updates,
                progress,
                reply,
            } => {
                let _ = reply.send(agent.prompt(session_id, prompt, updates, progress).await);
            }
            Command::Cancel { session_id, reply } => {
                let _ = reply.send(agent.cancel(session_id).await);
            }
            Command::ListModels { reply } => {
                let _ = reply.send(agent.list_models().await);
            }
        }
    });
}

/// Hold the child until the driver ends it, or until it exits on its own. Either way the child is
/// reaped before this returns, and the pid is cleared.
async fn supervise(
    mut child: tokio::process::Child,
    ended: oneshot::Receiver<()>,
    pid: Arc<AtomicU32>,
) {
    let child_pid = child.id();
    tokio::select! {
        _ = child.wait() => {}
        _ = ended => {
            if let Err(e) = child.start_kill() {
                log::warn!("could not kill the ACP agent (pid {child_pid:?}): {e}");
            }
            let _ = child.wait().await;
        }
    }
    pid.store(0, Ordering::SeqCst);
}

/// Start the agent with its stdio piped. Its stderr is discarded, as the backends always had it.
fn spawn_agent(command: &AcpAgentCommand) -> Result<tokio::process::Child, BackendError> {
    let mut process = tokio::process::Command::new(&command.program);
    process
        .args(&command.args)
        .envs(command.env.iter().map(|(name, value)| (name, value)));
    if let Some(cwd) = &command.cwd {
        process.current_dir(cwd);
    }
    process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    process.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BackendError::BinaryNotFound(command.program.to_string_lossy().into_owned())
        } else {
            BackendError::InvocationFailed(e.to_string())
        }
    })
}
