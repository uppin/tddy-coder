//! Unit tests: the status a target tab's failed launch reports ([`tab_launch_status`]) — one code
//! per kind of failure, in the error's own words, for shell and agent tabs alike.

use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_remote_shell::{RemoteShellError, ShellAddress};
use tddy_rpc::Code;

use super::tab_launch_status;
use crate::remote_agent_launch::RemoteLaunchError;
use crate::spec_launch::SpecLaunchError;
use crate::target_trust::TargetTrustError;

/// The code and message `error` is reported with.
fn reported(error: &RemoteLaunchError) -> (Code, String) {
    let status = tab_launch_status(error);
    (status.code(), status.message().to_string())
}

fn buildbox() -> ShellAddress {
    ShellAddress::new("ssh", "buildbox")
}

#[test]
fn a_target_its_transport_cannot_reach_is_unavailable_in_the_transports_words() {
    // Given
    let error = RemoteLaunchError::Shell(RemoteShellError::Unreachable {
        address: buildbox(),
        detail: "Could not resolve hostname buildbox".to_string(),
    });

    // When
    let status = reported(&error);

    // Then
    assert_eq!(
        status,
        (
            Code::Unavailable,
            "ssh target buildbox is unreachable: Could not resolve hostname buildbox".to_string()
        )
    );
}

#[test]
fn a_worktree_the_target_cannot_enter_is_unavailable() {
    // Given `cd` into the worktree failed through the remote shell
    let error = RemoteLaunchError::Shell(RemoteShellError::Exited {
        status: 1,
        stderr: "cd: /home/dev/repo/.worktrees/s1: No such file or directory".to_string(),
    });

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::Unavailable, error.to_string()));
}

#[test]
fn a_target_that_gives_no_answer_is_unavailable() {
    // Given
    let error = RemoteLaunchError::Shell(RemoteShellError::TimedOut {
        address: buildbox(),
        after: Duration::from_secs(60),
        command: "cd '/home/dev/repo/.worktrees/s1'".to_string(),
    });

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::Unavailable, error.to_string()));
}

#[test]
fn a_terminal_this_host_cannot_spawn_is_internal() {
    // Given
    let error = RemoteLaunchError::Spawn(
        "PTY spawn failed: Unable to spawn /nonexistent/ssh because it does not exist".to_string(),
    );

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::Internal, error.to_string()));
}

#[test]
fn an_agent_missing_on_the_target_is_a_failed_precondition_naming_its_command() {
    // Given
    let error = RemoteLaunchError::AgentNotOnTarget {
        command: "claude".to_string(),
    };

    // When
    let status = reported(&error);

    // Then
    assert_eq!(
        status,
        (
            Code::FailedPrecondition,
            "the agent `claude` is not installed on the target: its login shell finds no `claude` on its PATH"
                .to_string()
        )
    );
}

#[test]
fn a_spec_that_cannot_be_planned_stays_a_failed_precondition() {
    // Given
    let error = RemoteLaunchError::Plan(SpecLaunchError::UnknownAgent("acme".to_string()));

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::FailedPrecondition, error.to_string()));
}

#[test]
fn a_target_that_cannot_be_made_ready_stays_a_failed_precondition() {
    // Given
    let error = RemoteLaunchError::Trust(TargetTrustError::Malformed {
        file: "/home/dev/.claude.json".to_string(),
        message: "expected an object".to_string(),
    });

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::FailedPrecondition, error.to_string()));
}

#[test]
fn a_target_unreachable_while_it_is_made_ready_is_unavailable() {
    // Given the shell failed while pre-trusting the clone on the target
    let error = RemoteLaunchError::Trust(TargetTrustError::Shell(RemoteShellError::Unreachable {
        address: buildbox(),
        detail: "Connection refused".to_string(),
    }));

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::Unavailable, error.to_string()));
}

#[test]
fn an_agent_that_exits_while_starting_stays_a_failed_precondition() {
    // Given
    let error = RemoteLaunchError::ExitedDuringStartup {
        status: Some(127),
        last_output: "sh: claude: not found".to_string(),
    };

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::FailedPrecondition, error.to_string()));
}

#[test]
fn an_agent_that_never_starts_stays_a_failed_precondition() {
    // Given
    let error = RemoteLaunchError::AgentNeverStarted {
        after: Duration::from_secs(30),
    };

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::FailedPrecondition, error.to_string()));
}

#[test]
fn an_agent_that_never_answers_stays_a_failed_precondition() {
    // Given
    let error = RemoteLaunchError::AgentNeverAnswered {
        after: Duration::from_secs(30),
    };

    // When
    let status = reported(&error);

    // Then
    assert_eq!(status, (Code::FailedPrecondition, error.to_string()));
}
