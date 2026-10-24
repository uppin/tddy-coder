//! Running one clone: `git clone --progress` as the target account, into a temporary sibling of the
//! destination that is renamed into place only when git succeeds. Git's stderr is read as it is
//! written and parsed into [`GitProgress`]; the clone can be stopped through its [`CloneHandle`].
//!
//! A destination therefore either does not exist or holds a complete clone — never a half-written
//! one that a later start's "already present" check would mistake for a project.

use std::path::{Path, PathBuf};

use crate::git_progress::GitProgress;
#[cfg(unix)]
use crate::git_progress::{parse, ProgressThrottle};

/// How long a stopped clone has to end on SIGTERM before its process group is killed.
#[cfg(unix)]
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// How many of git's last non-progress lines an error carries.
#[cfg(unix)]
const ERROR_TAIL_LINES: usize = 8;

/// Stops a running clone: its git process and everything git started (ssh, index-pack), as one
/// process group.
#[derive(Debug, Clone)]
pub struct CloneHandle {
    pid: u32,
}

impl CloneHandle {
    /// A handle on the clone whose process-group leader is `pid`.
    pub fn for_process_group(pid: u32) -> CloneHandle {
        CloneHandle { pid }
    }

    /// The clone's git process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// End the clone: SIGTERM to its process group, SIGKILL if anything is still there after a
    /// short grace. Its temporary directory is removed by the clone that was running.
    #[cfg(unix)]
    pub fn stop(&self) -> std::io::Result<()> {
        let group = self.pid as libc::pid_t;
        signal_group(group, libc::SIGTERM)?;
        let deadline = std::time::Instant::now() + STOP_GRACE;
        while std::time::Instant::now() < deadline {
            if !group_exists(group) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        signal_group(group, libc::SIGKILL)
    }

    #[cfg(not(unix))]
    pub fn stop(&self) -> std::io::Result<()> {
        Err(std::io::Error::other("a clone can only be stopped on Unix"))
    }
}

/// Signal a process group; a group that is already gone is not an error.
#[cfg(unix)]
fn signal_group(group: libc::pid_t, signal: libc::c_int) -> std::io::Result<()> {
    // SAFETY: killpg takes plain integers and touches no memory of this process.
    if unsafe { libc::killpg(group, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(()),
        _ => Err(error),
    }
}

#[cfg(unix)]
fn group_exists(group: libc::pid_t) -> bool {
    // SAFETY: signal 0 only probes for the group's existence; no memory is touched.
    unsafe { libc::killpg(group, 0) == 0 }
}

/// Clone `git_url` into `destination` as `os_user`, with `env` on the git child. Calls `on_started`
/// once git is running and `on_progress` for each throttled progress line; returns when git does.
/// Skips (Ok) when `destination` already exists.
#[cfg(unix)]
pub fn clone_with_progress(
    os_user: &str,
    git_url: &str,
    destination: &Path,
    env: &[(&str, &str)],
    on_started: impl FnOnce(CloneHandle),
    on_progress: impl FnMut(GitProgress),
) -> anyhow::Result<()> {
    // The tree is already present: nothing to refuse, drop privileges for, or clone.
    if destination.exists() {
        log::info!(
            "clone_with_progress: destination already exists, skipping (dest={})",
            destination.display()
        );
        return Ok(());
    }
    log::info!(
        "clone_with_progress: will run git clone as os_user={} dest={}",
        os_user,
        destination.display()
    );
    let partial = partial_sibling(destination)?;
    let outcome = run_into(os_user, git_url, &partial, env, on_started, on_progress);
    match outcome {
        Ok(()) => std::fs::rename(&partial, destination).map_err(|e| {
            remove_partial(&partial);
            anyhow::anyhow!(
                "git clone: moving {} to {}: {}",
                partial.display(),
                destination.display(),
                e
            )
        }),
        Err(error) => {
            remove_partial(&partial);
            Err(error)
        }
    }
}

#[cfg(not(unix))]
pub fn clone_with_progress(
    _os_user: &str,
    _git_url: &str,
    _destination: &Path,
    _env: &[(&str, &str)],
    _on_started: impl FnOnce(CloneHandle),
    _on_progress: impl FnMut(GitProgress),
) -> anyhow::Result<()> {
    anyhow::bail!("clone_with_progress is only supported on Unix")
}

/// `<destination>.partial-<uuid>`, beside the destination so the final rename stays on one filesystem.
fn partial_sibling(destination: &Path) -> anyhow::Result<PathBuf> {
    let name = destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("destination {} has no file name", destination.display()))?;
    let mut partial = name.to_os_string();
    partial.push(format!(".partial-{}", uuid::Uuid::new_v4()));
    Ok(destination.with_file_name(partial))
}

#[cfg(unix)]
fn remove_partial(partial: &Path) {
    if let Err(e) = std::fs::remove_dir_all(partial) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!(
                "clone_with_progress: could not remove {}: {}",
                partial.display(),
                e
            );
        }
    }
}

