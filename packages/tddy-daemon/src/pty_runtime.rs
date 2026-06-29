//! PTY action runtime — spawns interactive tools as [`tddy_task::Task`] entries.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tddy_task::{TaskBody, TaskChannel, TaskContext, TaskHandle, TaskRegistry, TaskStatus};
use tokio::sync::{mpsc, oneshot};

use crate::pty_registry::{PtyControl, PtyRegistry};

/// Default terminal size for spawned PTY sessions.
pub const DEFAULT_TERM_ROWS: u16 = 24;
pub const DEFAULT_TERM_COLS: u16 = 220;

/// PTY reader buffer size.
const PTY_READ_BUF: usize = 4096;

/// Specification for spawning a process inside a PTY.
#[derive(Debug, Clone)]
pub struct PtySpawnSpec {
    pub argv: Vec<String>,
    pub worktree_path: PathBuf,
    pub session_id: String,
    pub terminal_id: String,
    pub kind: String,
}

/// Ready signal emitted once the PTY is open and the child has been spawned.
pub struct PtyReady {
    pub pid: u32,
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub current_size: Arc<Mutex<PtySize>>,
}

/// Spawn a PTY-backed tool as a registered task.
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

        std::thread::spawn(move || {
            if let Err(e) = open_pty_and_pump(spec, stdin_rx, output_ch_thread, setup_tx) {
                log::warn!(
                    target: "tddy_daemon::pty_runtime",
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

        if let Some(tx) = ready_tx {
            let _ = tx.send(Ok(PtyReady {
                pid: setup.pid,
                master: Arc::clone(&setup.master),
                current_size: Arc::clone(&setup.current_size),
            }));
        }

        self.pty_registry
            .insert(
                task_id.clone(),
                PtyControl {
                    master: Arc::clone(&setup.master),
                    current_size: Arc::clone(&setup.current_size),
                    terminal_id: self.spec.terminal_id.clone(),
                    kind: self.spec.kind.clone(),
                },
            )
            .await;

        let exit_status = tokio::select! {
            _ = cancel.cancelled() => {
                #[cfg(unix)]
                {
                    let _ = crate::session_deletion::signal_pid(setup.pid as i32, libc::SIGTERM);
                    let _ = crate::session_deletion::signal_pid(setup.pid as i32, libc::SIGKILL);
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

fn open_pty_and_pump(
    spec: PtySpawnSpec,
    mut stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    output_ch: Arc<TaskChannel>,
    setup_tx: oneshot::Sender<Result<SetupResult, String>>,
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
    let pair = pty_system
        .openpty(initial_size)
        .map_err(|e| format!("openpty failed: {e}"))?;

    let master = Arc::new(Mutex::new(pair.master));
    let current_size = Arc::new(Mutex::new(initial_size));
    let (exit_tx, exit_rx) = oneshot::channel();

    // Reader: start BEFORE child spawn so fast-exiting stubs cannot miss PTY output.
    let master_for_reader = Arc::clone(&master);
    let output_ch_reader = Arc::clone(&output_ch);
    std::thread::spawn(move || {
        let reader = {
            let m = master_for_reader.lock().unwrap();
            m.try_clone_reader()
        };
        match reader {
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::pty_runtime",
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

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn failed: {e}"))?;
    drop(pair.slave);

    let pid = child
        .process_id()
        .ok_or_else(|| "spawned child has no pid".to_string())?;

    let _ = setup_tx.send(Ok(SetupResult {
        pid,
        master: Arc::clone(&master),
        current_size: Arc::clone(&current_size),
        exit_rx,
    }));

    // Writer: stdin mpsc → PTY master.
    let master_for_writer = Arc::clone(&master);
    std::thread::spawn(move || {
        let writer = {
            let m = master_for_writer.lock().unwrap();
            m.take_writer()
        };
        match writer {
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::pty_runtime",
                    "PTY writer: take_writer failed: {e}"
                );
            }
            Ok(mut w) => {
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    tokio::task::block_in_place(|| loop {
                        let data = handle.block_on(stdin_rx.recv());
                        match data {
                            None => break,
                            Some(bytes) => {
                                if std::io::Write::write_all(&mut w, &bytes).is_err() {
                                    break;
                                }
                            }
                        }
                    });
                } else {
                    while let Some(bytes) = stdin_rx.blocking_recv() {
                        if std::io::Write::write_all(&mut w, &bytes).is_err() {
                            break;
                        }
                    }
                }
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
        let _ = exit_tx.send(status);
    });

    Ok(())
}
