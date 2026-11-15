//! The one line the daemon logs when a session's `main` terminal ends.

use std::time::Duration;

use tddy_task::TaskStatus;

/// Describe the end of `session_id`'s `main` terminal: its pid, how it ended and how long it ran.
/// An agent that dies unseen is otherwise indistinguishable from one still running.
pub(crate) fn describe_main_exit(
    session_id: &str,
    pid: u32,
    status: &TaskStatus,
    ran: Duration,
) -> String {
    let outcome = match status {
        TaskStatus::Completed {
            exit_code: Some(code),
        } => format!("exit code {code}"),
        TaskStatus::Completed { exit_code: None } => "exit code unknown".to_string(),
        TaskStatus::Failed { message } => message.clone(),
        TaskStatus::Cancelled => "stopped by the daemon".to_string(),
        TaskStatus::Pending | TaskStatus::Running => "still running".to_string(),
    };
    format!(
        "main exited: session={session_id} pid={pid} outcome={outcome} ran={:.1}s",
        ran.as_secs_f64()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ended_main_is_described_with_its_outcome_and_lifetime() {
        // Given a main that exited with code 3 after 1.3 s
        let status = TaskStatus::Completed { exit_code: Some(3) };

        // When its end is described
        let line = describe_main_exit("01a1de51", 96609, &status, Duration::from_millis(1300));

        // Then the line names the session, the pid, the outcome and the lifetime
        assert_eq!(
            line,
            "main exited: session=01a1de51 pid=96609 outcome=exit code 3 ran=1.3s"
        );
    }

    #[test]
    fn a_signalled_main_names_the_signal() {
        // Given a main the kernel terminated
        let status = TaskStatus::Failed {
            message: "terminated by signal Killed".to_string(),
        };

        // When its end is described
        let line = describe_main_exit("s1", 7, &status, Duration::from_secs(40));

        // Then the outcome is the failure as reported
        assert_eq!(
            line,
            "main exited: session=s1 pid=7 outcome=terminated by signal Killed ran=40.0s"
        );
    }

    #[test]
    fn a_cancelled_main_says_the_daemon_stopped_it() {
        // Given a main the daemon cancelled
        let status = TaskStatus::Cancelled;

        // When its end is described
        let line = describe_main_exit("s1", 7, &status, Duration::from_secs(2));

        // Then the outcome says the daemon stopped it
        assert_eq!(
            line,
            "main exited: session=s1 pid=7 outcome=stopped by the daemon ran=2.0s"
        );
    }
}
