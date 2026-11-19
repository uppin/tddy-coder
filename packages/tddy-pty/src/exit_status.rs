//! How a PTY child's exit reads as a task's terminal status.

use std::sync::Mutex;

use tddy_task::{TaskStatus, TerminalCapture};

/// How much of an ended child's last output a reported status is looked for in: far more than a
/// client's closing error lines.
const REPORTED_EXIT_TAIL_BYTES: usize = 4096;

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

/// A child whose own exit status is not its command's: a transport client that exits
/// `client_code` for any remote command that failed and prints that command's status last, as the
/// decimal right after `marker` (Coder's `coder ssh`: exits 1 and ends with `Process exited with
/// status 3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitCodeInOutput {
    pub client_code: i32,
    pub marker: String,
}

impl ExitCodeInOutput {
    /// The status the command ended with, read from the child's `status` and its `output`: the code
    /// after the last `marker` when the child exited `client_code`, else `status` unchanged — a
    /// clean exit, a signal, or a failure the client did not attribute to the command.
    pub fn read(&self, status: TaskStatus, output: &[u8]) -> TaskStatus {
        if status
            != (TaskStatus::Completed {
                exit_code: Some(self.client_code),
            })
        {
            return status;
        }
        let output = String::from_utf8_lossy(output);
        let reported = output.rfind(&self.marker).and_then(|at| {
            let after = &output[at + self.marker.len()..];
            let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            after[..digits].parse::<i32>().ok()
        });
        match reported {
            Some(code) => TaskStatus::Completed {
                exit_code: Some(code),
            },
            None => status,
        }
    }

    /// [`Self::read`] over the last [`REPORTED_EXIT_TAIL_BYTES`] of an ended child's `capture`.
    pub(crate) fn read_from_capture(
        &self,
        status: TaskStatus,
        capture: &Mutex<TerminalCapture>,
    ) -> TaskStatus {
        let output = capture
            .lock()
            .map(|capture| {
                let bytes = capture.buffered_bytes();
                bytes[bytes.len().saturating_sub(REPORTED_EXIT_TAIL_BYTES)..].to_vec()
            })
            .unwrap_or_default();
        self.read(status, &output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portable_pty::ExitStatus;

    /// How Coder's CLI reports a failed command: it exits 1 and names the status last.
    fn coder_s_report() -> ExitCodeInOutput {
        ExitCodeInOutput {
            client_code: 1,
            marker: "Process exited with status ".to_string(),
        }
    }

    #[test]
    fn a_client_failure_reads_as_the_status_it_reported_last() {
        // Given a client that exited 1 after the command's status, in the colours the CLI prints
        let output =
            b"agent output\r\n\x1b[31mEncountered an error running \"coder ssh\"\x1b[0m\r\n\
            \x1b[31merror: \x1b[0m\x1b[31mrun command: Process exited with status 3\x1b[0m\r\n";

        // When its exit is read
        let status = coder_s_report().read(TaskStatus::Completed { exit_code: Some(1) }, output);

        // Then it is the command's own status
        assert_eq!(status, TaskStatus::Completed { exit_code: Some(3) });
    }

    #[test]
    fn the_last_report_is_the_one_read() {
        // Given output in which the agent itself once printed the marker, before the client's report
        let output = b"Process exited with status 7\r\nerror: run command: Process exited with status 42\r\n";

        // When its exit is read
        let status = coder_s_report().read(TaskStatus::Completed { exit_code: Some(1) }, output);

        // Then the client's last report wins
        assert_eq!(
            status,
            TaskStatus::Completed {
                exit_code: Some(42)
            }
        );
    }

    #[test]
    fn a_status_the_client_did_not_attribute_to_the_command_is_kept() {
        // Given three ends the report does not apply to
        let report = coder_s_report();
        let output = b"error: run command: Process exited with status 3\r\n";
        let kept = [
            // a clean exit
            report.read(TaskStatus::Completed { exit_code: Some(0) }, output),
            // another code than the client's failure code
            report.read(TaskStatus::Completed { exit_code: Some(2) }, output),
            // the client's failure code with no report: the client itself failed
            report.read(
                TaskStatus::Completed { exit_code: Some(1) },
                b"dial tcp: refused\r\n",
            ),
        ];

        // Then each status is the client's own
        assert_eq!(
            kept,
            [
                TaskStatus::Completed { exit_code: Some(0) },
                TaskStatus::Completed { exit_code: Some(2) },
                TaskStatus::Completed { exit_code: Some(1) },
            ]
        );
    }

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
