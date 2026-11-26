use super::CliSessionManager;

use super::MAIN_TERMINAL_ID;

use tddy_task::{TaskId, TaskStatus};
use tokio::sync::{oneshot, watch};

use crate::cli_session_manager::pty_handle;
use tddy_pty::runtime::PtyReady;

use tddy_task::TaskHandle;

use super::{ExitSettlements, TerminalEntry};

use super::tmux_reattach::{classify_ended_main, MainExit, TmuxRun};

use tddy_terminal_rpc::pty_runtime::PtyRuntime;

use tddy_terminal_rpc::service::TMUX_KIND;

use tddy_terminal_rpc::pty_runtime::PtySpawnSpec;

use std::sync::Arc;

use std::path::PathBuf;

/// Work to finish for a terminal once its process has exited and its output is complete: it
/// receives the terminal's capture. The terminal leaves the index only once the returned future
/// completes, and [`CliSessionManager::exit_hook_settled`] waits for it — which is how a resume
/// reads what the previous run's hook recorded.
pub type ExitHook = Box<
    dyn FnOnce(
            Arc<std::sync::Mutex<tddy_task::TerminalCapture>>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send,
>;

/// What an ended main announces: an agent that ended with its main is announced with how it ended;
/// a client that ended while its agent lives announces nothing.
fn ended_agent_announcement(
    exit: MainExit,
    session_id: &str,
    tmux_run: &TmuxRun,
    status: &TaskStatus,
) -> Option<super::AgentEnd> {
    match exit {
        MainExit::AgentEnded => Some(super::AgentEnd::for_ended_main(
            session_id.to_string(),
            tmux_run,
            status,
        )),
        MainExit::ClientOnly => None,
    }
}

/// Wait until `status_rx` reports a terminal status, or its sender goes away.
async fn await_terminal_status(status_rx: &mut watch::Receiver<TaskStatus>) {
    loop {
        if status_rx.borrow().is_terminal() {
            break;
        }
        if status_rx.changed().await.is_err() {
            break;
        }
    }
}

/// Remove `terminal_id` from `session_id`'s index entry — only the entry `task_id` registered,
/// so a relaunched terminal is not removed by its predecessor's cleanup — and drop the entry
/// when its last terminal goes.
async fn deregister_terminal(
    terminals: &super::TerminalIndex,
    session_id: &str,
    terminal_id: &str,
    task_id: TaskId,
) {
    let mut reg = terminals.write().await;
    if let Some(tools) = reg.get_mut(session_id) {
        if tools.get(terminal_id).is_some_and(|e| e.task_id == task_id) {
            tools.remove(terminal_id);
        }
        if tools.is_empty() {
            reg.remove(session_id);
        }
    }
}

impl CliSessionManager {
    /// Spawn `argv` in a PTY as an identified tool and register it under `(session_id, terminal_id)`.
    ///
    /// Shared by [`start`](Self::start) (the `claude` tool) and
    /// [`start_terminal`](Self::start_terminal) (Bash tools). `on_exit` runs to completion when the
    /// process has exited, before the terminal is deregistered (see [`ExitHook`]).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_tool(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
        on_exit: Option<ExitHook>,
        tmux_session: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.spawn_tool_on(
            session_id,
            terminal_id,
            kind,
            worktree_path,
            model,
            argv,
            env,
            tddy_pty::PtyEnvBase::Inherited,
            None,
            os_user,
            on_exit,
            TmuxRun::with_location(tmux_session, os_user, None),
        )
        .await
    }

    /// [`Self::spawn_tool`] whose child starts from `env_base` before `env` is applied: an agent
    /// main starts from [`tddy_pty::PtyEnvBase::Cleared`] (`#agent-tools 28/28`). A transport
    /// client's main names how its agent's status is read in `exit_code_in_output`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_tool_on(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        env: Vec<(String, String)>,
        env_base: tddy_pty::PtyEnvBase,
        exit_code_in_output: Option<tddy_pty::ExitCodeInOutput>,
        os_user: Option<&str>,
        on_exit: Option<ExitHook>,
        tmux_run: TmuxRun,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let (ready_tx, ready_rx) = oneshot::channel();
        let mut spec = build_pty_spawn_spec(
            (session_id, terminal_id, kind),
            &worktree_path,
            argv,
            env,
            env_base,
            os_user,
        );
        spec.exit_code_in_output = exit_code_in_output;

        // The main terminal's recording is finished by the PTY runtime itself, before the terminal
        // leaves the PTY registry: a stopped main must answer history from its recording the moment
        // it can no longer be found running.
        let recording_hook = (terminal_id == MAIN_TERMINAL_ID).then(|| {
            let main_recordings = Arc::clone(&self.main_recordings);
            let session_id = session_id.to_string();
            let hook: tddy_pty::PtyExitHook = Box::new(move |task_id| {
                Box::pin(async move {
                    super::capture_recorder::finish_main_recording(
                        &main_recordings,
                        &session_id,
                        &task_id,
                    )
                    .await;
                })
            });
            hook
        });
        let task = PtyRuntime::spawn_with_exit_hook(
            &self.task_registry,
            &self.pty_registry,
            spec,
            ready_tx,
            recording_hook,
        )
        .await;

        let ready = await_pty_ready(ready_rx).await?;

        let mut handle =
            self.build_pty_handle(task, terminal_id, kind, worktree_path, model, ready)?;
        handle.tmux_session = tmux_run.session().map(str::to_string);

        self.register_spawned_terminal(session_id, terminal_id, &handle)
            .await;

        let on_exit = match on_exit {
            Some(hook) => Some(
                self.pending_exit(session_id, terminal_id, &handle.task_id, hook)
                    .await,
            ),
            None => None,
        };
        self.spawn_cleanup_for((session_id, terminal_id, kind), handle, on_exit, tmux_run)
    }

    fn spawn_cleanup_for(
        &self,
        (session_id, terminal_id, kind): (&str, &str, &str),
        handle: super::PtyHandle,
        on_exit: Option<PendingExit>,
        tmux_run: TmuxRun,
    ) -> anyhow::Result<Arc<super::PtyHandle>> {
        self.spawn_terminal_cleanup(
            session_id.to_string(),
            terminal_id.to_string(),
            handle.task_id.clone(),
            super::main_exit::MainExitWatch::new(handle.pid),
            on_exit,
            // A tmux attach of the user's own session is a client of nothing the session keeps; a
            // main, or a tab wrapping an agent in tmux, is a client of its agent.
            if kind == TMUX_KIND {
                tmux_run.not_a_client()
            } else {
                tmux_run
            },
        );

        finish_spawned_terminal(session_id, terminal_id, kind, handle)
    }

    async fn register_spawned_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
        handle: &super::PtyHandle,
    ) {
        self.terminals
            .write()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(
                terminal_id.to_string(),
                TerminalEntry {
                    task_id: handle.task_id.clone(),
                    worktree_path: handle.worktree_path.clone(),
                    model: handle.model.clone(),
                    tmux_session: handle.tmux_session.clone(),
                },
            );
    }

    pub(crate) fn build_pty_handle(
        &self,
        task: Arc<TaskHandle>,
        terminal_id: &str,
        kind: &str,
        worktree_path: PathBuf,
        model: &str,
        ready: PtyReady,
    ) -> anyhow::Result<pty_handle::PtyHandle> {
        let channel = task
            .channel("0")
            .ok_or_else(|| anyhow::anyhow!("PTY task missing channel 0"))?;
        let stdin_tx = channel
            .stdin_sender()
            .ok_or_else(|| anyhow::anyhow!("PTY channel missing stdin"))?;

        let (pty_done_tx, pty_done_rx) = watch::channel(false);
        let mut status_rx = task.status_watch();
        tokio::spawn(async move {
            loop {
                if status_rx.borrow().is_terminal() {
                    let _ = pty_done_tx.send(true);
                    break;
                }
                if status_rx.changed().await.is_err() {
                    break;
                }
            }
        });

        Ok(pty_handle::PtyHandle {
            terminal_id: terminal_id.to_string(),
            kind: kind.to_string(),
            worktree_path,
            model: model.to_string(),
            // Set by the caller that knows the terminal is a tmux one; see `spawn_tool`.
            tmux_session: None,
            stdin_tx,
            stdout_tx: channel.output_broadcast(),
            capture: channel.capture_arc(),
            pid: ready.pid,
            master: ready.master,
            pty_done: pty_done_rx,
            status_rx: task.status_watch(),
            current_size: ready.current_size,
            task_id: task.id.clone(),
            channel,
        })
    }

    pub(crate) fn spawn_terminal_cleanup(
        &self,
        session_id: String,
        terminal_id: String,
        task_id: TaskId,
        main_exit: super::main_exit::MainExitWatch,
        on_exit: Option<PendingExit>,
        tmux_run: TmuxRun,
    ) {
        let terminals = Arc::clone(&self.terminals);
        let exit_settlements = Arc::clone(&self.exit_settlements);
        let task_registry = self.task_registry.clone();
        let detach_watch = self.detach_watch().for_terminal(&terminal_id);
        tokio::spawn(async move {
            let task = match task_registry.get(&task_id).await {
                Some(t) => t,
                None => return,
            };
            let mut status_rx = task.status_watch();
            await_terminal_status(&mut status_rx).await;
            let status = task.status();
            // A tmux client's end is not yet the agent's: the session stays live until that is known.
            let settling = tmux_run.is_tmux_run();
            if settling {
                detach_watch.settling(&session_id).await;
            }
            if terminal_id == MAIN_TERMINAL_ID {
                super::main_exit::log_main_exit(&session_id, &main_exit, &status);
            }
            // The process has exited and its output is complete. The hook finishes before the
            // terminal leaves the index, and before a waiting resume goes on.
            if let Some(pending) = on_exit {
                let key = (session_id.clone(), terminal_id.clone());
                pending.finish(&task, key, &exit_settlements).await;
            }
            // Only a main or a tmux client of an agent can be ending its agent: a shell, or a tab on
            // the user's own tmux session, is a client of nothing the session keeps.
            let main_exit = if terminal_id == MAIN_TERMINAL_ID || tmux_run.is_tmux_run() {
                Some(classify_ended_main(tmux_run.clone()).await)
            } else {
                None
            };
            if matches!(main_exit, Some(MainExit::ClientOnly)) {
                detach_watch
                    .clone()
                    .hold_detached_agent(session_id.clone(), tmux_run.clone())
                    .await;
            }
            deregister_terminal(&terminals, &session_id, &terminal_id, task_id).await;
            if settling {
                detach_watch.settled(&session_id, main_exit).await;
            }
            // A tab's agent ending leaves the session's own agent state, and is not the session's end.
            if let Some(exit) = main_exit.filter(|_| terminal_id == MAIN_TERMINAL_ID) {
                let end = ended_agent_announcement(exit, &session_id, &tmux_run, &status);
                detach_watch.main_ended(session_id, exit, end).await;
            }
        });
    }

    /// Registers `hook` as `task_id`'s pending exit, for [`Self::exit_hook_settled`] to wait on.
    async fn pending_exit(
        &self,
        session_id: &str,
        terminal_id: &str,
        task_id: &TaskId,
        hook: ExitHook,
    ) -> PendingExit {
        let (settled, settled_rx) = watch::channel(false);
        self.exit_settlements.write().await.insert(
            (session_id.to_string(), terminal_id.to_string()),
            (task_id.clone(), settled_rx),
        );
        PendingExit { hook, settled }
    }

    /// Waits until the exit hook of `session_id`'s main terminal has finished, when its process has
    /// exited and the hook is still running — so a resume reads what the hook recorded. Returns at
    /// once when there is no hook pending, or when the process is still running (nothing has been
    /// recorded yet, and waiting for its exit would hold the caller for the whole run).
    pub async fn exit_hook_settled(&self, session_id: &str) {
        let key = (session_id.to_string(), MAIN_TERMINAL_ID.to_string());
        let Some((task_id, mut settled)) = self.exit_settlements.read().await.get(&key).cloned()
        else {
            return;
        };
        // The PTY runtime drops a terminal's control the moment its process exits.
        if self.pty_registry.get(&task_id).await.is_some() {
            return;
        }
        let _ = settled.wait_for(|done| *done).await;
    }

    pub(crate) async fn resolve_pty_handle(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<pty_handle::PtyHandle>> {
        let entry = self
            .terminals
            .read()
            .await
            .get(session_id)?
            .get(terminal_id)?
            .clone();
        let task = self.task_registry.get(&entry.task_id).await?;
        let control = self.pty_registry.get(&entry.task_id).await?;
        let ready = PtyReady {
            pid: task.pid_slot.lock().unwrap().first().copied().unwrap_or(0),
            master: control.master,
            current_size: control.current_size,
        };
        let mut handle = self
            .build_pty_handle(
                task,
                terminal_id,
                &control.kind,
                entry.worktree_path,
                &entry.model,
                ready,
            )
            .ok()?;
        handle.tmux_session = entry.tmux_session;
        Some(Arc::new(handle))
    }
}

