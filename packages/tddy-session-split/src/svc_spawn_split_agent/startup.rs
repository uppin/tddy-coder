//! The split agent's startup window: a split start reports success only once its agent has
//! survived the daemon's startup grace period, and fails with the agent's own last words otherwise.
#![allow(dead_code)] // TODO(agent-liveness): wired into `spawn_split_agent` once implemented.

use std::fmt;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tokio::sync::watch;

/// How many of the agent's last non-empty output lines a startup failure quotes.
pub(crate) const LAST_OUTPUT_LINES: usize = 20;

/// How long a freshly spawned split agent is watched, and how often: the daemon's own startup window
/// (`spawn_startup_grace_period_ms` / `spawn_startup_poll_interval_ms`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SplitAgentStartup {
    pub(crate) grace: Duration,
    pub(crate) poll: Duration,
}

impl SplitAgentStartup {
    /// The window `config` sets.
    pub(crate) fn from_config(config: &DaemonConfig) -> Self {
        Self {
            grace: config.spawn_startup_grace_period(),
            poll: config.spawn_startup_poll_interval(),
        }
    }
}

/// Whether the agent outlived its startup window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupOutcome {
    /// Still running when the window closed.
    Survived,
    /// Its terminal ended inside the window.
    Exited,
}

/// Watch `done` — the agent terminal's "has ended" signal — for `startup.grace`.
pub(crate) async fn await_split_agent_startup(
    done: watch::Receiver<bool>,
    startup: SplitAgentStartup,
) -> StartupOutcome {
    // TODO(agent-liveness): implement
    let _ = (done, startup);
    StartupOutcome::Survived
}

/// A split agent that exited inside its startup window: how it ended, and the tail of what it
/// printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitAgentDied {
    pub(crate) outcome: String,
    pub(crate) last_output: String,
}

impl SplitAgentDied {
    /// The death of an agent that ended with `outcome` (`exit code 3`, `terminated by signal …`),
    /// quoting the last [`LAST_OUTPUT_LINES`] non-empty lines of `capture`, terminal escapes removed.
    pub(crate) fn from_capture(outcome: &str, capture: &[u8]) -> Self {
        // TODO(agent-liveness): implement
        let _ = capture;
        Self {
            outcome: outcome.to_string(),
            last_output: String::new(),
        }
    }
}

impl fmt::Display for SplitAgentDied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO(agent-liveness): implement
        write!(f, "{}", self.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_window_of(grace_ms: u64) -> SplitAgentStartup {
        SplitAgentStartup {
            grace: Duration::from_millis(grace_ms),
            poll: Duration::from_millis(5),
        }
    }

    #[tokio::test]
    async fn an_agent_still_running_after_the_window_has_started() {
        // Given an agent whose terminal never ends
        let (_still_running, done) = watch::channel(false);

        // When its startup window passes
        let outcome = await_split_agent_startup(done, a_window_of(50)).await;

        // Then it has started
        assert_eq!(outcome, StartupOutcome::Survived);
    }

    #[tokio::test]
    async fn an_agent_that_exits_inside_the_window_has_not_started() {
        // Given an agent whose terminal ends 10 ms in
        let (ends, done) = watch::channel(false);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = ends.send(true);
        });

        // When its startup window is watched
        let outcome = await_split_agent_startup(done, a_window_of(2_000)).await;

        // Then it exited inside it
        assert_eq!(outcome, StartupOutcome::Exited);
    }

    #[tokio::test]
    async fn an_agent_whose_terminal_is_already_gone_has_not_started() {
        // Given an agent whose terminal's end was signalled by dropping the sender
        let (ends, done) = watch::channel(false);
        drop(ends);

        // When its startup window is watched
        let outcome = await_split_agent_startup(done, a_window_of(2_000)).await;

        // Then it exited
        assert_eq!(outcome, StartupOutcome::Exited);
    }

    #[test]
    fn the_last_lines_are_capped_and_stripped_of_terminal_escapes() {
        // Given an agent that printed 25 coloured lines and blank lines between them
        let mut printed = String::new();
        for n in 1..=25 {
            printed.push_str(&format!("\u{1b}[31mline {n}\u{1b}[0m\r\n\r\n"));
        }

        // When its death is described
        let died = SplitAgentDied::from_capture("exit code 1", printed.as_bytes());

        // Then only the last 20 non-empty lines are quoted, without escapes
        let expected: Vec<String> = (6..=25).map(|n| format!("line {n}")).collect();
        assert_eq!(died.last_output, expected.join("\n"));
    }

    #[test]
    fn a_death_reads_as_a_startup_failure_quoting_the_agent() {
        // Given an agent that printed one line and exited with code 3
        let died =
            SplitAgentDied::from_capture("exit code 3", b"boom: cannot reach the codebase\r\n");

        // When it is reported
        let reported = died.to_string();

        // Then the report says it exited during startup, how, and what it said
        assert_eq!(
            reported,
            "split agent exited during startup (exit code 3): boom: cannot reach the codebase"
        );
    }
}
