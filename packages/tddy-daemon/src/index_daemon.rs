//! Lazy get-or-spawn, supervision, restart and idle stop of the one `tddy-index-daemon` process
//! this daemon manages.
//!
//! The shape is [`tddy_lsp::LspRegistry::get_or_spawn`] one level up, and for the same reason its
//! own documentation gives: [`tddy_task::TaskRegistry`] owns *process lifetime* and the
//! SIGTERM→SIGKILL escalation net, but it keys by generated id and evicts terminal tasks, so
//! "the process we already started" needs a layer that keys by identity. Here the identity is
//! trivial — there is exactly **one** index daemon per host, because that process is itself
//! root-parameterised and serves every workspace root
//! (`docs/ft/coder/1-WIP/PRD-2026-09-15-warm-code-intelligence-daemon.md` § *One process, many
//! worktrees*) — so the map collapses to a single slot.
//!
//! What the layer still has to do is everything the trivial key does not remove: start the process
//! only when something needs it, hand the same process to the next caller, notice that it has
//! exited and replace it rather than return a dead handle, stop it once it has gone unused, and
//! stop it when this daemon shuts down.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_task::{IdleTimeoutTracker, TaskHandle, TaskId, TaskRegistry, TaskStatus};
use tokio::sync::{oneshot, Mutex};

use crate::index_daemon_body::IndexDaemonBody;

const LOG_TARGET: &str = "tddy_daemon::index_daemon";

/// The task kind every index-daemon process is registered under, so it is identifiable in a task
/// listing next to `lsp:rust`, `shell` and the rest.
const TASK_KIND: &str = "index-daemon";

/// How often a readiness wait looks again. The same 50 ms cadence as
/// `tddy_daemon_sandbox::sandbox_session::wait_for_sandbox_ready`.
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long stopping the process waits to *see* it stop. The task registry's own escalation net
/// runs on a 5-second grace period, so this is the window in which that net has either worked or
/// been given up on; it bounds daemon shutdown rather than letting a wedged child hold it open.
const STOP_OBSERVATION: Duration = Duration::from_secs(6);

/// What to run, where to reach it, and how patient to be with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDaemonSpawn {
    /// The `tddy-index-daemon` program to execute.
    pub program: PathBuf,
    /// The AF_UNIX socket the child is told to bind, and the one this daemon dials.
    pub socket_path: PathBuf,
    /// How long the child has to bind that socket before the start is refused.
    pub ready_timeout: Duration,
    /// How long the process may sit unused before [`IndexDaemonRegistry::reap_idle`] stops it.
    pub idle_timeout: Duration,
}

impl IndexDaemonSpawn {
    /// The spawn plan an `index_daemon:` configuration section asks for.
    pub fn from_config(config: &tddy_daemon_kernel::config::IndexDaemonConfig) -> Self {
        Self {
            program: config
                .binary_path
                .clone()
                .unwrap_or_else(resolve_index_daemon_path),
            socket_path: config.resolved_socket_path(),
            ready_timeout: Duration::from_secs(config.ready_timeout_secs),
            idle_timeout: Duration::from_secs(config.idle_timeout_secs),
        }
    }
}

/// A started, bound index daemon: the task that owns its process, and where to reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDaemon {
    pub task_id: TaskId,
    pub socket_path: PathBuf,
}

/// Why a caller did not get a usable index daemon.
///
/// Four classes rather than one string, because they mean different things to whoever asked: a
/// program that is not there is a deployment problem, a process that died is something to report
/// with its reason, a readiness deadline is a slow host, and a socket that will not answer is a
/// live process that cannot be talked to. None of them is a reason to carry on without an index
/// daemon — a configured one that cannot be started is an error, not a silent cold run.
#[derive(Debug)]
pub enum IndexDaemonError {
    /// The program could not be executed at all.
    NotStarted { detail: String },
    /// The process exited before it bound its socket. `reason` is the decoded exit plus the last
    /// thing it wrote to stderr.
    DiedBeforeReady { reason: String },
    /// The process was still running but had not bound its socket within the readiness budget.
    NotReadyInTime { seconds: u64 },
    /// The socket is there but would not answer.
    NotDialable { detail: String },
    /// This daemon is shutting down, so nothing new is started.
    Stopped,
}

