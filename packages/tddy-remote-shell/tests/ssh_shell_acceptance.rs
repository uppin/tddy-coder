//! `SshShell`: the OpenSSH transport of the remote-shell port, driven through a stand-in `ssh`
//! executable that runs the remote command on this host. No connection is opened.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-shell-port.md

mod support;

use std::ffi::OsString;
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use tddy_remote_shell::{
    ExecOutput, ExecRequest, InteractiveCommand, InteractiveExit, RemoteShell, RemoteShellError,
    ShellAddress, SshShell,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use support::{
    an_ssh_that_cannot_reach_its_target, an_ssh_that_never_answers, an_ssh_that_runs_commands_here,
};

/// What every call's arguments start with: the connection options, then the alias after `--`.
const OPTIONS_THEN_BUILDBOX: &str =
    "-o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=5 \
                                     -o ServerAliveCountMax=3 -- buildbox";

/// Bounds a step that would otherwise hang the suite on a transport that never relays. Far above
/// what the step needs.
const STEP_BUDGET: Duration = Duration::from_secs(10);

#[tokio::test]
async fn a_command_runs_on_the_target_and_returns_its_output_and_status() {
    // Given an ssh whose target runs commands
    let ssh = an_ssh_that_runs_commands_here();
    let shell = ssh.shell_on("buildbox");

    // When a command that prints on both streams and exits non-zero is run
    let output = shell
        .exec(ExecRequest::new("printf out; printf err >&2; exit 3"))
        .await;

    // Then its output and status come back as an output, and it ran on the alias
    assert_eq!(
        output,
        Ok(ExecOutput {
            status: 3,
            stdout: b"out".to_vec(),
            stderr: b"err".to_vec(),
        })
    );
    assert_eq!(
        ssh.calls(),
        vec![format!(
            "{OPTIONS_THEN_BUILDBOX} printf out; printf err >&2; exit 3"
        )]
    );
}

#[tokio::test]
async fn stdin_reaches_the_remote_command() {
    // Given an ssh whose target runs commands, and bytes that are not text
    let ssh = an_ssh_that_runs_commands_here();
    let shell = ssh.shell_on("buildbox");
    let bytes = vec![b'a', 0, 0xff, b'\n', b'z'];

    // When a command that copies its stdin is run with them
    let output = shell
        .exec(ExecRequest::new("cat").with_stdin(bytes.clone()))
        .await;

    // Then they come back byte for byte
    assert_eq!(output, Ok(ExecOutput::succeeded(bytes)));
}

#[tokio::test]
async fn a_command_that_outlives_its_deadline_is_stopped_and_reported_as_timed_out() {
    // Given an ssh that never returns
    let ssh = an_ssh_that_never_answers();
    let shell = ssh.shell_on("buildbox");
    let deadline = Duration::from_millis(300);

    // When a command is run through it with a short deadline
    let started = Instant::now();
    let outcome = shell.exec(ExecRequest::new("true").within(deadline)).await;

    // Then the call gives up at the deadline, naming the target
    assert!(
        matches!(
            &outcome,
            Err(RemoteShellError::TimedOut { address, after, .. })
                if *address == ShellAddress::new("ssh", "buildbox") && *after == deadline
        ),
        "{outcome:?}"
    );
    assert!(
        started.elapsed() < STEP_BUDGET,
        "the deadline must bound the call, which took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_one_shot_past_its_deadline_names_its_command() {
    // Given an ssh that never returns
    let ssh = an_ssh_that_never_answers();
    let shell = ssh.shell_on("buildbox");

    // When a command is run through it with a short deadline
    let error = shell
        .exec(ExecRequest::new("git -C /srv/repo fetch --all").within(Duration::from_millis(300)))
        .await
        .expect_err("the call gives up at its deadline");

    // Then the error says which command gave no answer, and the field carries it
    assert_eq!(
        error.to_string(),
        "ssh target buildbox: `git -C /srv/repo fetch --all` gave no answer within 300ms; the command was stopped"
    );
    assert!(
        matches!(&error, RemoteShellError::TimedOut { command, .. } if command == "git -C /srv/repo fetch --all"),
        "{error:?}"
    );
}

#[tokio::test]
async fn an_unreachable_target_is_reported_as_unreachable_not_as_a_failed_command() {
    // Given an ssh that cannot reach its target, as OpenSSH reports it: exit status 255
    let ssh = an_ssh_that_cannot_reach_its_target(
        "ssh: Could not resolve hostname buildbox: nodename nor servname provided",
    );
    let shell = ssh.shell_on("buildbox");

    // When a command is run through it
    let outcome = shell.exec(ExecRequest::new("true")).await;

    // Then the target is unreachable, with what ssh said — not an output with status 255
    assert_eq!(
        outcome,
        Err(RemoteShellError::Unreachable {
            address: ShellAddress::new("ssh", "buildbox"),
            detail: "ssh: Could not resolve hostname buildbox: nodename nor servname provided"
                .to_string(),
        })
    );
}

#[tokio::test]
async fn an_ssh_client_that_cannot_be_started_is_reported_as_a_spawn_failure() {
    // Given a shell whose ssh client does not exist
    let shell = SshShell::through("/no/such/dir/ssh", "buildbox");

    // When a command is run through it
    let outcome = shell.exec(ExecRequest::new("true")).await;

    // Then the failure is the local spawn's, naming the client
    assert!(
        matches!(&outcome, Err(RemoteShellError::Spawn(message)) if message.contains("/no/such/dir/ssh")),
        "expected a spawn failure naming /no/such/dir/ssh, got {outcome:?}"
    );
}

#[tokio::test]
async fn a_streamed_command_relays_both_directions_until_it_exits() {
    // Given an ssh whose target runs commands
    let ssh = an_ssh_that_runs_commands_here();
    let shell = ssh.shell_on("buildbox");

    // When a command that echoes its stdin, then complains and exits non-zero, is streamed
    let mut streamed = shell
        .exec_streamed("cat; printf done >&2; exit 4")
        .await
        .expect("the streamed command starts");
    let relayed = tokio::time::timeout(STEP_BUDGET, async {
        streamed
            .stdin
            .write_all(b"ping\n")
            .await
            .expect("stdin is open");
        streamed.stdin.flush().await.expect("stdin flushes");
        let mut echoed = [0; 5];
        streamed
            .stdout
            .read_exact(&mut echoed)
            .await
            .expect("stdout carries the echo while the command runs");
        streamed.stdin.shutdown().await.expect("stdin closes");
        let mut stderr = Vec::new();
        streamed
            .stderr
            .read_to_end(&mut stderr)
            .await
            .expect("stderr ends with the command");
        (echoed.to_vec(), stderr, streamed.exit.wait().await)
    })
    .await
    .expect("the streamed command relays and exits within the step budget");

    // Then the echo arrived while it ran, stderr stayed apart, and its exit status is reported
    assert_eq!(relayed, (b"ping\n".to_vec(), b"done".to_vec(), Ok(4)));
}

#[test]
fn the_interactive_command_asks_ssh_for_a_terminal_on_the_alias() {
    // Given a shell on an alias
    let shell = SshShell::new("buildbox");

    // When the interactive command for an agent is asked for
    let interactive = shell.interactive("cd '/home/dev/repo' && exec claude --resume");

    // Then it is ssh with a forced terminal, the connection options, and the command as one
    // argument after the alias — an argv a local PTY can run
    let argv: Vec<OsString> = [
        "ssh",
        "-tt",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "ServerAliveInterval=5",
        "-o",
        "ServerAliveCountMax=3",
        "--",
        "buildbox",
        "cd '/home/dev/repo' && exec claude --resume",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    assert_eq!(
        interactive,
        InteractiveCommand {
            argv,
            env: Vec::new(),
            exit: InteractiveExit::ClientStatus,
        }
    );
}

#[tokio::test]
async fn every_ssh_call_carries_the_same_connection_options() {
    // Given an ssh whose target runs commands
    let ssh = an_ssh_that_runs_commands_here();
    let shell = ssh.shell_on("buildbox");

    // When a one-shot command and a streamed command are run, and an interactive one is built
    shell
        .exec(ExecRequest::new("true"))
        .await
        .expect("the one-shot command runs");
    let streamed = shell
        .exec_streamed("true")
        .await
        .expect("the streamed command starts");
    let streamed_exit = tokio::time::timeout(STEP_BUDGET, streamed.exit.wait())
        .await
        .expect("the streamed command exits within the step budget");
    let interactive = shell.interactive("true");

    // Then each carried the one option set
    assert_eq!(streamed_exit, Ok(0));
    assert_eq!(
        ssh.calls(),
        vec![
            format!("{OPTIONS_THEN_BUILDBOX} true"),
            format!("{OPTIONS_THEN_BUILDBOX} true"),
        ]
    );
    let interactive_options: Vec<String> = interactive
        .argv
        .iter()
        .skip(2)
        .take(8)
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    assert_eq!(interactive_options, SshShell::ssh_options());
    assert_eq!(
        SshShell::ssh_options().join(" "),
        "-o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=5 -o ServerAliveCountMax=3"
    );
}
