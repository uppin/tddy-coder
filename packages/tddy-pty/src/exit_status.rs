//! How a PTY child's exit reads as a task's terminal status.

use tddy_task::TaskStatus;

/// The terminal status a child that exited with `status` reports: a normal exit keeps its code, and
/// an exit by signal is a failure naming the signal.
pub fn task_status_of(status: &portable_pty::ExitStatus) -> TaskStatus {
    if let Some(signal) = status.signal() {
        return TaskStatus::Failed {
            message: format!("terminated by signal {signal}"),
        };
    }
    TaskStatus::Completed {
        exit_code: Some(status.exit_code() as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portable_pty::ExitStatus;

    #[test]
    fn a_normal_exit_keeps_its_code() {
        // Given a child that exited with code 3
        let exited = ExitStatus::with_exit_code(3);

        // When its exit is read as a task status
        let status = task_status_of(&exited);

        // Then the code is the one it exited with
        assert_eq!(status, TaskStatus::Completed { exit_code: Some(3) });
    }

    #[test]
    fn a_clean_exit_is_a_completion_with_code_zero() {
        // Given a child that exited cleanly
        let exited = ExitStatus::with_exit_code(0);

        // When its exit is read as a task status
        let status = task_status_of(&exited);

        // Then it completed with code 0
        assert_eq!(status, TaskStatus::Completed { exit_code: Some(0) });
    }

    #[test]
    fn a_signal_is_a_failure_naming_it() {
        // Given a child the kernel terminated with SIGKILL
        let exited = ExitStatus::with_signal("Killed");

        // When its exit is read as a task status
        let status = task_status_of(&exited);

        // Then it is a failure that names the signal
        assert_eq!(
            status,
            TaskStatus::Failed {
                message: "terminated by signal Killed".to_string()
            }
        );
    }
}
