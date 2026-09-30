//! The one way this crate runs git.
//!
//! Null stdin, `GIT_OPTIONAL_LOCKS=0` (a status-like read must not take the index lock a caller's
//! own `git` may be holding), a fixed committer identity (the repository's own `user.*` may be
//! unset, and a subagent's commit must not be attributed to the developer), and the command line
//! carried in the error so a failure names what was run.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::worktree::WorktreeError;

pub(crate) const COMMITTER_NAME: &str = "tddy-subagent";
pub(crate) const COMMITTER_EMAIL: &str = "tddy-subagent@tddy.invalid";

/// What a finished git process left: its exit status and both streams, undecoded.
pub(crate) struct RawOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Run `git <args>` in `dir`, optionally feeding `stdin`, and return the process's output whether
/// or not it succeeded — for the callers that read a non-zero exit (`git apply --3way` exits 1 on a
/// conflict it has written markers for).
pub(crate) async fn git_raw<I, S>(
    dir: &Path,
    args: I,
    env: &[(&str, &OsStr)],
    stdin: Option<&[u8]>,
) -> Result<(Vec<String>, RawOutput), WorktreeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<S> = args.into_iter().collect();
    let shown: Vec<String> = args
        .iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned())
        .collect();
    let io = |source| WorktreeError::Io {
        context: format!("running git {} in {}", shown.join(" "), dir.display()),
        source,
    };
    let mut command = Command::new("git");
    command
        .args(&args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_AUTHOR_NAME", COMMITTER_NAME)
        .env("GIT_AUTHOR_EMAIL", COMMITTER_EMAIL)
        .env("GIT_COMMITTER_NAME", COMMITTER_NAME)
        .env("GIT_COMMITTER_EMAIL", COMMITTER_EMAIL)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().map_err(io)?;
    let mut child_stdin = child.stdin.take();
    let feed = async {
        if let (Some(pipe), Some(bytes)) = (child_stdin.as_mut(), stdin) {
            pipe.write_all(bytes).await?;
        }
        // Closing the pipe is what tells git the input is complete.
        drop(child_stdin.take());
        Ok::<(), std::io::Error>(())
    };
    let (fed, output) = tokio::join!(feed, child.wait_with_output());
    let output = output.map_err(io)?;
    // A git that exits before reading its input closes the pipe; its exit status says why.
    if let Err(source) = fed {
        if source.kind() != std::io::ErrorKind::BrokenPipe {
            return Err(io(source));
        }
    }
    Ok((
        shown,
        RawOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
    ))
}

/// Run `git <args>` in `dir` and return its stdout; a non-zero exit is an error naming the command.
pub(crate) async fn git<I, S>(
    dir: &Path,
    args: I,
    env: &[(&str, &OsStr)],
    stdin: Option<&[u8]>,
) -> Result<String, WorktreeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let (shown, output) = git_raw(dir, args, env, stdin).await?;
    if !output.success {
        return Err(WorktreeError::Git {
            args: shown,
            stderr: output.stderr,
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
