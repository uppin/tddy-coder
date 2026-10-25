//! PTY spawn runtime — spawns a process inside a `portable_pty` master as a [`tddy_task::Task`]
//! and pumps its I/O through the task's PTY channel.
//!
//! Host-specific concerns (OS-user impersonation, privilege drops, per-user `PATH`/`HOME`) are the
//! caller's responsibility: this runtime receives the final `argv`/`env` via [`PtySpawnSpec`] and
//! spawns them verbatim.

use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tddy_task::{TaskBody, TaskChannel, TaskContext, TaskHandle, TaskRegistry, TaskStatus};
use tokio::sync::{mpsc, oneshot};

use crate::registry::{PtyControl, PtyRegistry};

/// Default terminal size for spawned PTY sessions.
pub const DEFAULT_TERM_ROWS: u16 = 24;
pub const DEFAULT_TERM_COLS: u16 = 220;

/// PTY reader buffer size.
const PTY_READ_BUF: usize = 4096;

/// How long a child's exit waits for the reader to drain what is left of its output. Draining
/// takes milliseconds; the bound matters only when a process the child left behind still holds the
/// terminal open.
const READER_DRAIN_GRACE: Duration = Duration::from_secs(1);

/// The variables `gh` honours ahead of its own login.
const GITHUB_TOKEN_VARIABLES: [&str; 4] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

/// Specification for spawning a process inside a PTY.
///
/// `argv` and `env` are final: the caller has already applied any host-specific transforms (e.g.
/// a `setpriv` privilege-drop front, per-user `HOME`/`PATH`) before constructing this spec.
#[derive(Debug, Clone)]
pub struct PtySpawnSpec {
    pub argv: Vec<String>,
    pub worktree_path: PathBuf,
    pub session_id: String,
    pub terminal_id: String,
    pub kind: String,
    /// Extra environment variables set on the spawned process (in addition to the inherited env).
    pub env: Vec<(String, String)>,
}

/// Ready signal emitted once the PTY is open and the child has been spawned.
pub struct PtyReady {
    pub pid: u32,
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub current_size: Arc<Mutex<PtySize>>,
}

/// Spawn a PTY-backed process as a registered task.
pub struct PtyRuntime;

impl PtyRuntime {
    pub async fn spawn(
        registry: &TaskRegistry,
        pty_registry: &PtyRegistry,
        spec: PtySpawnSpec,
        ready_tx: oneshot::Sender<Result<PtyReady, String>>,
    ) -> Arc<TaskHandle> {
        let (channel, stdin_rx) = TaskChannel::pty("0", "pty");
        let body = PtyTaskBody {
            spec: spec.clone(),
            pty_registry: pty_registry.clone(),
            stdin_rx: stdin_rx.expect("pty channel must have stdin"),
            ready_tx: Some(ready_tx),
        };
        registry
            .spawn(body, spec.kind, spec.session_id, vec![channel])
            .await
    }
}

struct PtyTaskBody {
    spec: PtySpawnSpec,
    pty_registry: PtyRegistry,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    ready_tx: Option<oneshot::Sender<Result<PtyReady, String>>>,
}

struct SetupResult {
    pid: u32,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    current_size: Arc<Mutex<PtySize>>,
    exit_rx: oneshot::Receiver<TaskStatus>,
}

