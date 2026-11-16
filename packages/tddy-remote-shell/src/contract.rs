//! What every remote-shell transport does, as one function a transport's own suite calls.
//!
//! The commands are plain POSIX shell, so a transport driven through a stand-in client that runs
//! them on this host passes exactly when a real target would. The in-memory shell is given the
//! answers to the same commands by [`an_in_memory_shell_for_the_contract`].

use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::{
    ExecOutput, ExecRequest, FileMode, InMemoryShell, RemoteShell, RemoteShellError, ShellAddress,
};

/// Prints on both streams and exits non-zero.
const BOTH_STREAMS_THEN_THREE: &str = "printf out; printf err >&2; exit 3";
/// Copies its stdin to its stdout.
const ECHO_STDIN: &str = "cat";
/// Outlives any deadline the contract gives it.
const NEVER_FINISHES: &str = "sleep 30";
/// Echoes its stdin while it is open, then says so on stderr and exits non-zero.
const ECHO_THEN_FOUR: &str = "cat; printf done >&2; exit 4";

/// The deadline the command that never finishes is given.
const SHORT_DEADLINE: Duration = Duration::from_millis(300);
/// How long any one step of the contract may take before the transport is judged to hang. Far
/// above what a step needs: it only turns a hang into a failure.
const STEP_BUDGET: Duration = Duration::from_secs(10);

const LINE: &[u8] = b"ping\n";
/// Not text: a NUL, a byte that is no UTF-8, both line endings, and no trailing newline.
const FILE_BYTES: &[u8] = &[b'a', 0, 0xff, b'\r', b'\n', b'z'];

/// Asserts that `shell` behaves like a remote shell. `scratch_dir` is a directory on the target
/// the contract may create and write files under.
///
/// Panics on the first behaviour that differs, naming it.
pub async fn assert_behaves_like_a_remote_shell(shell: &dyn RemoteShell, scratch_dir: &str) {
    let address = shell.address();
    a_target_has_a_name(&address);
    a_command_returns_both_streams_and_its_status(shell).await;
    stdin_reaches_the_command(shell).await;
    a_command_past_its_deadline_is_timed_out(shell, &address).await;
    a_file_reads_back_as_written(shell, scratch_dir).await;
    a_missing_file_is_a_failed_command(shell, scratch_dir).await;
    a_streamed_command_relays_until_it_exits(shell).await;
    the_interactive_command_carries_the_command(shell);
}

fn a_target_has_a_name(address: &ShellAddress) {
    assert_ne!(address.transport, "", "the address names no transport");
    assert_ne!(address.name, "", "the address names no target");
}

async fn a_command_returns_both_streams_and_its_status(shell: &dyn RemoteShell) {
    let output = shell.exec(ExecRequest::new(BOTH_STREAMS_THEN_THREE)).await;
    assert_eq!(
        output,
        Ok(ExecOutput {
            status: 3,
            stdout: b"out".to_vec(),
            stderr: b"err".to_vec(),
        }),
        "a command that ran is an output, whatever its exit status"
    );
}

async fn stdin_reaches_the_command(shell: &dyn RemoteShell) {
    let output = shell
        .exec(ExecRequest::new(ECHO_STDIN).with_stdin(FILE_BYTES))
        .await;
    assert_eq!(
        output,
        Ok(ExecOutput::succeeded(FILE_BYTES)),
        "the request's stdin is the command's stdin, byte for byte"
    );
}

async fn a_command_past_its_deadline_is_timed_out(shell: &dyn RemoteShell, address: &ShellAddress) {
    let started = Instant::now();
    let outcome = shell
        .exec(ExecRequest::new(NEVER_FINISHES).within(SHORT_DEADLINE))
        .await;
    assert_eq!(
        outcome,
        Err(RemoteShellError::TimedOut {
            address: address.clone(),
            after: SHORT_DEADLINE,
        }),
        "a command still running at its deadline is timed out, naming the target"
    );
    assert!(
        started.elapsed() < STEP_BUDGET,
        "the deadline must bound the call, which took {:?}",
        started.elapsed()
    );
}

async fn a_file_reads_back_as_written(shell: &dyn RemoteShell, scratch_dir: &str) {
    let path = format!("{scratch_dir}/made by the contract/file.bin");
    let written = shell
        .write_file(&path, FILE_BYTES, FileMode::OwnerOnly)
        .await;
    assert_eq!(written, Ok(()), "a file is written, its directory created");
    let read = shell.read_file(&path).await;
    assert_eq!(
        read,
        Ok(FILE_BYTES.to_vec()),
        "a written file reads back byte for byte"
    );
}

async fn a_missing_file_is_a_failed_command(shell: &dyn RemoteShell, scratch_dir: &str) {
    let read = shell
        .read_file(&format!("{scratch_dir}/never written"))
        .await;
    assert!(
        matches!(&read, Err(RemoteShellError::Exited { status, stderr })
            if *status != 0 && stderr.contains("No such file")),
        "a file that is not there is the failed command that read it, got {read:?}"
    );
}

async fn a_streamed_command_relays_until_it_exits(shell: &dyn RemoteShell) {
    let mut streamed = shell
        .exec_streamed(ECHO_THEN_FOUR)
        .await
        .expect("a streamed command starts");

    let relayed = tokio::time::timeout(STEP_BUDGET, async {
        streamed.stdin.write_all(LINE).await.expect("stdin is open");
        streamed.stdin.flush().await.expect("stdin flushes");
        let mut echoed = vec![0; LINE.len()];
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
        (echoed, stderr, streamed.exit.wait().await)
    })
    .await
    .expect("a streamed command relays and exits within the step budget");

    assert_eq!(
        relayed,
        (LINE.to_vec(), b"done".to_vec(), Ok(4)),
        "a streamed command echoes while it runs, keeps stderr apart, and reports its exit"
    );
}

fn the_interactive_command_carries_the_command(shell: &dyn RemoteShell) {
    let interactive = shell.interactive("exec my-agent --resume");
    assert_eq!(
        interactive.argv.last().and_then(|last| last.to_str()),
        Some("exec my-agent --resume"),
        "the interactive argv ends with the command, as one argument"
    );
    assert!(
        interactive.argv.len() > 1,
        "the interactive argv starts with a local program, got {:?}",
        interactive.argv
    );
}

/// An [`InMemoryShell`] that answers the contract's commands as a POSIX target would. Must be
/// called inside a tokio runtime: the streamed command is played by a task.
pub fn an_in_memory_shell_for_the_contract() -> InMemoryShell {
    let shell = InMemoryShell::new("contract");
    shell
        .answers(
            BOTH_STREAMS_THEN_THREE,
            ExecOutput {
                status: 3,
                stdout: b"out".to_vec(),
                stderr: b"err".to_vec(),
            },
        )
        .answers_when(|request| {
            (request.command == ECHO_STDIN)
                .then(|| ExecOutput::succeeded(request.stdin.clone().unwrap_or_default()))
        })
        .hangs_on(NEVER_FINISHES);
    let stream = shell.streams(ECHO_THEN_FOUR);
    tokio::spawn(async move {
        let line = stream.stdin_exact(LINE.len()).await;
        stream.prints(line).await;
        let rest = stream.stdin_to_end().await;
        stream.prints(rest).await;
        stream.complains("done").await;
        stream.ends_with(4).await;
    });
    shell
}
