use super::CliSessionManager;

use tddy_task::TaskId;

use std::path::PathBuf;

use super::MAIN_TERMINAL_ID;

use super::pty_handle;

use tddy_terminal_rpc::service::TMUX_KIND;

use crate::tmux::kill_tmux_session;

use std::sync::Arc;

/// Kill the tmux session `name` of a terminal registered as `os_user`, in the blocking pool. A
/// failure is logged and not returned: shutdown must still stop every other child.
async fn kill_tmux_session_at_shutdown(name: String, os_user: Option<String>) {
    let Some(os_user) = os_user else {
        log::warn!(
            target: "tddy_daemon::claude_cli_session",
            "kill_all: tmux session {name:?} has no OS user recorded, so it is not killed"
        );
        return;
    };
    let killed = tokio::task::spawn_blocking({
        let name = name.clone();
        move || kill_tmux_session(&os_user, &name)
    })
    .await;
    match killed {
        Ok(Ok(())) => log::info!(
            target: "tddy_daemon::claude_cli_session",
            "kill_all: killed tmux session {name:?}"
        ),
        Ok(Err(error)) => log::warn!(
            target: "tddy_daemon::claude_cli_session",
            "kill_all: could not kill tmux session {name:?}, continuing shutdown: {error}"
        ),
        Err(error) => log::warn!(
            target: "tddy_daemon::claude_cli_session",
            "kill_all: the kill of tmux session {name:?} did not finish, continuing shutdown: {error}"
        ),
    }
}

impl CliSessionManager {
    /// Look up the **main** (`claude`) terminal of a session by id.
    ///
    /// Back-compat convenience: equivalent to `get_terminal(session_id, MAIN_TERMINAL_ID)`.
    pub async fn get(&self, session_id: &str) -> Option<Arc<pty_handle::PtyHandle>> {
        self.get_terminal(session_id, MAIN_TERMINAL_ID).await
    }

    /// Start a **Bash tool** attached to `session_id`: a shell (`shell_path`, resolved from
    /// `$SHELL` at the RPC layer) in `worktree_path`, taking no inputs. Returns the new handle with
    /// a fresh `terminal_id` (never the reserved `MAIN_TERMINAL_ID`) and kind `"bash"`.
    pub async fn start_terminal(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        shell_path: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let terminal_id = uuid::Uuid::now_v7().to_string();
        let argv = vec![shell_path.to_string()];
        self.spawn_tool(
            session_id,
            &terminal_id,
            "bash",
            worktree_path,
            "",
            argv,
            Vec::new(),
            None,
            None,
            None,
        )
        .await
    }

    /// Start an **agent terminal** attached to `session_id`: the CLI harness `kind` names (its
    /// session type, `"warp-cli"`, …) running `argv` in `worktree_path`. Returns the new handle with
    /// a fresh `terminal_id` (never the reserved `MAIN_TERMINAL_ID`) and that kind.
    pub async fn start_agent_terminal(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        kind: &str,
        argv: Vec<String>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let terminal_id = uuid::Uuid::now_v7().to_string();
        self.spawn_tool(
            session_id,
            &terminal_id,
            kind,
            worktree_path,
            "",
            argv,
            Vec::new(),
            None,
            // A tab's exit is never recorded: it carries no resume token of its own.
            None,
            None,
        )
        .await
    }

    /// Start a **tmux terminal** attached to `session_id`: `argv` (a tmux client attaching to, or
    /// creating, `tmux_session`) in `worktree_path`, run **as `os_user`** so it reaches that user's
    /// own tmux server. Returns the new handle with a fresh `terminal_id`, kind `"tmux"`, and
    /// `tmux_session` recorded so a listing can name it. Stopping it ends the client only, which
    /// detaches it.
    pub async fn start_tmux_terminal(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        os_user: &str,
        argv: Vec<String>,
        tmux_session: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let terminal_id = uuid::Uuid::now_v7().to_string();
        self.spawn_tool(
            session_id,
            &terminal_id,
            TMUX_KIND,
            worktree_path,
            "",
            argv,
            Vec::new(),
            Some(os_user),
            None,
            Some(tmux_session),
        )
        .await
    }

    /// Look up a specific tool of a session by `terminal_id` (use `MAIN_TERMINAL_ID` for the
    /// `claude` terminal).
    pub async fn get_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<pty_handle::PtyHandle>> {
        self.resolve_pty_handle(session_id, terminal_id).await
    }