#[async_trait]
impl TaskBody for PtyTaskBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let task_id = ctx.task_id();
        let task_id_log = task_id.clone();
        let cancel = ctx.cancel_token();
        let output_ch = match ctx.channel("0") {
            Some(ch) => ch,
            None => {
                return TaskStatus::Failed {
                    message: "missing PTY channel".into(),
                };
            }
        };

        let (setup_tx, setup_rx) = oneshot::channel::<Result<SetupResult, String>>();
        let spec = self.spec.clone();
        let stdin_rx = self.stdin_rx;
        let ready_tx = self.ready_tx;
        let output_ch_thread = Arc::clone(&output_ch);
        // Captured from the async context so the blocking writer thread — which has no ambient
        // runtime of its own — can drive the async stdin receiver and the child-exit signal.
        let rt_handle = tokio::runtime::Handle::current();

        std::thread::spawn(move || {
            if let Err(e) = open_pty_and_pump(spec, stdin_rx, output_ch_thread, setup_tx, rt_handle)
            {
                log::warn!(
                    target: "tddy_pty::runtime",
                    "PTY setup failed for task {}: {}",
                    task_id_log,
                    e
                );
            }
        });

        let setup = match setup_rx.await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                if let Some(tx) = ready_tx {
                    let _ = tx.send(Err(e.clone()));
                }
                return TaskStatus::Failed { message: e };
            }
            Err(_) => {
                return TaskStatus::Failed {
                    message: "PTY setup thread did not respond".into(),
                };
            }
        };

        ctx.register_child_pid(setup.pid);

        // Registered before the ready signal: whoever is told the process runs finds its control,
        // and its absence means the process has exited.
        self.pty_registry
            .insert(
                task_id.clone(),
                PtyControl {
                    master: Arc::clone(&setup.master),
                    current_size: Arc::clone(&setup.current_size),
                    capture: output_ch.capture_arc(),
                    terminal_id: self.spec.terminal_id.clone(),
                    kind: self.spec.kind.clone(),
                },
            )
            .await;

        if let Some(tx) = ready_tx {
            let _ = tx.send(Ok(PtyReady {
                pid: setup.pid,
                master: Arc::clone(&setup.master),
                current_size: Arc::clone(&setup.current_size),
            }));
        }

        let exit_status = tokio::select! {
            _ = cancel.cancelled() => {
                #[cfg(unix)]
                {
                    let _ = signal_pid(setup.pid as i32, libc::SIGTERM);
                    let _ = signal_pid(setup.pid as i32, libc::SIGKILL);
                }
                ctx.deregister_child_pid(setup.pid);
                self.pty_registry.remove(&task_id).await;
                return TaskStatus::Cancelled;
            }
            status = setup.exit_rx => status.unwrap_or(TaskStatus::Failed {
                message: "PTY exit monitor dropped".into(),
            }),
        };

        ctx.deregister_child_pid(setup.pid);
        self.pty_registry.remove(&task_id).await;
        exit_status
    }
}

/// Send `sig` to `pid`, treating a vanished process (ESRCH) as success.
#[cfg(unix)]
fn signal_pid(pid: i32, sig: libc::c_int) -> Result<(), std::io::Error> {
    let ret = unsafe { libc::kill(pid, sig) };
    if ret == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(err)
}