/// Run `git clone --progress` into `partial` and read its stderr until it closes.
#[cfg(unix)]
fn run_into(
    os_user: &str,
    git_url: &str,
    partial: &Path,
    env: &[(&str, &str)],
    on_started: impl FnOnce(CloneHandle),
    on_progress: impl FnMut(GitProgress),
) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use tddy_daemon_kernel::pre_exec_stages::StagedCommand;
    use tddy_daemon_kernel::privilege_drop::{plan_privilege_change, resolve_target_account};

    let target = resolve_target_account(os_user).map_err(anyhow::Error::msg)?;
    let home = target.home.clone();
    // Refused here, before any fork, when this process cannot become `os_user`.
    let change = plan_privilege_change(target)?;

    let mut command = std::process::Command::new("git");
    command
        .args(["clone", "--progress"])
        .arg(git_url)
        .arg(partial)
        .env("HOME", &home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        // Its own process group, so stopping the clone reaches ssh and index-pack as well.
        .process_group(0);
    // Transport-shim env vars (e.g. GIT_SSH_COMMAND + TDDY_* for a facilitator clone) travel in the
    // child's own environment, which the privilege drop does not touch.
    for (key, value) in env {
        command.env(key, value);
    }

    let mut staged = StagedCommand::new(command, &format!("clone {}", partial.display()), change)
        .spawn()
        .map_err(|e| anyhow::anyhow!("git clone: {}", e))?;
    on_started(CloneHandle::for_process_group(staged.child.id()));

    let stderr = staged
        .child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("git clone: stderr was not piped"))?;
    let tail = report_progress(stderr, on_progress);

    let status = staged
        .child
        .wait()
        .map_err(|e| anyhow::anyhow!("git clone: {}", e))?;
    let _ = staged.stages.join();
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("git clone failed ({status}): {}", tail.join("\n"))
}

/// Read git's stderr to its end, handing each throttled progress line to `on_progress`. Returns
/// git's last [`ERROR_TAIL_LINES`] lines that were not progress — what a failure is explained by.
#[cfg(unix)]
fn report_progress(
    stderr: impl std::io::Read,
    mut on_progress: impl FnMut(GitProgress),
) -> Vec<String> {
    let mut throttle = ProgressThrottle::new();
    let mut tail = std::collections::VecDeque::new();
    for_each_stderr_line(stderr, |line| match parse(line) {
        Some(progress) => {
            if throttle.admit(&progress, std::time::Instant::now()) {
                on_progress(progress);
            }
        }
        None => {
            tail.push_back(line.to_string());
            if tail.len() > ERROR_TAIL_LINES {
                tail.pop_front();
            }
        }
    });
    tail.into_iter().collect()
}

/// Call `on_line` with each non-empty line of `reader`, where git ends a line with `\r` while a
/// stage redraws and `\n` when it ends. Lines are handed over as they complete, not at end-of-file.
#[cfg(unix)]
fn for_each_stderr_line(mut reader: impl std::io::Read, mut on_line: impl FnMut(&str)) {
    let mut chunk = [0u8; 4096];
    let mut line: Vec<u8> = Vec::new();
    let mut emit = |line: &mut Vec<u8>| {
        let text = String::from_utf8_lossy(line);
        let text = text.trim();
        if !text.is_empty() {
            on_line(text);
        }
        line.clear();
    };
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                for &byte in &chunk[..n] {
                    if byte == b'\r' || byte == b'\n' {
                        emit(&mut line);
                    } else {
                        line.push(byte);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    emit(&mut line);
}
