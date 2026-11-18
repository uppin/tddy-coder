//! `CoderShell`: a Coder workspace as a transport of the remote-shell port, driven through a
//! stand-in `coder` executable that runs the workspace's command on this host. No deployment is
//! reached.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-coder-shell.md

#[path = "support/coder.rs"]
mod coder_support;
#[path = "support/mod.rs"]
mod ssh_support;

use std::ffi::OsString;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use pretty_assertions::assert_eq;
use tddy_remote_shell::contract::assert_behaves_like_a_remote_shell;
use tddy_remote_shell::{
    CoderShell, CoderWorkspaceRef, ExecOutput, ExecRequest, FileMode, InteractiveCommand,
    RemoteShell, RemoteShellError, ShellAddress,
};

use ssh_support::an_ssh_that_connects_through_its_proxy_and_runs_commands_here;

use coder_support::{
    a_coder_that_is_a_pty_unless_asked_for_stdio, a_coder_that_refuses_to_connect,
    a_coder_whose_workspaces_end_their_lines_with_carriage_returns,
    a_coder_whose_workspaces_merge_stderr_into_stdout,
    a_coder_whose_workspaces_never_see_the_end_of_their_input,
    a_coder_whose_workspaces_outlive_their_connection, a_coder_whose_workspaces_run_commands_here,
    a_coder_whose_workspaces_start_with_no_pager_variables, the_credentials, DEPLOYMENT_URL,
    SESSION_TOKEN,
};

/// What a one-shot call's arguments start with: never start the workspace, the workspace and its
/// agent, then the POSIX shell the command is handed to.
const ONE_SHOT_ON_ADAS_DEVBOX: &str = "ssh --disable-autostart ada/devbox.main -- sh -c ";

/// What the stand-in records for a call that was given the deployment and the token.
const GIVEN_THE_DEPLOYMENT_AND_TOKEN: &str =
    "CODER_URL=https://coder.example.com CODER_SESSION_TOKEN=adas-coder-session-token";

/// Bounds a step that would otherwise hang the suite on a transport that never relays. Far above
/// what the step needs.
const STEP_BUDGET: Duration = Duration::from_secs(10);

/// A settings file as a launch writes it: its last line, the closing brace, has no newline — the
/// line a pty holds back until its input ends.
const SETTINGS_FILE: &[u8] = b"{\n  \"hooks\": {\"hook_token\": \"tok-7f3a9c\"}\n}";

/// Not text: a NUL, a byte that is no UTF-8, the pty's end-of-file and interrupt characters, both
/// line endings, and no trailing newline.
const CONTROL_BYTES: &[u8] = &[b'a', 0, 0xff, 0x04, 0x03, b'\r', b'\n', b'z'];

fn adas_devbox() -> CoderWorkspaceRef {
    CoderWorkspaceRef::new("ada", "devbox").on_agent("main")
}

fn the_daemons_own_coder_variables() -> (Option<OsString>, Option<OsString>) {
    (
        std::env::var_os(CoderShell::URL_VARIABLE),
        std::env::var_os(CoderShell::SESSION_TOKEN_VARIABLE),
    )
}

#[tokio::test]
async fn the_coder_shell_behaves_like_a_remote_shell() {
    // Given a coder shell whose workspace runs commands
    let coder = a_coder_whose_workspaces_run_commands_here();
    let ssh = an_ssh_that_connects_through_its_proxy_and_runs_commands_here();
    let shell = coder.shell_on_through_ssh(adas_devbox(), &ssh);

    // When / Then it is held to the contract every transport is held to
    assert_behaves_like_a_remote_shell(&shell, &coder.scratch_dir().to_string_lossy()).await;
}

#[tokio::test]
async fn a_command_runs_on_the_named_workspace() {
    // Given a coder whose workspaces run commands
    let coder = a_coder_whose_workspaces_run_commands_here();
    let shell = coder.shell_on(adas_devbox());

    // When a command that prints on both streams and exits non-zero is run
    let output = shell
        .exec(ExecRequest::new("printf out; printf err >&2; exit 3"))
        .await;

    // Then its output and status come back as an output
    assert_eq!(
        output,
        Ok(ExecOutput {
            status: 3,
            stdout: b"out".to_vec(),
            stderr: b"err".to_vec(),
        })
    );
    // And it ran once, on that workspace's agent, without starting the workspace
    let calls = coder.calls();
    assert_eq!(calls.len(), 1, "got {calls:?}");
    assert!(
        calls[0].starts_with(ONE_SHOT_ON_ADAS_DEVBOX),
        "expected a call starting `{ONE_SHOT_ON_ADAS_DEVBOX}`, got {calls:?}"
    );
}

