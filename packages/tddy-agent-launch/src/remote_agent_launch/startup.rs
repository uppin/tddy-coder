use std::path::Path;
use std::time::Duration;
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_core::ssh_exec::shell_single_quote;
use tddy_remote_shell::RemoteShell;

/// How waiting for an agent to start on its target ended.
pub(super) enum AgentStart {
    /// The agent recorded its pid: it is running (or ran) on the target, under this pid.
    Started(u32),
    /// The session's terminal ended before the pid was recorded.
    Exited,
    /// The bound elapsed with the terminal still running and no pid recorded.
    NeverStarted,
}

/// Wait, bounded by `bound`, for the agent to record its pid in `worktree` on the target — the
/// remote command writes `.tddy/agent.pid` immediately before it `exec`s the agent — or for
/// `handle`'s terminal to end, whichever comes first.
pub(super) async fn wait_for_agent_start(
    shell: &dyn RemoteShell,
    worktree: &Path,
    handle: &PtyHandle,
    bound: Duration,
) -> AgentStart {
    let pid_file = worktree.join(crate::cli_session_pointer::REMOTE_AGENT_PID_FILE);
    let probe = format!("cat {}", shell_single_quote(&pid_file.to_string_lossy()));
    let deadline = tokio::time::Instant::now() + bound;
    loop {
        if *handle.pty_done.borrow() {
            return AgentStart::Exited;
        }
        // A probe may take as long as the bound has left: one round trip to a remote deployment
        // can outlast any fixed few seconds, and a probe cut short never sees the pid.
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if let Ok(output) = shell
            .exec(tddy_remote_shell::ExecRequest::new(probe.clone()).within(left))
            .await
        {
            if output.status == 0 {
                if let Ok(pid) = String::from_utf8_lossy(&output.stdout).trim().parse() {
                    return AgentStart::Started(pid);
                }
            }
        }
        if *handle.pty_done.borrow() {
            return AgentStart::Exited;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return AgentStart::NeverStarted;
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(50))).await;
    }
}

/// Whether the agent ends within `window`, failing the launch. Called only once the agent is
/// known to have started (its pid is recorded on the target), so the window runs from the
/// agent's start: an agent still running when it closes has started.
///
/// The agent's own liveness on the target is judged beside the terminal's end, because under
/// load the terminal's exit is reported long after the agent that caused it is gone: an agent
/// whose pid is dead on the target has exited, whatever its terminal has yet to say.
pub(super) async fn exits_during_startup(
    handle: &PtyHandle,
    window: Duration,
    shell: &dyn RemoteShell,
    agent_pid: Option<u32>,
) -> bool {
    let deadline = tokio::time::Instant::now() + window;
    let mut probed_at = tokio::time::Instant::now() - Duration::from_secs(1);
    loop {
        if *handle.pty_done.borrow() {
            return true;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return false;
        }
        if let Some(pid) = agent_pid {
            if probed_at.elapsed() >= Duration::from_millis(50) {
                probed_at = now;
                // Bounded by what the window has left, as the pid probe is by its bound; a probe
                // that does not answer in time leaves the agent judged alive.
                let alive = shell
                    .exec(
                        tddy_remote_shell::ExecRequest::new(format!("kill -0 {pid}"))
                            .within(deadline.saturating_duration_since(now)),
                    )
                    .await
                    .map(|output| output.status == 0)
                    .unwrap_or(true);
                if !alive {
                    return true;
                }
            }
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(10))).await;
    }
}

/// The tail of what `handle`'s terminal showed, once it has finished showing it. The task's
/// end is signalled from its exit status, which the PTY's reader can lag under load: the last
/// bytes are still in flight when the process is already known to have ended. Reading the
/// capture at that moment races the reader and can quote nothing — so wait until the captured
/// bytes stop changing (the reader has drained the PTY), bounded so a wedged reader cannot
/// hang the failed start.
pub(super) async fn settled_last_output(handle: &PtyHandle) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut previous = captured_bytes(handle);
    let mut unchanged_since = tokio::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let current = captured_bytes(handle);
        if current != previous {
            previous = current;
            unchanged_since = tokio::time::Instant::now();
            continue;
        }
        if unchanged_since.elapsed() >= Duration::from_millis(50)
            || tokio::time::Instant::now() >= deadline
        {
            break;
        }
    }
    last_output_of(&previous)
}

/// What `handle`'s terminal has captured so far.
fn captured_bytes(handle: &PtyHandle) -> Vec<u8> {
    handle
        .capture
        .lock()
        .map(|capture| capture.buffered_bytes().to_vec())
        .unwrap_or_default()
}

/// The tail of `capture`, as a startup failure quotes it.
fn last_output_of(capture: &[u8]) -> String {
    let text = String::from_utf8_lossy(capture);
    let mut lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim_end())
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.len() > 20 {
        lines.drain(..lines.len() - 20);
    }
    lines.join("\n")
}