impl std::fmt::Display for IndexDaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted { detail } => write!(f, "{detail}"),
            Self::DiedBeforeReady { reason } => {
                write!(f, "the index daemon died before it was ready: {reason}")
            }
            Self::NotReadyInTime { seconds } => write!(
                f,
                "the index daemon had not bound its socket after {seconds}s"
            ),
            Self::NotDialable { detail } => {
                write!(f, "the index daemon could not be dialled: {detail}")
            }
            Self::Stopped => write!(f, "this daemon is shutting down its index daemon"),
        }
    }
}

impl std::error::Error for IndexDaemonError {}

/// The live process, with the idle timer that decides when it has outstayed its use.
type Live = (Arc<IndexDaemon>, Arc<IdleTimeoutTracker>);

struct Registry {
    spawn: IndexDaemonSpawn,
    tasks: TaskRegistry,
    /// The one process this daemon manages, once something has needed it.
    live: Mutex<Option<Live>>,
    /// Held across a start, so concurrent first callers queue on it and the second one through
    /// finds the first's process instead of starting a second. The `LspRegistry` spawn gate, minus
    /// the per-key map a single key does not need.
    starting: Mutex<()>,
    /// Set by [`IndexDaemonRegistry::shutdown`] under `live`'s lock, so a start that completes
    /// after shutdown cancels its own process rather than leaving it behind.
    stopped: AtomicBool,
}

/// Lazy get-or-spawn of the index-daemon process, with idle and shutdown teardown.
#[derive(Clone)]
pub struct IndexDaemonRegistry {
    inner: Arc<Registry>,
}

impl IndexDaemonRegistry {
    /// Manage the process `spawn` describes, running it on `tasks`. Starts nothing.
    pub fn new(spawn: IndexDaemonSpawn, tasks: TaskRegistry) -> Self {
        Self {
            inner: Arc::new(Registry {
                spawn,
                tasks,
                live: Mutex::new(None),
                starting: Mutex::new(()),
                stopped: AtomicBool::new(false),
            }),
        }
    }

    /// The socket this daemon tells its index daemon to bind.
    pub fn socket_path(&self) -> &Path {
        &self.inner.spawn.socket_path
    }

    /// Lazily get (or start) the index daemon. A second caller is handed the running process; a
    /// process that has exited is replaced rather than returned dead.
    pub async fn get_or_spawn(&self) -> Result<Arc<IndexDaemon>, IndexDaemonError> {
        if let Some(live) = self.live().await {
            return Ok(live);
        }
        let starting = self.inner.starting.lock().await;
        // Whoever held the gate before us may have started the very process we came for.
        let started = match self.live().await {
            Some(live) => Ok(live),
            None => self.start().await,
        };
        drop(starting);
        started
    }

    /// A gRPC channel to the index daemon, starting it if nothing has yet.
    ///
    /// [`tddy_sandbox_runner::connect_uds_channel`] is the shared AF_UNIX connector every tonic
    /// client in this workspace dials through, so the index daemon is reached the same way the
    /// in-jail `SandboxService` and the daemon's own `ConnectionService` are. Dialled per call
    /// rather than cached: a channel is bound to one connection, and a restarted index daemon is
    /// behind a new one.
    pub async fn connect(&self) -> Result<tonic::transport::Channel, IndexDaemonError> {
        let running = self.get_or_spawn().await?;
        tddy_sandbox_runner::connect_uds_channel(&running.socket_path)
            .await
            .map_err(|err| IndexDaemonError::NotDialable {
                detail: format!("{}: {err:#}", running.socket_path.display()),
            })
    }

    /// The index daemon this registry is currently holding, if it is holding one.
    pub async fn running(&self) -> Option<Arc<IndexDaemon>> {
        self.inner
            .live
            .lock()
            .await
            .as_ref()
            .map(|(running, _)| Arc::clone(running))
    }

    /// Stop the process if it has been unused for at least the idle timeout; returns the task that
    /// was stopped.
    pub async fn reap_idle(&self) -> Option<TaskId> {
        let idle = {
            let mut live = self.inner.live.lock().await;
            let expired = live
                .as_ref()
                .is_some_and(|(_, tracker)| tracker.should_shutdown());
            if expired {
                live.take()
            } else {
                None
            }
        };
        let (running, _) = idle?;
        log::info!(
            target: LOG_TARGET,
            "the index daemon has been idle for {}s; stopping it",
            self.inner.spawn.idle_timeout.as_secs()
        );
        self.stop(&running.task_id).await;
        Some(running.task_id.clone())
    }

