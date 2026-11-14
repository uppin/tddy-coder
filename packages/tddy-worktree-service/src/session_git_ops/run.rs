//! The git commands behind each [`GitOp`], run in the target's worktree under its push identity.
//!
//! Every command runs with `GIT_TERMINAL_PROMPT=0`: a daemon has no terminal to answer a
//! credential prompt, so a push that needs one fails instead of waiting for it.

use std::path::Path;
use std::process::Command;

use super::{GitOp, GitOpError, GitOpOutcome, PushIdentity, ResetMode};

/// The environment variable a push's credential helper reads the account token from. The token
/// itself is only ever in this variable — never in an argument or a log line.
const PUSH_TOKEN_ENV: &str = "TDDY_PUSH_ACCOUNT_TOKEN";

/// A credential helper that answers with the token in [`PUSH_TOKEN_ENV`]. Handed to git per
/// command (`-c credential.helper=…` and the `GIT_CONFIG_*` environment), so it also answers a
/// `git credential fill` run by a push hook.
const CREDENTIAL_HELPER: &str =
    "!f() { echo username=x-access-token; echo \"password=$TDDY_PUSH_ACCOUNT_TOKEN\"; }; f";

/// Run `op` in `worktree` with `identity` applied to every git command it starts.
pub(super) fn run(
    worktree: &Path,
    identity: &PushIdentity,
    op: GitOp,
) -> Result<GitOpOutcome, GitOpError> {
    let git = Git {
        worktree,
        env: &identity.env,
        remote_name: identity.remote_name.as_deref(),
        account_token: identity.account_token.as_deref(),
    };
    match op {
        GitOp::Commit { message, paths } => commit(&git, &message, &paths),
        GitOp::Push { force_with_lease } => push(&git, force_with_lease),
        GitOp::Rebase { onto } => rebase(&git, checked_ref(&onto)?),
        GitOp::Stash => {
            let out = git.run(&["stash", "push", "--include-untracked"])?;
            Ok(done(format!("stashed: {out}")))
        }
        GitOp::Reset { mode, to } => {
            git.run(&["reset", reset_flag(mode), checked_ref(&to)?])?;
            Ok(done(format!(
                "reset {} to {to}",
                reset_flag(mode).trim_start_matches("--")
            )))
        }
        GitOp::Checkout { to } => {
            git.run(&["checkout", checked_ref(&to)?])?;
            Ok(done(format!("checked out {to}")))
        }
        GitOp::Clean => {
            git.run(&["clean", "-fd"])?;
            Ok(done("removed untracked files".to_string()))
        }
    }
}

/// One worktree and the environment its git commands run with.
struct Git<'a> {
    worktree: &'a Path,
    env: &'a [(String, String)],
    remote_name: Option<&'a str>,
    account_token: Option<&'a str>,
}

impl Git<'_> {
    /// Run `git args…` and return its trimmed stdout, or its stderr as the refusal.
    fn run(&self, args: &[&str]) -> Result<String, GitOpError> {
        self.run_inner(args, false)
    }

    /// Run a push, handing git the account token through a per-command credential helper when
    /// the identity carries one.
    fn run_push(&self, args: &[&str]) -> Result<String, GitOpError> {
        self.run_inner(args, true)
    }

    fn run_inner(&self, args: &[&str], credential: bool) -> Result<String, GitOpError> {
        let mut command = Command::new("git");
        if credential {
            if let Some(token) = self.account_token {
                command
                    .args(["-c", &format!("credential.helper={CREDENTIAL_HELPER}")])
                    .env(PUSH_TOKEN_ENV, token)
                    .env("GIT_CONFIG_COUNT", "1")
                    .env("GIT_CONFIG_KEY_0", "credential.helper")
                    .env("GIT_CONFIG_VALUE_0", CREDENTIAL_HELPER);
            }
        }
        let output = command
            .args(args)
            .current_dir(self.worktree)
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| GitOpError::Git(format!("git {}: {e}", args.join(" "))))?;
        if !output.status.success() {
            return Err(GitOpError::Git(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

fn done(summary: String) -> GitOpOutcome {
    GitOpOutcome::Done { summary }
}

/// A ref named by the caller, refused when it would parse as an option to git.
fn checked_ref(name: &str) -> Result<&str, GitOpError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err(GitOpError::Git(format!(
            "{name:?} is not a commit, branch or tag"
        )));
    }
    Ok(trimmed)
}

fn reset_flag(mode: ResetMode) -> &'static str {
    match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
    }
}

/// Stage `paths` (every change when empty) and commit them with `message`.
fn commit(git: &Git<'_>, message: &str, paths: &[String]) -> Result<GitOpOutcome, GitOpError> {
    if paths.is_empty() {
        git.run(&["add", "-A"])?;
        git.run(&["commit", "-m", message])?;
    } else {
        let mut add = vec!["add", "-A", "--"];
        add.extend(paths.iter().map(String::as_str));
        git.run(&add)?;
        let mut commit = vec!["commit", "-m", message, "--"];
        commit.extend(paths.iter().map(String::as_str));
        git.run(&commit)?;
    }
    let head = git.run(&["rev-parse", "--short", "HEAD"])?;
    Ok(done(format!("committed {head}")))
}

/// Push the worktree's branch to the remote its identity names (`origin` when it names none)
/// under its own name.
fn push(git: &Git<'_>, force_with_lease: bool) -> Result<GitOpOutcome, GitOpError> {
    let branch = git.run(&["symbolic-ref", "--short", "HEAD"])?;
    let remote = git.remote_name.unwrap_or("origin");
    let refspec = format!("HEAD:refs/heads/{branch}");
    if force_with_lease {
        git.run_push(&["push", "--force-with-lease", remote, &refspec])?;
    } else {
        git.run_push(&["push", remote, &refspec])?;
    }
    Ok(done(format!("pushed {branch} to {remote}")))
}

/// Rebase the worktree's branch onto `onto`. A conflict aborts the rebase, leaving the worktree
/// as it was, and reports the conflicting paths.
fn rebase(git: &Git<'_>, onto: &str) -> Result<GitOpOutcome, GitOpError> {
    let Err(error) = git.run(&["rebase", onto]) else {
        return Ok(done(format!("rebased onto {onto}")));
    };
    let paths = conflicted_paths(git)?;
    if paths.is_empty() {
        return Err(error);
    }
    git.run(&["rebase", "--abort"])?;
    Ok(GitOpOutcome::RebaseConflict { paths })
}

/// The paths git reports as unmerged in the worktree.
fn conflicted_paths(git: &Git<'_>) -> Result<Vec<String>, GitOpError> {
    let listing = git.run(&["diff", "--name-only", "--diff-filter=U"])?;
    Ok(listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}
