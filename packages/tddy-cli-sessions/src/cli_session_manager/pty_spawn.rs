use super::CliSessionManager;

use super::MAIN_TERMINAL_ID;

use tddy_task::TaskId;
use tokio::sync::{oneshot, watch};

use crate::cli_session_manager::pty_handle;
use tddy_pty::runtime::PtyReady;

use tddy_task::TaskHandle;

use super::{ExitSettlements, TerminalEntry};

use super::tmux_reattach::{classify_ended_main, TmuxRun};

use tddy_terminal_rpc::pty_runtime::PtyRuntime;

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
            (env, tddy_pty::PtyEnvBase::Inherited),
            os_user,
            on_exit,
            tmux_session,
        )
        .await
    }

    /// [`Self::spawn_tool`] whose child starts from `env.1` before `env.0` is applied: an agent
    /// main starts from [`tddy_pty::PtyEnvBase::Cleared`] (`#agent-tools 28/28`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_tool_on(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        (env, env_base): (Vec<(String, String)>, tddy_pty::PtyEnvBase),
        os_user: Option<&str>,
        on_exit: Option<ExitHook>,
        tmux_session: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let (ready_tx, ready_rx) = oneshot::channel();
        let spec = PtySpawnSpec {
            argv,
            worktree_path: worktree_path.clone(),
            session_id: session_id.to_string(),
            terminal_id: terminal_id.to_string(),
            kind: kind.to_string(),
            env,
            env_base,
            os_user: os_user.map(str::to_string),
        };

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

        let ready = ready_rx
            .await
            .map_err(|_| anyhow::anyhow!("PTY runtime did not signal ready"))?
            .map_err(|e| anyhow::anyhow!("PTY spawn failed: {e}"))?;

        let mut handle =
            self.build_pty_handle(task, terminal_id, kind, worktree_path, model, ready)?;
        handle.tmux_session = tmux_session.map(str::to_string);

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

        let on_exit = match on_exit {
            Some(hook) => Some(
                self.pending_exit(session_id, terminal_id, &handle.task_id, hook)
                    .await,
            ),
            None => None,
        };
        self.spawn_terminal_cleanup(
            session_id.to_string(),
            terminal_id.to_string(),
            handle.task_id.clone(),
            on_exit,
            TmuxRun::new(tmux_session, os_user),
        );

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
        on_exit: Option<PendingExit>,
        tmux_run: TmuxRun,
    ) {
        let terminals = Arc::clone(&self.terminals);
        let exit_settlements = Arc::clone(&self.exit_settlements);
        let task_registry = self.task_registry.clone();
        let detach_watch = self.detach_watch();
        tokio::spawn(async move {
            let task = match task_registry.get(&task_id).await {
                Some(t) => t,
                None => return,
            };
            let mut status_rx = task.status_watch();
            loop {
                if status_rx.borrow().is_terminal() {
                    break;
                }
                if status_rx.changed().await.is_err() {
                    break;
                }
            }
            // The process has exited and its output is complete. The hook finishes before the
            // terminal leaves the index, and before a waiting resume goes on.
            if let Some(pending) = on_exit {
                let key = (session_id.clone(), terminal_id.clone());
                pending.finish(&task, key, &exit_settlements).await;
            }
            // Only a main's exit can end the agent: a tab is a client of nothing the session keeps.
            let main_exit = if terminal_id == MAIN_TERMINAL_ID {
                Some(classify_ended_main(tmux_run.clone()).await)
            } else {
                None
            };
            let mut reg = terminals.write().await;
            if let Some(tools) = reg.get_mut(&session_id) {
                if tools
                    .get(&terminal_id)
                    .is_some_and(|e| e.task_id == task_id)
                {
                    tools.remove(&terminal_id);
                }
                if tools.is_empty() {
                    reg.remove(&session_id);
                }
            }
            if let Some(exit) = main_exit {
                detach_watch.main_ended(session_id, exit, tmux_run).await;
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