/// Open a PTY, spawn `spec.argv` inside it, and pump stdin/stdout between the PTY master and the
/// task's channel. Reports readiness (or the failure reason) via `setup_tx`.
fn open_pty_and_pump(
    spec: PtySpawnSpec,
    mut stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    output_ch: Arc<TaskChannel>,
    setup_tx: oneshot::Sender<Result<SetupResult, String>>,
    rt_handle: tokio::runtime::Handle,
) -> Result<(), String> {
    if spec.argv.is_empty() {
        let _ = setup_tx.send(Err("empty argv".into()));
        return Err("empty argv".into());
    }

    let pty_system = native_pty_system();
    let initial_size = PtySize {
        rows: DEFAULT_TERM_ROWS,
        cols: DEFAULT_TERM_COLS,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = match pty_system.openpty(initial_size) {
        Ok(pair) => pair,
        Err(e) => {
            let msg = format!("openpty failed: {e}");
            let _ = setup_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };

    // Size the screen model with the PTY, before the child can write: a snapshot of a capture that
    // does not know its grid is refused.
    if let Ok(mut capture) = output_ch.capture_arc().lock() {
        capture.resize(initial_size.rows, initial_size.cols);
    }

    let master = Arc::new(Mutex::new(pair.master));
    let current_size = Arc::new(Mutex::new(initial_size));
    let (exit_tx, exit_rx) = oneshot::channel();
    // Fires when the child exits so the writer thread can stop and release its master fd, rather
    // than parking on `stdin_rx` forever (the child never reads/writes stdin once it is gone).
    let (child_exited_tx, child_exited_rx) = oneshot::channel::<()>();

    // Reader: start BEFORE child spawn so fast-exiting stubs cannot miss PTY output.
    let master_for_reader = Arc::clone(&master);
    let output_ch_reader = Arc::clone(&output_ch);
    // Disconnects when the reader ends, which the exit monitor waits for (see below).
    let (reader_done_tx, reader_done_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let _reader_done = reader_done_tx;
        let reader = {
            let m = master_for_reader.lock().unwrap();
            m.try_clone_reader()
        };
        match reader {
            Err(e) => {
                log::warn!(
                    target: "tddy_pty::runtime",
                    "PTY reader: try_clone_reader failed: {e}"
                );
            }
            Ok(mut r) => {
                let mut buf = vec![0u8; PTY_READ_BUF];
                loop {
                    match std::io::Read::read(&mut r, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            output_ch_reader.write(Bytes::copy_from_slice(&buf[..n]));
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    let mut cmd = CommandBuilder::new(&spec.argv[0]);
    for arg in &spec.argv[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&spec.worktree_path);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // The environment is never a credential: a daemon that has one of these exported must not hand
    // it to the agent by inheritance, or `gh` would act as whoever exported it.
    for token_variable in GITHUB_TOKEN_VARIABLES {
        cmd.env_remove(token_variable);
    }

    for (key, value) in &spec.env {
        cmd.env(key, value);
    }

    let child = match pair.slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(e) => {
            let msg = format!("spawn failed: {e}");
            let _ = setup_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };
    drop(pair.slave);

    let pid = match child.process_id() {
        Some(pid) => pid,
        None => {
            let msg = "spawned child has no pid".to_string();
            let _ = setup_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };

    let _ = setup_tx.send(Ok(SetupResult {
        pid,
        master: Arc::clone(&master),
        current_size: Arc::clone(&current_size),
        exit_rx,
    }));

    // Writer: stdin mpsc → PTY master. Stops on stdin close, a write error, or child exit —
    // whichever comes first — so the dup'd writer fd and the master Arc are released promptly.
    let master_for_writer = Arc::clone(&master);
    std::thread::spawn(move || {
        let writer = {
            let m = master_for_writer.lock().unwrap();
            m.take_writer()
        };
        match writer {
            Err(e) => {
                log::warn!(
                    target: "tddy_pty::runtime",
                    "PTY writer: take_writer failed: {e}"
                );
            }
            Ok(mut w) => {
                let mut child_exited_rx = child_exited_rx;
                rt_handle.block_on(async {
                    loop {
                        tokio::select! {
                            data = stdin_rx.recv() => match data {
                                None => break,
                                Some(bytes) => {
                                    if std::io::Write::write_all(&mut w, &bytes).is_err() {
                                        break;
                                    }
                                }
                            },
                            _ = &mut child_exited_rx => break,
                        }
                    }
                });
            }
        }
    });

    // Exit monitor: wait for child, report terminal status.
    let mut child_monitor = child;
    std::thread::spawn(move || {
        let status = match child_monitor.wait() {
            Ok(_) => TaskStatus::Completed { exit_code: Some(0) },
            Err(e) => TaskStatus::Failed {
                message: format!("child wait error: {e}"),
            },
        };
        // Unblock the writer thread first so it drops its master fd.
        let _ = child_exited_tx.send(());
        // The reader ends once every holder of the slave side has closed it. Waiting for it means
        // the capture holds everything the child wrote before it exited, by the time the terminal
        // status is reported. A process the child left behind can hold the slave open for as long
        // as it runs, so the wait is bounded: past it, the exit is reported without that output.
        if let Err(RecvTimeoutError::Timeout) = reader_done_rx.recv_timeout(READER_DRAIN_GRACE) {
            log::info!(
                target: "tddy_pty::runtime",
                "PTY child {pid} exited while another process still holds its terminal open"
            );
        }
        let _ = exit_tx.send(status);
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spawned shell echoes stdin back on the output channel; the pump plumbs both directions.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawns_a_shell_and_pumps_io() {
        // Given a bash spawn spec in a tempdir
        let registry = TaskRegistry::new();
        let pty_registry = PtyRegistry::new();
        let dir = std::env::temp_dir();
        let spec = PtySpawnSpec {
            argv: vec!["/bin/bash".to_string()],
            worktree_path: dir,
            session_id: "session-1".to_string(),
            terminal_id: "term-1".to_string(),
            kind: "bash".to_string(),
            env: Vec::new(),
        };
        let (ready_tx, ready_rx) = oneshot::channel();

        // When it is spawned and a command is written to stdin
        let task = PtyRuntime::spawn(&registry, &pty_registry, spec, ready_tx).await;
        let ready = ready_rx.await.expect("ready").expect("spawned");
        assert!(ready.pid > 0, "a spawned shell must have a pid");

        let channel = task.channel("0").expect("channel 0");
        let mut rx = channel.subscribe();
        channel
            .stdin_sender()
            .expect("stdin")
            .send(Bytes::from_static(b"echo tddy-pump-marker\n"))
            .expect("send stdin");

        // Then the echoed marker is delivered on the output channel
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut seen = String::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!remaining.is_zero(), "timed out; saw: {seen:?}");
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(bytes)) => {
                    seen.push_str(&String::from_utf8_lossy(&bytes));
                    if seen.contains("tddy-pump-marker") {
                        break;
                    }
                }
                Ok(Err(_)) => panic!("output channel closed; saw: {seen:?}"),
                Err(_) => panic!("timed out; saw: {seen:?}"),
            }
        }

        registry.cancel_task(&task.id).await;
    }

    /// Weak view of a PTY master handle — lets a test observe whether every strong owner
    /// (reader thread, writer thread, registry, ready signal) has released it.
    type MasterWeak = std::sync::Weak<Mutex<Box<dyn MasterPty + Send>>>;

    /// A spec that runs `argv` in a scratch tempdir. Meaningful ids so failures name the session.
    fn a_pty_spec_running(argv: &[&str]) -> PtySpawnSpec {
        PtySpawnSpec {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            worktree_path: std::env::temp_dir(),
            session_id: "fd-leak-session".to_string(),
            terminal_id: "fd-leak-term".to_string(),
            kind: "sh".to_string(),
            env: Vec::new(),
        }
    }

    /// Bounded wait for the task to reach a terminal status. Returns false if it never does.
    async fn reaches_terminal_within(task: &Arc<TaskHandle>, budget: std::time::Duration) -> bool {
        let mut status = task.status_watch();
        tokio::time::timeout(budget, async {
            while !status.borrow().is_terminal() {
                if status.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .is_ok()
    }

    /// Bounded wait for every strong owner of the PTY master to drop it. Returns false if some
    /// owner still pins it when the budget expires.
    async fn master_released_within(master: &MasterWeak, budget: std::time::Duration) -> bool {
        tokio::time::timeout(budget, async {
            while master.upgrade().is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .is_ok()
    }

    /// Regression test for the FD leak: when the child exits, the PTY master (and its dup'd
    /// writer/reader fds) must be released immediately — not pinned until the task registry
    /// ages the task out via its TTL. The session's stdin channel is deliberately kept open
    /// (via the retained `task`) to prove the writer thread stops on child exit rather than
    /// only when the last stdin sender drops.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn releases_the_pty_master_when_the_child_exits_even_while_stdin_stays_open() {
        // Given — a PTY whose child exits immediately, with the session handle (and thus its
        // stdin sender) kept alive for the whole test.
        let registry = TaskRegistry::new();
        let pty_registry = PtyRegistry::new();
        let (ready_tx, ready_rx) = oneshot::channel();
        let task = PtyRuntime::spawn(
            &registry,
            &pty_registry,
            a_pty_spec_running(&["/bin/sh", "-c", "exit 0"]),
            ready_tx,
        )
        .await;
        let ready = ready_rx.await.expect("ready signal").expect("PTY spawned");
        let master: MasterWeak = Arc::downgrade(&ready.master);
        drop(ready); // release the test's own strong ref — only PTY-internal owners remain.

        // When — the child process exits on its own.
        assert!(
            reaches_terminal_within(&task, std::time::Duration::from_secs(5)).await,
            "PTY task never reached terminal status"
        );

        // Then — the master is released promptly, even though the stdin channel is still open.
        assert!(
            master_released_within(&master, std::time::Duration::from_secs(3)).await,
            "PTY master was not released after the child exited — the writer thread is still \
             blocked on stdin and pinning the master fd"
        );
    }

    /// A spawned `/bin/sh -c script`, once it has signalled ready.
    async fn a_running_shell(script: &str) -> Arc<TaskHandle> {
        let registry = TaskRegistry::new();
        let pty_registry = PtyRegistry::new();
        let (ready_tx, ready_rx) = oneshot::channel();
        let task = PtyRuntime::spawn(
            &registry,
            &pty_registry,
            a_pty_spec_running(&["/bin/sh", "-c", script]),
            ready_tx,
        )
        .await;
        let _ready = ready_rx.await.expect("ready signal").expect("PTY spawned");
        task
    }

    /// The exit is reported only once the reader has drained the terminal, so the capture holds
    /// what was written to it up to then.
    ///
    /// Linux only: there a process the child leaves behind, ignoring the hangup, can still write to
    /// the terminal. macOS revokes the terminal as its session leader (the child) exits.
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn output_written_just_after_the_child_exits_is_captured_when_the_exit_is_reported() {
        // Given a child that exits at once, leaving a process on its terminal that writes a line
        // a moment later and then exits too.
        //
        // The hangup is ignored in the child *before* it forks, so the background process is
        // born ignoring it. Ignoring it inside the subshell instead races the child's exit: the
        // child is the session leader, its exit sends SIGHUP to the terminal's foreground group
        // (which, without job control, includes the subshell), and on a loaded machine that can
        // land before the subshell has run its own `trap` — killing it, closing the terminal, and
        // reporting the exit with nothing captured.
        let task = a_running_shell("trap '' HUP; (sleep 0.3; printf 'late-line') & exit 0").await;

        // When the exit is reported
        assert!(
            reaches_terminal_within(&task, std::time::Duration::from_secs(5)).await,
            "PTY task never reached terminal status"
        );

        // Then the capture already holds that line
        let capture = task.channel("0").expect("channel 0").capture_arc();
        let output = String::from_utf8_lossy(capture.lock().unwrap().buffered_bytes()).into_owned();
        assert!(
            output.contains("late-line"),
            "capture at exit was {output:?}"
        );
    }

    /// The wait for the terminal to drain is bounded: a process the child left running does not
    /// hold its exit back. (On macOS the terminal is revoked as the child exits, so nothing can
    /// hold it; the bound is what keeps this true on Linux.)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_process_left_holding_the_terminal_does_not_hold_back_the_exit() {
        // Given a child that exits at once, leaving a long sleep on its terminal that ignores the
        // hangup
        let task = a_running_shell("(trap '' HUP; sleep 10) & exit 0").await;

        // When / Then the exit is reported well before that sleep ends
        assert!(
            reaches_terminal_within(&task, std::time::Duration::from_secs(4)).await,
            "the exit waited for the process left holding the terminal"
        );
    }
}
