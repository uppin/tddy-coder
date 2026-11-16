//! Why a remote-shell call did not give its caller what it asked for.

use std::fmt;
use std::time::Duration;

use crate::ShellAddress;

/// The four things that can go wrong, kept apart so a caller can tell a dead host from a failed
/// command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteShellError {
    /// The transport could not reach the target: nothing ran there.
    Unreachable {
        address: ShellAddress,
        /// What the transport said.
        detail: String,
    },
    /// The command was still running at its deadline and was stopped.
    TimedOut {
        address: ShellAddress,
        after: Duration,
    },
    /// The command ran and exited non-zero, for a caller that needed it to succeed.
    Exited { status: i32, stderr: String },
    /// The transport's local client could not be started, or its pipes could not be served.
    Spawn(String),
}

impl fmt::Display for RemoteShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable { address, detail } => {
                write!(f, "{address} is unreachable: {detail}")
            }
            Self::TimedOut { address, after } => write!(
                f,
                "{address}: no answer within {after:?}; the command was stopped"
            ),
            Self::Exited { status, stderr } => write!(
                f,
                "the command exited with status {status}: {}",
                stderr.trim()
            ),
            Self::Spawn(message) => write!(f, "the remote shell could not be started: {message}"),
        }
    }
}

impl std::error::Error for RemoteShellError {}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn buildbox() -> ShellAddress {
        ShellAddress::new("ssh", "buildbox")
    }

    #[test]
    fn an_unreachable_target_names_the_target_and_what_the_transport_said() {
        // Given
        let error = RemoteShellError::Unreachable {
            address: buildbox(),
            detail: "Could not resolve hostname buildbox".to_string(),
        };

        // When / Then
        assert_eq!(
            error.to_string(),
            "ssh target buildbox is unreachable: Could not resolve hostname buildbox"
        );
    }

    #[test]
    fn a_timeout_names_the_target_and_the_deadline() {
        // Given
        let error = RemoteShellError::TimedOut {
            address: buildbox(),
            after: Duration::from_secs(60),
        };

        // When / Then
        assert_eq!(
            error.to_string(),
            "ssh target buildbox: no answer within 60s; the command was stopped"
        );
    }

    #[test]
    fn a_failed_command_carries_its_status_and_trimmed_stderr() {
        // Given
        let error = RemoteShellError::Exited {
            status: 128,
            stderr: "fatal: not a git repository\n".to_string(),
        };

        // When / Then
        assert_eq!(
            error.to_string(),
            "the command exited with status 128: fatal: not a git repository"
        );
    }

    #[test]
    fn a_spawn_failure_says_the_shell_could_not_be_started() {
        // Given
        let error = RemoteShellError::Spawn("ssh: No such file or directory".to_string());

        // When / Then
        assert_eq!(
            error.to_string(),
            "the remote shell could not be started: ssh: No such file or directory"
        );
    }
}