fn build_pty_spawn_spec(
    (session_id, terminal_id, kind): (&str, &str, &str),
    worktree_path: &std::path::Path,
    argv: Vec<String>,
    env: Vec<(String, String)>,
    env_base: tddy_pty::PtyEnvBase,
    os_user: Option<&str>,
) -> PtySpawnSpec {
    PtySpawnSpec {
        argv,
        worktree_path: worktree_path.to_owned(),
        session_id: session_id.to_string(),
        terminal_id: terminal_id.to_string(),
        kind: kind.to_string(),
        env,
        env_base,
        os_user: os_user.map(str::to_string),
        exit_code_in_output: None,
    }
}

fn finish_spawned_terminal(
    session_id: &str,
    terminal_id: &str,
    kind: &str,
    handle: super::PtyHandle,
) -> anyhow::Result<Arc<super::PtyHandle>> {
    log::info!(
        target: "tddy_daemon::claude_cli_session",
        "spawn_tool: registered session={} terminal={} kind={} pid={} task_id={}",
        session_id,
        terminal_id,
        kind,
        handle.pid,
        handle.task_id
    );

    Ok(Arc::new(handle))
}

async fn await_pty_ready(
    ready_rx: oneshot::Receiver<Result<PtyReady, String>>,
) -> anyhow::Result<PtyReady> {
    let ready = ready_rx
        .await
        .map_err(|_| anyhow::anyhow!("PTY runtime did not signal ready"))?
        .map_err(|e| anyhow::anyhow!("PTY spawn failed: {e}"))?;
    Ok(ready)
}

/// An exit hook, and the flag that tells [`CliSessionManager::exit_hook_settled`] it has finished.
pub(crate) struct PendingExit {
    hook: ExitHook,
    settled: watch::Sender<bool>,
}

impl PendingExit {
    /// Runs the hook on `task`'s capture, then tells any waiting resume it has finished and forgets
    /// it — unless `key` has since been taken by a later run of the same terminal.
    async fn finish(self, task: &TaskHandle, key: (String, String), settlements: &ExitSettlements) {
        if let Some(channel) = task.channel("0") {
            (self.hook)(channel.capture_arc()).await;
        }
        let _ = self.settled.send(true);
        let mut pending = settlements.write().await;
        if pending.get(&key).is_some_and(|(id, _)| *id == task.id) {
            pending.remove(&key);
        }
    }
}