    /// List all running tools of a session, including the `MAIN_TERMINAL_ID` terminal.
    pub async fn list_terminals(&self, session_id: &str) -> Vec<Arc<pty_handle::PtyHandle>> {
        let ids: Vec<String> = self
            .terminals
            .read()
            .await
            .get(session_id)
            .map(|tools| tools.keys().cloned().collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        for terminal_id in ids {
            if let Some(handle) = self.resolve_pty_handle(session_id, &terminal_id).await {
                out.push(handle);
            }
        }
        out
    }

    /// Stop a started tool: cancel its task and remove it from the registry.
    /// Returns `true` if the tool existed. The reserved `MAIN_TERMINAL_ID` is not stoppable
    /// here (callers must reject it before calling).
    pub async fn stop_terminal(&self, session_id: &str, terminal_id: &str) -> bool {
        let task_id = {
            let mut reg = self.terminals.write().await;
            let removed = reg
                .get_mut(session_id)
                .and_then(|tools| tools.remove(terminal_id));
            if reg.get(session_id).is_some_and(|tools| tools.is_empty()) {
                reg.remove(session_id);
            }
            removed.map(|e| e.task_id)
        };

        match task_id {
            Some(id) => {
                self.task_registry.cancel_task(&id).await;
                true
            }
            None => false,
        }
    }

    /// Stop all PTY terminals belonging to `session_id`: cancel each task and remove from registry.
    ///
    /// Called when a session ends (natural exit or explicit termination) so its processes don't
    /// outlive the session. This is the per-session counterpart to [`kill_all`].
    pub async fn stop_session(&self, session_id: &str) {
        let task_ids: Vec<TaskId> = {
            let mut reg = self.terminals.write().await;
            reg.remove(session_id)
                .map(|tools| tools.into_values().map(|e| e.task_id).collect())
                .unwrap_or_default()
        };

        for task_id in task_ids {
            log::info!(
                target: "tddy_daemon::claude_cli_session",
                "stop_session: session={} task_id={} — cancelling task",
                session_id,
                task_id
            );
            self.task_registry.cancel_task(&task_id).await;
        }
    }

    /// Kill all tracked PTY processes across every session: kill the tmux session of each tmux-run
    /// agent, cancel each task, wait up to 5 s, then SIGKILL any child PIDs that remain. Clears the
    /// terminal index on completion.
    ///
    /// Called during daemon shutdown so that spawned `claude` / shell processes do not
    /// outlive the daemon as orphans.
    pub async fn kill_all(&self) {
        let (task_ids, tmux_sessions): (Vec<TaskId>, Vec<(String, Option<String>)>) = {
            let mut reg = self.terminals.write().await;
            let mut ids = Vec::new();
            let mut tmux = Vec::new();
            for entry in reg.values().flat_map(|tools| tools.values()) {
                ids.push(entry.task_id.clone());
                if let Some(name) = &entry.tmux_session {
                    tmux.push((name.clone(), entry.os_user.clone()));
                }
            }
            reg.clear();
            (ids, tmux)
        };

        // A tmux-run agent lives in its tmux session, not in the terminal's PTY: that session is
        // killed first, as the user it was started as. Each kill is logged on its own, so one that
        // fails does not stop the rest.
        for (name, os_user) in tmux_sessions {
            kill_tmux_session_at_shutdown(name, os_user).await;
        }

        if task_ids.is_empty() {
            log::debug!(
                target: "tddy_daemon::claude_cli_session",
                "kill_all: no registered sessions — nothing to terminate"
            );
            return;
        }

        let pids: Vec<u32> = {
            let mut collected = Vec::new();
            for id in &task_ids {
                if let Some(handle) = self.task_registry.get(id).await {
                    collected.extend(handle.pid_slot.lock().unwrap().iter().copied());
                    handle.cancel.cancel();
                }
            }
            collected
        };

        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "kill_all: cancelling {} task(s), {} pid(s): {:?}",
            task_ids.len(),
            pids.len(),
            pids
        );

        for id in &task_ids {
            self.task_registry.cancel_task(id).await;
        }

        tokio::task::spawn_blocking(move || {
            #[cfg(unix)]
            {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while std::time::Instant::now() < deadline {
                    let any_alive = pids
                        .iter()
                        .any(|&pid| unsafe { libc::kill(pid as i32, 0) } == 0);
                    if !any_alive {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let still_alive: Vec<u32> = pids
                    .iter()
                    .copied()
                    .filter(|&pid| unsafe { libc::kill(pid as i32, 0) } == 0)
                    .collect();
                if still_alive.is_empty() {
                    log::info!(
                        target: "tddy_daemon::claude_cli_session",
                        "kill_all: all processes exited cleanly after cancel"
                    );
                } else {
                    log::warn!(
                        target: "tddy_daemon::claude_cli_session",
                        "kill_all: {} process(es) still alive after 5 s — sending SIGKILL: {:?}",
                        still_alive.len(),
                        still_alive
                    );
                    for &pid in &still_alive {
                        let _ = tddy_session_activity::session_deletion::signal_pid(
                            pid as i32,
                            libc::SIGKILL,
                        );
                    }
                }
            }
        })
        .await
        .ok();
    }
}