#[tokio::test]
async fn the_deployment_url_and_token_reach_only_the_coder_process() {
    // Given a coder whose workspaces run commands, and this process's own environment
    let coder = a_coder_whose_workspaces_run_commands_here();
    let ssh = an_ssh_that_connects_through_its_proxy_and_runs_commands_here();
    let shell = coder.shell_on_through_ssh(adas_devbox(), &ssh);
    let before = the_daemons_own_coder_variables();

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

    // Then each coder process was given the deployment and the token in its environment
    assert_eq!(streamed_exit, Ok(0));
    assert_eq!(
        coder.environments(),
        vec![
            GIVEN_THE_DEPLOYMENT_AND_TOKEN.to_string(),
            GIVEN_THE_DEPLOYMENT_AND_TOKEN.to_string(),
        ]
    );
    // And the interactive command carries them as its own environment, for the one child a PTY
    // starts
    assert_eq!(
        interactive.env,
        vec![
            (OsString::from("CODER_URL"), OsString::from(DEPLOYMENT_URL)),
            (
                OsString::from("CODER_SESSION_TOKEN"),
                OsString::from(SESSION_TOKEN)
            ),
        ]
    );
    // And neither travelled on a command line, where any process on the host could read it
    let command_lines_naming_the_token: Vec<String> = coder
        .calls()
        .into_iter()
        .chain(ssh.calls())
        .chain(
            interactive
                .argv
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned()),
        )
        .filter(|line| line.contains(SESSION_TOKEN) || line.contains(DEPLOYMENT_URL))
        .collect();
    assert_eq!(command_lines_naming_the_token, Vec::<String>::new());
    // And this process's own environment is as it was
    assert_eq!(the_daemons_own_coder_variables(), before);
}

