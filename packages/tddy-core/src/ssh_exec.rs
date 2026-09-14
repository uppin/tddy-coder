//! OpenSSH helpers for exec-catalog and worktree operations on an SSH target.
//!
//! The code-managing daemon shells out to `ssh -o BatchMode=yes`; there is no fallback to the
//! host filesystem when the child fails.

use std::path::Component;
use std::process::{Command, Output, Stdio};

/// Escape a string for a single-quoted POSIX shell argument.
pub fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// Build `ssh -o BatchMode=yes -- <host_alias> <remote_shell_command>`.
pub fn ssh_batch_command(host_alias: &str, remote_shell_command: &str) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.arg("-o")
        .arg("BatchMode=yes")
        .arg("--")
        .arg(host_alias)
        .arg(remote_shell_command)
        .stdin(Stdio::null());
    cmd
}

/// Run one remote shell command; non-zero exit is an error with stderr in the message.
pub fn run_ssh_batch(host_alias: &str, remote_shell_command: &str) -> Result<Output, String> {
    let output = ssh_batch_command(host_alias, remote_shell_command)
        .output()
        .map_err(|e| format!("ssh spawn failed: {e}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        Err(if detail.is_empty() {
            format!("ssh exited with status {}", output.status)
        } else {
            format!("ssh: {detail}")
        })
    }
}

/// Contain `arg_path` against an absolute worktree root on the SSH target (string paths only).
pub fn contain_remote_path(remote_root: &str, arg_path: &str) -> Result<String, String> {
    let root = remote_root.trim().trim_end_matches('/');
    if root.is_empty() {
        return Err("remote worktree root is empty".to_string());
    }

    let candidate = if arg_path.starts_with('/') {
        arg_path.to_string()
    } else {
        format!("{}/{}", root, arg_path.trim_start_matches('/'))
    };

    for component in std::path::Path::new(&candidate).components() {
        if component == Component::ParentDir {
            return Err(format!("path contains '..' component: {candidate}"));
        }
    }

    let normalized = candidate.replace("//", "/");
    if normalized != root && !normalized.starts_with(&format!("{}/", root)) {
        return Err(format!("resolved path escapes worktree: {normalized}"));
    }
    Ok(normalized)
}

/// Default checkout directory on an SSH target for the fixture alias `buildbox` and example URLs.
pub fn default_remote_repo_root(git_url: &str) -> String {
    let trimmed = git_url.trim();
    if trimmed.is_empty() || trimmed.contains("example.com") {
        "/home/dev/repo".to_string()
    } else if trimmed.starts_with("git@") {
        let path = trimmed.split(':').nth(1).unwrap_or("repo");
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or("repo")
            .trim_end_matches(".git");
        format!("/home/dev/{name}")
    } else {
        let name = trimmed
            .rsplit('/')
            .next()
            .unwrap_or("repo")
            .trim_end_matches(".git");
        format!("/home/dev/{name}")
    }
}