    /// Stop the process, and refuse to start another. Called on the way out of the daemon, so the
    /// index daemon does not outlive the process that started it.
    pub async fn shutdown(&self) {
        let live = {
            let mut live = self.inner.live.lock().await;
            // Under the same lock a completing start takes before it records its process, so the
            // two cannot cross: either the start sees this flag and cancels its own process, or it
            // recorded it before us and we are taking it here.
            self.inner.stopped.store(true, Ordering::SeqCst);
            live.take()
        };
        let Some((running, _)) = live else {
            return;
        };
        log::info!(target: LOG_TARGET, "stopping the index daemon on shutdown");
        self.stop(&running.task_id).await;
    }

    /// The running process, with its idle timer refreshed — or `None`, having dropped an entry
    /// whose task has become terminal so a fresh process can replace it.
    async fn live(&self) -> Option<Arc<IndexDaemon>> {
        let mut live = self.inner.live.lock().await;
        let (existing, tracker) = live
            .as_ref()
            .map(|(running, tracker)| (Arc::clone(running), Arc::clone(tracker)))?;
        let alive = match self.inner.tasks.get(&existing.task_id).await {
            Some(handle) => !handle.status().is_terminal(),
            None => false,
        };
        if alive {
            tracker.record_activity();
            return Some(existing);
        }
        log::info!(
            target: LOG_TARGET,
            "the index daemon (task {}) is gone; a fresh one will be started",
            existing.task_id.0
        );
        *live = None;
        None
    }

    /// Start the process, wait for it to bind, and record it as the live one.
    async fn start(&self) -> Result<Arc<IndexDaemon>, IndexDaemonError> {
        if self.inner.stopped.load(Ordering::SeqCst) {
            return Err(IndexDaemonError::Stopped);
        }
        let socket_path = self.inner.spawn.socket_path.clone();
        let cleared = clear_the_socket_of_a_process_that_is_gone(&socket_path)?;

        let (started_tx, started_rx) = oneshot::channel();
        let handle = self
            .inner
            .tasks
            .spawn(
                IndexDaemonBody {
                    program: self.inner.spawn.program.clone(),
                    socket_path: socket_path.clone(),
                    started_tx,
                },
                TASK_KIND,
                "",
                vec![],
            )
            .await;

        if let Err(refusal) = self.became_ready(&handle, started_rx, cleared).await {
            self.stop(&handle.id).await;
            return Err(refusal);
        }

        let running = Arc::new(IndexDaemon {
            task_id: handle.id.clone(),
            socket_path,
        });
        let mut live = self.inner.live.lock().await;
        if self.inner.stopped.load(Ordering::SeqCst) {
            drop(live);
            self.stop(&handle.id).await;
            return Err(IndexDaemonError::Stopped);
        }
        *live = Some((
            Arc::clone(&running),
            Arc::new(IdleTimeoutTracker::new(self.inner.spawn.idle_timeout)),
        ));
        log::info!(
            target: LOG_TARGET,
            "the index daemon (task {}) is serving on {}",
            running.task_id.0,
            running.socket_path.display()
        );
        Ok(running)
    }

    /// Wait until the child has bound its socket.
    ///
    /// The readiness signal is the socket itself appearing, which is what
    /// `docs/dev/1-WIP/2026-09-15-warm-code-intelligence-daemon.md` calls the
    /// bind-then-write-the-marker contract: `UnixListener::bind` creates the path *and* starts
    /// accepting, so the file is evidence of the one thing a caller needs, and it does not depend
    /// on the child's log level the way reading its `listening on …` narration would.
    ///
    /// That only holds if the path was empty when the child started, which is why
    /// [`SocketWasCleared`] is an argument rather than a convention: a socket a killed predecessor
    /// left behind would otherwise read as this child having bound instantly, and a child dying on
    /// its first breath would be reported as a success.
    ///
    /// The child is checked on **every** tick, following
    /// `tddy_daemon_sandbox::sandbox_session::wait_for_sandbox_ready`: a process that dies before
    /// binding is reported at once, with the reason it died, instead of being reported as a
    /// timeout after a budget nobody was waiting on.
    async fn became_ready(
        &self,
        handle: &Arc<TaskHandle>,
        started_rx: oneshot::Receiver<Result<(), String>>,
        _cleared: SocketWasCleared,
    ) -> Result<(), IndexDaemonError> {
        match started_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(detail)) => return Err(IndexDaemonError::NotStarted { detail }),
            Err(_) => {
                return Err(IndexDaemonError::NotStarted {
                    detail: "the index daemon's task ended before it started a process".to_string(),
                })
            }
        }

        let deadline = tokio::time::Instant::now() + self.inner.spawn.ready_timeout;
        loop {
            if self.inner.spawn.socket_path.exists() {
                return Ok(());
            }
            if let Some(reason) = how_it_died(handle) {
                return Err(IndexDaemonError::DiedBeforeReady { reason });
            }
            if self.inner.stopped.load(Ordering::SeqCst) {
                return Err(IndexDaemonError::Stopped);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(IndexDaemonError::NotReadyInTime {
                    seconds: self.inner.spawn.ready_timeout.as_secs(),
                });
            }
            tokio::time::sleep(READINESS_POLL_INTERVAL).await;
        }
    }

    /// Cancel `task_id` and wait, bounded, to see it reach a terminal state — so "stopped" means
    /// the process is gone rather than that a signal was sent after us.
    async fn stop(&self, task_id: &TaskId) {
        self.inner.tasks.cancel_task(task_id).await;
        let watched = self.inner.tasks.clone();
        let id = task_id.clone();
        let observed = tokio::time::timeout(STOP_OBSERVATION, async move {
            loop {
                match watched.get(&id).await {
                    Some(handle) if handle.status().is_terminal() => return,
                    None => return,
                    _ => tokio::time::sleep(READINESS_POLL_INTERVAL).await,
                }
            }
        })
        .await;
        if observed.is_err() {
            log::warn!(
                target: LOG_TARGET,
                "the index daemon (task {}) had not stopped {}s after being cancelled",
                task_id.0,
                STOP_OBSERVATION.as_secs()
            );
        }
    }
}

