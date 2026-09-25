use super::CliSessionManager;

use super::MAIN_TERMINAL_ID;

use tddy_task::TaskId;
use tokio::sync::{oneshot, watch};

use crate::{cli_session_manager::pty_handle, pty_runtime::PtyReady};

use tddy_task::TaskHandle;

use super::TerminalEntry;

use crate::pty_runtime::PtyRuntime;

use crate::pty_runtime::PtySpawnSpec;

use std::sync::Arc;

use std::path::PathBuf;

impl CliSessionManager {
    /// Spawn `argv` in a PTY as an identified tool and register it under `(session_id, terminal_id)`.
    ///
    /// Shared by [`start`](Self::start) (the `claude` tool) and
    /// [`start_terminal`](Self::start_terminal) (Bash tools).
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
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let (ready_tx, ready_rx) = oneshot::channel();
        let spec = PtySpawnSpec {
            argv,
            worktree_path: worktree_path.clone(),
            session_id: session_id.to_string(),
            terminal_id: terminal_id.to_string(),
            kind: kind.to_string(),
            env,
            os_user: os_user.map(str::to_string),
        };

        let task = PtyRuntime::spawn(&self.task_registry, &self.pty_registry, spec, ready_tx).await;

        let ready = ready_rx
            .await
            .map_err(|_| anyhow::anyhow!("PTY runtime did not signal ready"))?
            .map_err(|e| anyhow::anyhow!("PTY spawn failed: {e}"))?;

        let handle = self.build_pty_handle(task, terminal_id, kind, worktree_path, model, ready)?;

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
                },
            );

        self.spawn_terminal_cleanup(
            session_id.to_string(),
            terminal_id.to_string(),
            handle.task_id.clone(),
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
    ) {
        let terminals = Arc::clone(&self.terminals);
        let managed_workflows = Arc::clone(&self.managed_workflows);
        let livekit_terminals = Arc::clone(&self.livekit_terminals);
        let task_registry = self.task_registry.clone();
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
            // The main claude terminal exiting ends the session — drop its managed workflow so the
            // per-session toolcall listener socket is cleaned up, and forget how it was exposed to
            // LiveKit: there is no terminal left to bridge, and an entry nobody removes would keep
            // a dead session's block waiting for a consumer that could only find an empty PTY.
            if terminal_id == MAIN_TERMINAL_ID {
                managed_workflows.write().await.remove(&session_id);
                livekit_terminals.write().await.remove(&session_id);
            }
        });
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
        self.build_pty_handle(
            task,
            terminal_id,
            &control.kind,
            entry.worktree_path,
            &entry.model,
            ready,
        )
        .ok()
        .map(Arc::new)
    }
}