#[test]
fn the_interactive_command_opens_a_terminal_on_the_workspace() {
    // Given a shell on a workspace's agent, through the coder on PATH
    let shell = CoderShell::new(the_credentials(), adas_devbox());

    // When the interactive command for an agent is asked for
    let interactive = shell.interactive("cd '/home/dev/repo' && exec claude --resume");

    // Then it is coder's ssh, the workspace never started, and the command as one argument after
    // `--` — an argv a local PTY can run, given the deployment and the token. No `-t`: coder
    // v2.33.5 has no such flag, and gives the command a terminal of its own when its stdin is one
    let argv: Vec<OsString> = [
        "coder",
        "ssh",
        "--disable-autostart",
        "ada/devbox.main",
        "--",
        "cd '/home/dev/repo' && exec claude --resume",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    assert_eq!(
        interactive,
        InteractiveCommand {
            argv,
            env: vec![
                (OsString::from("CODER_URL"), OsString::from(DEPLOYMENT_URL)),
                (
                    OsString::from("CODER_SESSION_TOKEN"),
                    OsString::from(SESSION_TOKEN)
                ),
            ],
        }
    );
}

#[tokio::test]
async fn a_command_through_a_merging_pty_keeps_its_status_and_merged_words() {
    // Given a coder whose pty merges the workspace's stderr into its stdout
    let coder = a_coder_whose_workspaces_merge_stderr_into_stdout();
    let shell = coder.shell_on(adas_devbox());

    // When a command that prints on both streams and exits non-zero is run
    let output = shell
        .exec(ExecRequest::new("printf out; printf err >&2; exit 3"))
        .await;

    // Then the command ran: its status is its own, its words arrive merged, and the start
    // marker that proves it ran is not part of the output
    assert_eq!(
        output,
        Ok(ExecOutput {
            status: 3,
            stdout: b"outerr".to_vec(),
            stderr: Vec::new(),
        })
    );
}

#[tokio::test]
async fn a_value_through_a_merging_pty_is_the_value_alone() {
    // Given a coder whose pty merges the workspace's stderr into its stdout
    let coder = a_coder_whose_workspaces_merge_stderr_into_stdout();
    let shell = coder.shell_on(adas_devbox());

    // When a value-returning command is run, as the target's home is read
    let output = shell
        .exec(ExecRequest::new("printf '%s' /home/coder"))
        .await;

    // Then the value is clean: no start marker and no line ending the pty added
    assert_eq!(output, Ok(ExecOutput::succeeded("/home/coder")));
}

#[tokio::test]
async fn a_failed_command_through_a_merging_pty_is_not_unreachable() {
    // Given a coder whose pty merges the workspace's stderr into its stdout
    let coder = a_coder_whose_workspaces_merge_stderr_into_stdout();
    let shell = coder.shell_on(adas_devbox());

    // When a command runs and exits 1, as `test -d` does for a path that is not there
    let output = shell
        .exec(ExecRequest::new("printf 'no such directory' >&2; exit 1"))
        .await;

    // Then it is a failed command with its words, not an unreachable workspace
    assert_eq!(
        output,
        Ok(ExecOutput {
            status: 1,
            stdout: b"no such directory".to_vec(),
            stderr: Vec::new(),
        })
    );
}

#[tokio::test]
async fn a_stopped_workspace_is_unreachable_not_a_failed_command() {
    // Given a coder that will not connect to a workspace that is not running, and says so
    let coder = a_coder_that_refuses_to_connect("workspace must be started");
    let shell = coder.shell_on(adas_devbox());

    // When a command is run through it
    let outcome = shell.exec(ExecRequest::new("true")).await;

    // Then the workspace is unreachable, with what coder said — not an output with status 1
    assert_eq!(
        outcome,
        Err(RemoteShellError::Unreachable {
            address: ShellAddress::new("coder", "ada/devbox.main"),
            detail: "workspace must be started".to_string(),
        })
    );
    // And coder was told not to start it
    assert_eq!(
        coder
            .calls()
            .iter()
            .map(|call| call.starts_with(ONE_SHOT_ON_ADAS_DEVBOX))
            .collect::<Vec<_>>(),
        vec![true]
    );
}

#[tokio::test]
async fn a_missing_coder_cli_is_a_spawn_failure_that_names_it() {
    // Given a shell whose coder CLI does not exist
    let shell = CoderShell::through("/no/such/dir/coder", the_credentials(), adas_devbox());

    // When a command is run through it
    let outcome = shell.exec(ExecRequest::new("true")).await;

    // Then the failure is the local spawn's, naming the CLI
    assert!(
        matches!(&outcome, Err(RemoteShellError::Spawn(message)) if message.contains("/no/such/dir/coder")),
        "expected a spawn failure naming /no/such/dir/coder, got {outcome:?}"
    );
}

#[tokio::test]
async fn a_file_written_through_a_pty_that_never_ends_its_input_reads_back_as_written() {
    // Given a coder whose workspace's pty never turns the end of its input into an EOF
    let coder = a_coder_whose_workspaces_never_see_the_end_of_their_input();
    let shell = coder.shell_on(adas_devbox());
    let path = format!(
        "{}/.claude/settings.local.json",
        coder.scratch_dir().to_string_lossy()
    );

    // When a settings file whose last line has no newline is written, then read back
    let written = tokio::time::timeout(
        STEP_BUDGET,
        shell.write_file(&path, SETTINGS_FILE, FileMode::OwnerOnly),
    )
    .await
    .expect("the write finishes within the step budget, not at the shell's deadline");
    let read = shell.read_file(&path).await;

    // Then the file is whole, closing brace and all
    assert_eq!(written, Ok(()));
    assert_eq!(read, Ok(SETTINGS_FILE.to_vec()));
    // And its bytes never travelled on a command line, where any process on the host could read
    // the hook token they carry
    let command_lines_naming_the_token: Vec<String> = coder
        .calls()
        .into_iter()
        .filter(|line| line.contains("tok-7f3a9c"))
        .collect();
    assert_eq!(command_lines_naming_the_token, Vec::<String>::new());
}

#[tokio::test]
async fn stdin_through_a_pty_that_never_ends_its_input_reaches_the_command_byte_for_byte() {
    // Given a coder whose workspace's pty never turns the end of its input into an EOF
    let coder = a_coder_whose_workspaces_never_see_the_end_of_their_input();
    let shell = coder.shell_on(adas_devbox());

    // When a command that copies its stdin is given bytes the pty would otherwise act on
    let output = tokio::time::timeout(
        STEP_BUDGET,
        shell.exec(ExecRequest::new("cat").with_stdin(CONTROL_BYTES)),
    )
    .await
    .expect("the command finishes within the step budget, not at the shell's deadline");

    // Then the command read them exactly, and then the end of its input
    assert_eq!(output, Ok(ExecOutput::succeeded(CONTROL_BYTES)));
}

#[tokio::test]
async fn a_command_given_no_stdin_reads_an_empty_one_through_a_pty_that_never_ends_its_input() {
    // Given a coder whose workspace's pty never turns the end of its input into an EOF
    let coder = a_coder_whose_workspaces_never_see_the_end_of_their_input();
    let shell = coder.shell_on(adas_devbox());

    // When a command that reads its stdin is run with none
    let output = tokio::time::timeout(STEP_BUDGET, shell.exec(ExecRequest::new("cat")))
        .await
        .expect("the command finishes within the step budget, not at the shell's deadline");

    // Then it read an empty one
    assert_eq!(output, Ok(ExecOutput::succeeded("")));
}

#[tokio::test]
async fn a_file_read_through_a_pty_that_ends_lines_with_carriage_returns_reads_as_it_is() {
    // Given a coder whose workspace's pty ends every line it writes with a carriage return, and a
    // settings file on the workspace
    let coder = a_coder_whose_workspaces_end_their_lines_with_carriage_returns();
    let shell = coder.shell_on(adas_devbox());
    let dir = coder.scratch_dir().join(".claude");
    std::fs::create_dir_all(&dir).expect("the workspace's directory is made");
    let path = dir.join("settings.local.json");
    std::fs::write(&path, SETTINGS_FILE).expect("the settings file is written");

    // When it is read through the shell
    let read = tokio::time::timeout(STEP_BUDGET, shell.read_file(&path.to_string_lossy()))
        .await
        .expect("the read finishes within the step budget");

    // Then its bytes are the file's, with no carriage return the pty would have added
    assert_eq!(read, Ok(SETTINGS_FILE.to_vec()));
}

#[tokio::test]
async fn lines_a_command_prints_through_a_pty_that_ends_lines_with_carriage_returns_end_as_printed()
{
    // Given a coder whose workspace's pty ends every line it writes with a carriage return
    let coder = a_coder_whose_workspaces_end_their_lines_with_carriage_returns();
    let shell = coder.shell_on(adas_devbox());

    // When a command prints two lines, as `git worktree list` does
    let output = tokio::time::timeout(
        STEP_BUDGET,
        shell.exec(ExecRequest::new("printf 'one\\ntwo\\n'")),
    )
    .await
    .expect("the command finishes within the step budget");

    // Then each line ends as the command ended it
    assert_eq!(output, Ok(ExecOutput::succeeded("one\ntwo\n")));
}

/// The first process id in `pid_file`, once the command has written it.
async fn the_pid_written_to(pid_file: &std::path::Path) -> String {
    let started = Instant::now();
    loop {
        let written = std::fs::read_to_string(pid_file).unwrap_or_default();
        if !written.is_empty() || started.elapsed() > STEP_BUDGET {
            return written;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn the_process_is_running(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill runs")
        .success()
}

/// Whether `pid` is gone within `budget`, polled.
async fn the_process_is_gone_within(pid: &str, budget: Duration) -> bool {
    let started = Instant::now();
    while the_process_is_running(pid) && started.elapsed() < budget {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    !the_process_is_running(pid)
}

#[tokio::test]
async fn a_one_shot_past_its_deadline_is_stopped_on_the_workspace_too() {
    // Given a coder whose workspaces outlive their connection, and a command that records its
    // pid and then outlives its deadline
    let coder = a_coder_whose_workspaces_outlive_their_connection();
    let shell = coder.shell_on(adas_devbox());
    std::fs::create_dir_all(coder.scratch_dir()).expect("the workspace's directory is made");
    let pid_file = coder.scratch_dir().join("pid");
    let command = format!(
        "printf '%s' $$ > '{}'; exec sleep 30",
        pid_file.to_string_lossy()
    );

    // When it is run with a short deadline
    let outcome = shell
        .exec(ExecRequest::new(command).within(Duration::from_secs(2)))
        .await;
    let pid = the_pid_written_to(&pid_file).await;
    let gone = the_process_is_gone_within(&pid, Duration::from_secs(8)).await;
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid])
        .status();

    // Then the call timed out
    assert_eq!(
        outcome,
        Err(RemoteShellError::TimedOut {
            address: ShellAddress::new("coder", "ada/devbox.main"),
            after: Duration::from_secs(2),
        })
    );
    // And the command it started on the workspace is gone too, not left running
    assert_eq!(
        gone, true,
        "process {pid} is still running on the workspace"
    );
}

#[tokio::test]
async fn a_one_shot_never_starts_a_pager() {
    // Given a coder whose workspaces start with no pager variables set
    let coder = a_coder_whose_workspaces_start_with_no_pager_variables();
    let shell = coder.shell_on(adas_devbox());

    // When a one-shot prints the variables a pager or a git prompt would be chosen by
    let output = shell
        .exec(ExecRequest::new(
            "printf '%s|%s|%s' \"$PAGER\" \"$GIT_PAGER\" \"$GIT_TERMINAL_PROMPT\"",
        ))
        .await;

    // Then the pager is `cat` and git never prompts
    assert_eq!(output, Ok(ExecOutput::succeeded("cat|cat|0")));
}

#[tokio::test]
async fn a_streamed_command_keeps_its_bytes_and_its_end_through_a_pty_workspace() {
    // Given a coder that is a pty to a plain `coder ssh <workspace> -- <command>`: its input never
    // ends and stderr merges into stdout
    let coder = a_coder_that_is_a_pty_unless_asked_for_stdio();
    let ssh = an_ssh_that_connects_through_its_proxy_and_runs_commands_here();
    let shell = coder.shell_on_through_ssh(adas_devbox(), &ssh);

    // When it is held to the contract's streamed behaviour: echo, EOF, a separate stderr, exit 4
    // (the whole contract is not run: its one-shot cases are the pty's, covered above)
    let mut streamed = shell
        .exec_streamed("cat; printf done >&2; exit 4")
        .await
        .expect("a streamed command starts");
    let relayed = tokio::time::timeout(STEP_BUDGET, async {
        streamed
            .stdin
            .write_all(b"ping\n")
            .await
            .expect("stdin is open");
        streamed.stdin.flush().await.expect("stdin flushes");
        let mut echoed = vec![0; 5];
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

    // Then it echoed while it ran, kept stderr apart, and reported its exit
    assert_eq!(relayed, (b"ping\n".to_vec(), b"done".to_vec(), Ok(4)));
}

#[tokio::test]
async fn every_byte_value_round_trips_through_a_streamed_cat_over_a_pty_workspace() {
    // Given the same pty-like coder, and all 256 byte values
    let coder = a_coder_that_is_a_pty_unless_asked_for_stdio();
    let ssh = an_ssh_that_connects_through_its_proxy_and_runs_commands_here();
    let shell = coder.shell_on_through_ssh(adas_devbox(), &ssh);
    let every_byte: Vec<u8> = (0..=255).collect();

    // When they are written to a streamed `cat`, its input is ended, and its output is read
    let mut streamed = shell
        .exec_streamed("cat")
        .await
        .expect("a streamed command starts");
    let relayed = tokio::time::timeout(STEP_BUDGET, async {
        streamed
            .stdin
            .write_all(&every_byte)
            .await
            .expect("stdin is open");
        streamed.stdin.shutdown().await.expect("stdin closes");
        let mut stdout = Vec::new();
        streamed
            .stdout
            .read_to_end(&mut stdout)
            .await
            .expect("stdout ends with the command");
        (stdout, streamed.exit.wait().await)
    })
    .await
    .expect("cat ends within the step budget: its end of input arrived");

    // Then every byte came back as sent, and cat ended on its EOF
    assert_eq!(relayed, (every_byte, Ok(0)));
}

#[tokio::test]
async fn a_streamed_command_reaches_the_workspace_through_coder_ssh_stdio() {
    // Given a coder and an ssh that drives it as a ProxyCommand
    let coder = a_coder_that_is_a_pty_unless_asked_for_stdio();
    let ssh = an_ssh_that_connects_through_its_proxy_and_runs_commands_here();
    let shell = coder.shell_on_through_ssh(adas_devbox(), &ssh);

    // When a streamed command is run to its end
    let streamed = shell
        .exec_streamed("true")
        .await
        .expect("the streamed command starts");
    let exit = tokio::time::timeout(STEP_BUDGET, streamed.exit.wait())
        .await
        .expect("the streamed command exits within the step budget");

    // Then it ran, and coder was called once, as a stdio proxy for that workspace's agent that
    // never starts the workspace
    assert_eq!(exit, Ok(0));
    assert_eq!(
        coder.calls(),
        vec!["ssh --stdio --disable-autostart ada/devbox.main".to_string()]
    );
    // And the proxy was given the deployment and the token in its environment
    assert_eq!(
        coder.environments(),
        vec![GIVEN_THE_DEPLOYMENT_AND_TOKEN.to_string()]
    );
    // And neither is on any command line: not ssh's, not the proxy command's
    let command_lines_naming_the_token: Vec<String> = coder
        .calls()
        .into_iter()
        .chain(ssh.calls())
        .filter(|line| line.contains(SESSION_TOKEN) || line.contains(DEPLOYMENT_URL))
        .collect();
    assert_eq!(command_lines_naming_the_token, Vec::<String>::new());
}