/// How a task that has reached a terminal state got there, or `None` while it is still running.
fn how_it_died(handle: &Arc<TaskHandle>) -> Option<String> {
    match handle.status() {
        TaskStatus::Failed { message } => Some(message),
        TaskStatus::Completed {
            exit_code: Some(code),
        } => Some(format!("exited with status {code}")),
        TaskStatus::Completed { exit_code: None } => Some("exited".to_string()),
        TaskStatus::Cancelled => Some("was cancelled".to_string()),
        TaskStatus::Pending | TaskStatus::Running => None,
    }
}

/// Proof that the socket path held nothing at the moment a child was started, which is what lets
/// [`IndexDaemonRegistry::became_ready`] read "the path exists" as "this child bound it".
///
/// A marker type rather than a comment because the two steps are one fact split across twenty
/// lines, and the failure mode of separating them is silent: readiness would be satisfied by a
/// predecessor's socket, and a child that died immediately would be reported as serving.
#[must_use]
struct SocketWasCleared;

/// Remove a socket left behind by an index daemon that is no longer running.
///
/// `tddy-index-daemon` refuses to bind a path that already exists, because from *inside* that
/// process the file is indistinguishable from the socket of a peer still serving clients. This
/// daemon is not inside it: it only ever starts a process when the one it was managing has reached
/// a terminal state, and a process killed outright leaves its socket behind. Clearing it here is
/// what makes restart-after-a-kill work at all.
fn clear_the_socket_of_a_process_that_is_gone(
    socket_path: &Path,
) -> Result<SocketWasCleared, IndexDaemonError> {
    if !socket_path.exists() {
        return Ok(SocketWasCleared);
    }
    std::fs::remove_file(socket_path)
        .map(|()| SocketWasCleared)
        .map_err(|err| IndexDaemonError::NotStarted {
            detail: format!(
                "the socket {} could not be cleared for a new index daemon: {err}",
                socket_path.display()
            ),
        })
}

/// Resolve the `tddy-index-daemon` binary.
///
/// The three tiers of `tddy_daemon_sandbox::resolve_sandbox_runner_path`:
/// `CARGO_BIN_EXE_tddy-index-daemon` (a cargo test in the crate that owns the binary) → a sibling
/// of `current_exe()`, with the `deps/` hop an integration-test binary needs → the bare name, left
/// to `PATH`, which is where `./install` puts it.
pub fn resolve_index_daemon_path() -> PathBuf {
    const PROGRAM: &str = "tddy-index-daemon";

    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_tddy-index-daemon") {
        if !bin.trim().is_empty() {
            return PathBuf::from(bin);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut bin_dir) = exe.parent().map(|dir| dir.to_path_buf()) {
            if bin_dir.file_name().and_then(|name| name.to_str()) == Some("deps") {
                bin_dir.pop();
            }
            let candidate = bin_dir.join(PROGRAM);
            if candidate.is_file() {
                return candidate;
            }
        }
        if let Some(sibling) = exe.parent().map(|dir| dir.join(PROGRAM)) {
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from(PROGRAM)
}
