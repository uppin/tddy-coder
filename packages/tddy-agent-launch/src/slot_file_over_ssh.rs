//! Writing a slot file on the SSH target an agent's worktree lives on, with a deadline.
//!
//! An ssh session's hook slot files belong on the ssh target, not at that path on this host
//! (`spec_launch::launch_claude_cli`). The write is one `ssh … 'mkdir -p <dir> && cat > <file>'`
//! with the bytes on its stdin. A target that never answers must not hold the launch: the ssh is
//! started with a connect timeout and keep-alives, and the whole child is bounded by a wall-clock
//! deadline after which it is killed and the launch refused, naming the host.

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tddy_core::ssh_exec::shell_single_quote;

use crate::spec_launch::SpecLaunchError;

/// How long ssh may take to establish the connection (`ConnectTimeout`).
const SSH_CONNECT_TIMEOUT_SECS: u64 = 15;
/// How often ssh probes a connected target that has gone quiet (`ServerAliveInterval`).
const SSH_SERVER_ALIVE_INTERVAL_SECS: u64 = 5;
/// How many unanswered probes end the connection (`ServerAliveCountMax`).
const SSH_SERVER_ALIVE_COUNT_MAX: u64 = 3;
/// The wall-clock bound on one slot file write, connect included. Past it the ssh is killed.
pub const SLOT_FILE_OVER_SSH_DEADLINE: Duration = Duration::from_secs(60);
/// How often the child is polled while the write runs.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Write `bytes` to `path` on the SSH target `alias`, creating its directory, within
/// [`SLOT_FILE_OVER_SSH_DEADLINE`].
pub(crate) fn write_slot_file_over_ssh(
    alias: &str,
    path: &Path,
    bytes: &[u8],
) -> Result<(), SpecLaunchError> {
    write_over_ssh(
        Path::new("ssh"),
        alias,
        path,
        bytes,
        SLOT_FILE_OVER_SSH_DEADLINE,
    )
}

/// [`write_slot_file_over_ssh`] through the ssh client `ssh`, bounded by `deadline`.
fn write_over_ssh(
    ssh: &Path,
    alias: &str,
    path: &Path,
    bytes: &[u8],
    deadline: Duration,
) -> Result<(), SpecLaunchError> {
    let dir = path
        .parent()
        .map(|p| p.to_string_lossy())
        .unwrap_or_default();
    let remote = format!(
        "mkdir -p {} && cat > {}",
        shell_single_quote(&dir),
        shell_single_quote(&path.to_string_lossy())
    );
    run_over_ssh(ssh, alias, path, &remote, bytes, deadline).map(|_| ())
}

/// Runs the shell command `remote` on the ssh target `alias` through the ssh client `ssh`, with
/// `stdin` on its standard input, and returns what it printed. `path` is the file the command is
/// about, named by the errors. The ssh is bounded as every slot file write is: killed at `deadline`.
pub(crate) fn run_over_ssh(
    ssh: &Path,
    alias: &str,
    path: &Path,
    remote: &str,
    stdin: &[u8],
    deadline: Duration,
) -> Result<String, SpecLaunchError> {
    let failed = |message: String| SpecLaunchError::SlotFile {
        path: path.to_path_buf(),
        message: format!("over ssh to {alias}: {message}"),
    };
    let mut child = Command::new(ssh)
        .args(ssh_options())
        .arg("--")
        .arg(alias)
        .arg(remote)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| failed(e.to_string()))?;

    // Every pipe is served off this thread, so a target that reads nothing or writes a lot cannot
    // hold the deadline loop below.
    let mut stdin_pipe = child
        .stdin
        .take()
        .ok_or_else(|| failed("no stdin".to_string()))?;
    let payload = stdin.to_vec();
    let writer = std::thread::spawn(move || stdin_pipe.write_all(&payload));
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| failed("no stdout".to_string()))?;
    let printed = std::thread::spawn(move || {
        let mut text = String::new();
        stdout_pipe.read_to_string(&mut text).map(|_| text)
    });
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| failed("no stderr".to_string()))?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| failed(e.to_string()))? {
            break status;
        }
        if started.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SpecLaunchError::SlotFileOverSshTimedOut {
                host: alias.to_string(),
                path: path.to_path_buf(),
                after: deadline,
            });
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    let written = writer
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::other("the stdin writer panicked")));
    let printed = printed
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::other("the stdout reader panicked")));
    let stderr = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(failed(stderr.trim().to_string()));
    }
    written.map_err(|e| failed(e.to_string()))?;
    printed.map_err(|e| failed(format!("what it printed cannot be read: {e}")))
}

/// The ssh options every slot file write runs with: no prompt, and a connection that cannot hang.
fn ssh_options() -> Vec<String> {
    [
        "BatchMode=yes".to_string(),
        format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECS}"),
        format!("ServerAliveInterval={SSH_SERVER_ALIVE_INTERVAL_SECS}"),
        format!("ServerAliveCountMax={SSH_SERVER_ALIVE_COUNT_MAX}"),
    ]
    .into_iter()
    .flat_map(|option| ["-o".to_string(), option])
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    /// An executable `ssh` stand-in in `dir` running `body`.
    fn a_stub_ssh(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("ssh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_target_that_never_answers_is_killed_at_the_deadline_naming_the_host() {
        // Given an ssh that never returns
        let dir = tempfile::tempdir().unwrap();
        let ssh = a_stub_ssh(dir.path(), "exec sleep 30\n");
        let slot = dir.path().join("worktree/.claude/settings.local.json");

        // When a slot file is written through it with a one-second deadline
        let started = Instant::now();
        let refused = write_over_ssh(&ssh, "buildbox", &slot, b"{}", Duration::from_secs(1))
            .expect_err("a target that never answers must be refused");

        // Then the write gives up at the deadline, naming the host
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the deadline must bound the write, took {:?}",
            started.elapsed()
        );
        assert_eq!(
            refused.to_string(),
            format!(
                "slot file {} on ssh target buildbox: no answer within 1s; the ssh was killed",
                slot.display()
            )
        );
    }

    #[test]
    fn a_target_that_answers_gets_the_file_and_the_ssh_cannot_hang() {
        // Given an ssh that records its options and runs the remote command here
        let dir = tempfile::tempdir().unwrap();
        let argv = dir.path().join("argv");
        let ssh = a_stub_ssh(
            dir.path(),
            &format!(
                "echo \"$@\" > '{}'\nfor last; do :; done\nexec sh -c \"$last\"\n",
                argv.display()
            ),
        );
        let slot = dir.path().join("worktree/.claude/settings.local.json");

        // When a slot file is written through it
        write_over_ssh(
            &ssh,
            "buildbox",
            &slot,
            b"{\"hooks\":{}}",
            SLOT_FILE_OVER_SSH_DEADLINE,
        )
        .expect("the write succeeds");

        // Then the file holds the bytes, and the ssh ran with a bounded connection
        assert_eq!(std::fs::read(&slot).unwrap(), b"{\"hooks\":{}}");
        let options = std::fs::read_to_string(&argv).unwrap();
        for option in [
            "BatchMode=yes",
            "ConnectTimeout=15",
            "ServerAliveInterval=5",
            "ServerAliveCountMax=3",
            "-- buildbox",
        ] {
            assert!(options.contains(option), "{option} in {options:?}");
        }
    }
}
