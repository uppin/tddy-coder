use std::time::Duration;
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
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

/// Wait, bounded by `bound`, for `session_id`'s agent to record its pid in `worktree` on the target
/// — the remote command writes its [`crate::cli_session_pointer::remote_agent_pid_file`]
/// immediately before it `exec`s the agent — or for `handle`'s terminal to end, whichever comes
/// first.
pub(super) async fn wait_for_agent_start(
    shell: &dyn RemoteShell,
    pid: &crate::cli_session_pointer::RemoteAgentPid,
    handle: &PtyHandle,
    bound: Duration,
) -> AgentStart {
    let probe = pid.read_command();
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

/// How judging an agent's startup window ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum StartupJudgement {
    /// The agent (or its terminal) ended within the window: the launch fails.
    Exited,
    /// The window passed with the agent confirmed alive by an answered probe (or, with no pid to
    /// probe, with the terminal still up).
    Stayed,
    /// No probe was answered by the cap, with the terminal still up: the target went silent.
    NeverAnswered,
}

/// Judge whether the agent ends within `window`. Called only once the agent is known to have
/// started (its pid is recorded on the target), so the window runs from the agent's start: an
/// agent still running when it closes has started.
///
/// The agent's own liveness on the target is judged beside the terminal's end, because under
/// load the terminal's exit is reported long after the agent that caused it is gone: an agent
/// whose pid is dead on the target has exited, whatever its terminal has yet to say.
///
/// Each liveness probe may take up to `probe_bound`: over a transport whose round trip exceeds
/// the window, a probe bounded by what the window has left would never answer. An unanswered
/// probe is evidence of nothing, so probing continues; the window counts as passed only once a
/// probe that answered found the agent alive at or after the window's end. The terminal's end
/// stops the wait at any time. Probing is capped at `window + probe_bound` from the call: a
/// start with no answered probe by then is [`StartupJudgement::NeverAnswered`], never taken as
/// alive. Without a pid there is nothing to probe and the window elapses.
pub(super) async fn exits_during_startup(
    handle: &PtyHandle,
    window: Duration,
    shell: &dyn RemoteShell,
    agent_pid: Option<u32>,
    probe_bound: Duration,
) -> StartupJudgement {
    let deadline = tokio::time::Instant::now() + window;
    let cap = deadline + probe_bound;
    let mut probed_at = tokio::time::Instant::now() - Duration::from_secs(1);
    loop {
        if *handle.pty_done.borrow() {
            return StartupJudgement::Exited;
        }
        let now = tokio::time::Instant::now();
        match agent_pid {
            None => {
                if now >= deadline {
                    return StartupJudgement::Stayed;
                }
            }
            Some(pid) => {
                if now >= cap {
                    return StartupJudgement::NeverAnswered;
                }
                if probed_at.elapsed() >= Duration::from_millis(50) {
                    probed_at = now;
                    let probe_left = probe_bound.min(cap.saturating_duration_since(now));
                    let answered = shell
                        .exec(
                            tddy_remote_shell::ExecRequest::new(format!("kill -0 {pid}"))
                                .within(probe_left),
                        )
                        .await;
                    log::debug!(
                        target: "tddy_agent_launch::remote_agent_launch",
                        "liveness probe of agent pid {pid} (bound {probe_left:?}) answered after {:?}: {}",
                        now.elapsed(),
                        match &answered {
                            Ok(output) => format!("exit status {}", output.status),
                            Err(error) => format!("no answer: {error}"),
                        }
                    );
                    if let Ok(output) = answered {
                        if output.status != 0 {
                            return StartupJudgement::Exited;
                        }
                        if tokio::time::Instant::now() >= deadline {
                            return StartupJudgement::Stayed;
                        }
                    }
                }
            }
        }
        let pause = if now >= deadline {
            Duration::from_millis(10)
        } else {
            (deadline - now).min(Duration::from_millis(10))
        };
        tokio::time::sleep(pause).await;
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
