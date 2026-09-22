//! Remotes: `GIT_SSH_COMMAND`, fetch and push, default-remote detection, integration-base
//! resolution, and remote-branch listing.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::refs::{validate_chain_pr_integration_base_ref, validate_integration_base_ref};

/// Last-resort remote-tracking ref used for integration worktrees when no remote can be detected
/// from the main worktree's upstream and the project registry does not specify `remote_name` or
/// `main_branch_ref`.
///
/// `origin/master` preserves the historical hardcoded contract for the worst case (no upstream, no
/// project config). Every other path resolves the remote name from the main worktree's upstream
/// tracking branch or the project's stored `remote_name` before falling back here.
pub const FALLBACK_DEFAULT_INTEGRATION_BASE_REF: &str = "origin/master";

/// Legacy alias for [`FALLBACK_DEFAULT_INTEGRATION_BASE_REF`]; kept for callers that documented the
/// old name.
#[deprecated(note = "use FALLBACK_DEFAULT_INTEGRATION_BASE_REF")]
pub const DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF: &str = FALLBACK_DEFAULT_INTEGRATION_BASE_REF;

/// Optional `GIT_SSH_COMMAND` applied to git subprocesses that contact a remote. Set once at daemon
/// startup from `DaemonConfig::git.ssh_command`. `None` (the default) inherits the ambient
/// environment, preserving prior behavior for the CLI and tests.
static GIT_SSH_COMMAND: OnceLock<Option<String>> = OnceLock::new();

/// Configure the `GIT_SSH_COMMAND` used for git operations that reach a remote (fetch). Intended to
/// be called once during daemon startup; subsequent calls are ignored. Passing `None` is a no-op
/// that leaves the ambient environment in effect.
pub fn set_git_ssh_command(cmd: Option<String>) {
    let _ = GIT_SSH_COMMAND.set(cmd);
}

/// Builds a `git` command for an operation that contacts a remote (fetch). Applies the configured
/// `GIT_SSH_COMMAND` (if any) and hardens the process against interactive hangs: stdin is closed and
/// `GIT_TERMINAL_PROMPT=0` is set, so a missing key/passphrase or credential prompt fails fast
/// instead of blocking forever — a headless daemon has no TTY to answer such a prompt.
fn git_remote_command(repo_root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    if let Some(Some(ssh)) = GIT_SSH_COMMAND.get() {
        cmd.env("GIT_SSH_COMMAND", ssh);
    }
    cmd
}

/// Fetches a remote-tracking ref for chain PRs (multi-segment `<remote>/<path>` allowed).
pub fn fetch_chain_pr_integration_base(
    repo_root: &Path,
    integration_base_ref: &str,
) -> Result<(), String> {
    validate_chain_pr_integration_base_ref(integration_base_ref)?;
    let (remote, branch_path) = integration_base_ref
        .split_once('/')
        .expect("validate_chain_pr_integration_base_ref ensures <remote>/<path> form");
    log::info!(
        "fetch_chain_pr_integration_base: repo={} integration_base_ref={}",
        repo_root.display(),
        integration_base_ref
    );
    let output = git_remote_command(repo_root)
        .args(["fetch", remote, branch_path])
        .output()
        .map_err(|e| format!("git fetch {remote} {branch_path}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::debug!(
            "fetch_chain_pr_integration_base: git fetch failed stderr={}",
            stderr.trim()
        );
        return Err(format!("git fetch {remote} {branch_path} failed: {stderr}"));
    }
    log::debug!(
        "fetch_chain_pr_integration_base: fetch completed for {}",
        integration_base_ref
    );
    Ok(())
}

/// Fetches a remote ref whether it is a single-segment integration base or a multi-segment chain ref.
pub fn fetch_ref_for_workflow(repo_root: &Path, start_ref: &str) -> Result<(), String> {
    log::debug!(
        "fetch_ref_for_workflow: repo={} ref={}",
        repo_root.display(),
        start_ref
    );
    if validate_integration_base_ref(start_ref).is_ok() {
        fetch_integration_base(repo_root, start_ref)
    } else if validate_chain_pr_integration_base_ref(start_ref).is_ok() {
        fetch_chain_pr_integration_base(repo_root, start_ref)
    } else {
        Err(format!(
            "invalid workflow integration base ref for fetch: {}",
            start_ref
        ))
    }
}

/// Fetch the last-resort default integration base ([`FALLBACK_DEFAULT_INTEGRATION_BASE_REF`]).
/// Must succeed before creating a worktree from that ref.
pub fn fetch_origin_master(repo_root: &Path) -> Result<(), String> {
    log::debug!("fetch_origin_master: repo_root={}", repo_root.display());
    fetch_integration_base(repo_root, FALLBACK_DEFAULT_INTEGRATION_BASE_REF)
}

/// Fetches the given remote-tracking integration base ref (e.g. `origin/main`, `upstream/main`).
pub fn fetch_integration_base(repo_root: &Path, integration_base_ref: &str) -> Result<(), String> {
    validate_integration_base_ref(integration_base_ref)?;
    let (remote, branch) = integration_base_ref
        .split_once('/')
        .expect("validate_integration_base_ref ensures <remote>/<branch> form");
    log::info!(
        "fetch_integration_base: repo={} integration_base_ref={}",
        repo_root.display(),
        integration_base_ref
    );
    let output = git_remote_command(repo_root)
        .args(["fetch", remote, branch])
        .output()
        .map_err(|e| format!("git fetch {remote} {branch}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::debug!(
            "fetch_integration_base: git fetch failed stderr={}",
            stderr.trim()
        );
        return Err(format!("git fetch {remote} {branch} failed: {stderr}"));
    }
    log::debug!(
        "fetch_integration_base: fetch completed for {}",
        integration_base_ref
    );
    Ok(())
}

/// Pushes a local branch to `remote` and sets it as the upstream (`git push -u <remote> <branch>`),
/// run inside `worktree_dir`. Uses [`git_remote_command`] so any configured `GIT_SSH_COMMAND`
/// applies and interactive prompts fail fast. Returns a descriptive `Err` on a non-zero exit — no
/// silent success, no fallback.
pub fn push_new_branch_to_remote(
    worktree_dir: &Path,
    branch: &str,
    remote: &str,
) -> Result<(), String> {
    log::info!(
        "push_new_branch_to_remote: worktree={} branch={} remote={}",
        worktree_dir.display(),
        branch,
        remote
    );
    let output = git_remote_command(worktree_dir)
        .args(["push", "-u", remote, branch])
        .output()
        .map_err(|e| format!("git push -u {remote} {branch}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git push -u {remote} {branch} failed: {}",
            stderr.trim()
        ));
    }
    Ok(())
}

/// Legacy push helper: pushes to `origin` specifically. Retained for callers that have not yet
/// threaded the resolved default remote; new callers should use [`push_new_branch_to_remote`] with
/// the remote resolved via [`detect_default_remote_name`] or the project registry.
pub fn push_new_branch_to_origin(worktree_dir: &Path, branch: &str) -> Result<(), String> {
    push_new_branch_to_remote(worktree_dir, branch, "origin")
}

/// Detects the default remote name from the main worktree's upstream tracking branch.
///
/// Runs `git rev-parse --abbrev-ref @{upstream}` in `repo_root`; on success the result has the form
/// `<remote>/<branch>` and the segment before the first `/` is the remote. Returns `None` on a
/// detached HEAD, a branch with no upstream, a non-repository path, a missing `git`, or any non-zero
/// exit — this probe never errors the caller. `origin` is **not** assumed here; callers add it as the
/// last-resort fallback via [`resolve_default_integration_base_ref_with_remote`].
#[must_use]
pub fn detect_default_remote_name(repo_root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s == "HEAD" {
        return None;
    }
    let remote = s.split_once('/').map(|(r, _)| r).unwrap_or(&s);
    if remote.is_empty() {
        return None;
    }
    Some(remote.to_string())
}

/// Resolves which remote-tracking ref to use when no per-project override is supplied, using an
/// explicit preferred remote when the caller has one (e.g. from `ProjectData::remote_name`).
///
/// Remote selection order:
/// 1. `preferred_remote` when `Some` (caller-supplied, e.g. project config).
/// 2. [`detect_default_remote_name`] (main worktree's upstream).
/// 3. `"origin"` last resort.
///
/// Then `git fetch <remote>` and probe `<remote>/master` → `<remote>/main` →
/// `refs/remotes/<remote>/HEAD`.
pub fn resolve_default_integration_base_ref_with_remote(
    repo_root: &Path,
    preferred_remote: Option<&str>,
) -> Result<String, String> {
    let remote = preferred_remote
        .map(str::to_string)
        .or_else(|| detect_default_remote_name(repo_root))
        .unwrap_or_else(|| "origin".to_string());
    log::info!(
        "resolve_default_integration_base_ref_with_remote: fetching {} repo={}",
        remote,
        repo_root.display()
    );
    let fetch_out = git_remote_command(repo_root)
        .args(["fetch", &remote])
        .output()
        .map_err(|e| format!("git fetch {remote}: {e}"))?;
    if !fetch_out.status.success() {
        let stderr = String::from_utf8_lossy(&fetch_out.stderr);
        return Err(format!("git fetch {remote} failed: {stderr}"));
    }

    let master_ref = format!("{remote}/master");
    if remote_ref_exists(repo_root, &master_ref)? {
        log::debug!("resolve_default_integration_base_ref_with_remote: chose {master_ref}");
        return Ok(master_ref);
    }
    let main_ref = format!("{remote}/main");
    if remote_ref_exists(repo_root, &main_ref)? {
        log::debug!("resolve_default_integration_base_ref_with_remote: chose {main_ref}");
        return Ok(main_ref);
    }

    let head_symref = format!("refs/remotes/{remote}/HEAD");
    let sym = Command::new("git")
        .args(["symbolic-ref", "-q", &head_symref])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git symbolic-ref: {e}"))?;
    if sym.status.success() {
        let sym_ref = String::from_utf8_lossy(&sym.stdout).trim().to_string();
        log::debug!(
            "resolve_default_integration_base_ref_with_remote: {head_symref} -> {}",
            sym_ref
        );
        if let Some(rest) = sym_ref.strip_prefix("refs/remotes/") {
            validate_integration_base_ref(rest)?;
            return Ok(rest.to_string());
        }
    }

    Err(format!(
        "could not resolve integration base ref: no {remote}/master, {remote}/main, or {remote}/HEAD"
    ))
}

/// Resolves which remote-tracking ref to use when no per-project override is supplied.
///
/// Delegates to [`resolve_default_integration_base_ref_with_remote`] with no preferred remote, so
/// the remote is detected from the main worktree's upstream and falls back to `origin` only when
/// detection fails.
pub fn resolve_default_integration_base_ref(repo_root: &Path) -> Result<String, String> {
    resolve_default_integration_base_ref_with_remote(repo_root, None)
}

fn remote_ref_exists(repo_root: &Path, rev: &str) -> Result<bool, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", rev])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    Ok(out.status.success())
}

/// Commit sha of `<remote>/<branch>` in `repo_root`, or `None` when the branch has no
/// remote-tracking ref there (never pushed, deleted, or `repo_root` is not a git repository).
///
/// The remote is resolved via [`detect_default_remote_name`] (main worktree upstream), falling back
/// to `origin` only when detection fails. Resolves the *remote-tracking* ref, so it is only as fresh
/// as the last fetch: conservative by construction, since a PR-stack child worktree is created from
/// `<remote>/<base>` and a stale-missing answer delays a spawn rather than permitting one that would
/// fail inside `git fetch`.
///
/// Runs on a polled display path (`QueryBranch`), so every failure — a bad path, a missing git, a
/// non-repository — degrades to `None` rather than failing the enclosing call.
#[must_use]
pub fn remote_branch_ref_sha(repo_root: &Path, branch: &str) -> Option<String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return None;
    }
    let remote = detect_default_remote_name(repo_root).unwrap_or_else(|| "origin".to_string());
    let out = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{remote}/{branch}"),
        ])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return None;
    }
    Some(sha)
}

/// Lists remote-tracking branches under `<remote>/`, most recent commit first, up to `limit`
/// entries. Uses `git branch -r --sort=-committerdate`. Excludes `<remote>/HEAD` and any ref that is
/// not under `<remote>/`. Entries that fail [`validate_chain_pr_integration_base_ref`] are skipped.
pub fn list_recent_remote_branches(
    repo_root: &Path,
    remote: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    list_recent_remote_branches_skip(repo_root, remote, 0, limit)
}

/// Like [`list_recent_remote_branches`], but skips the first `skip` qualifying remote branches
/// (same ordering and filtering), then returns up to `limit` entries.
pub fn list_recent_remote_branches_skip(
    repo_root: &Path,
    remote: &str,
    skip: usize,
    limit: usize,
) -> Result<Vec<String>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let remote_prefix = format!("{remote}/");
    let head_ref = format!("{remote}/HEAD");
    let output = Command::new("git")
        .args([
            "branch",
            "-r",
            "--sort=-committerdate",
            "--format=%(refname:short)",
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git branch -r: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git branch -r failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut seen: HashSet<String> = HashSet::new();
    let mut skip_remaining = skip;
    let mut out: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == head_ref {
            continue;
        }
        if !line.starts_with(&remote_prefix) {
            continue;
        }
        if validate_chain_pr_integration_base_ref(line).is_err() {
            continue;
        }
        let line = line.to_string();
        if !seen.insert(line.clone()) {
            continue;
        }
        if skip_remaining > 0 {
            skip_remaining -= 1;
            continue;
        }
        out.push(line);
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod list_recent_remote_branches_tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn list_recent_remote_branches_lists_origin_refs() {
        let base = std::env::temp_dir().join("tddy-core-list-recent-remote-branches");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&repo)
            .output()
            .unwrap();
        fs::write(repo.join("f"), "x").unwrap();
        Command::new("git")
            .args(["add", "f"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "c"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", repo.to_str().unwrap()])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "-b", "feature/a"])
            .current_dir(&repo)
            .output()
            .unwrap();
        fs::write(repo.join("g"), "y").unwrap();
        Command::new("git")
            .args(["add", "g"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "c2"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "feature/a"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let list = list_recent_remote_branches(&repo, "origin", 10).unwrap();
        assert!(
            list.iter()
                .any(|r| r == "origin/main" || r == "origin/feature/a"),
            "expected origin/main or origin/feature/a in {:?}",
            list
        );
        assert!(!list.contains(&"origin/HEAD".to_string()));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn list_recent_remote_branches_skip_skips_first_n() {
        let base = std::env::temp_dir().join("tddy-core-list-recent-remote-branches-skip");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let repo = base.join("repo-skip");
        fs::create_dir_all(&repo).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&repo)
            .output()
            .unwrap();
        fs::write(repo.join("f"), "x").unwrap();
        Command::new("git")
            .args(["add", "f"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "c"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", repo.to_str().unwrap()])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();

        for name in ["feature/a", "feature/b"] {
            Command::new("git")
                .args(["checkout", "-b", name])
                .current_dir(&repo)
                .output()
                .unwrap();
            fs::write(repo.join("g"), name).unwrap();
            Command::new("git")
                .args(["add", "g"])
                .current_dir(&repo)
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-m", "c2"])
                .current_dir(&repo)
                .output()
                .unwrap();
            Command::new("git")
                .args(["push", "-u", "origin", name])
                .current_dir(&repo)
                .output()
                .unwrap();
        }

        let first = list_recent_remote_branches_skip(&repo, "origin", 0, 1).unwrap();
        let second = list_recent_remote_branches_skip(&repo, "origin", 1, 1).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(
            first[0], second[0],
            "skip(0,1) and skip(1,1) must differ when multiple remotes exist; first={first:?} second={second:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn push_new_branch_to_origin_creates_the_branch_on_the_remote() {
        // Given — a working repo whose `origin` is a real bare remote, with a new local branch that
        // does not yet exist on the remote.
        let base = std::env::temp_dir().join("tddy-core-push-new-branch-to-origin");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let origin = base.join("origin.git");
        Command::new("git")
            .args(["init", "--bare"])
            .arg(&origin)
            .output()
            .unwrap();

        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init"],
            vec!["config", "user.email", "t@t.com"],
            vec!["config", "user.name", "T"],
        ] {
            Command::new("git")
                .args(&args)
                .current_dir(&repo)
                .output()
                .unwrap();
        }
        fs::write(repo.join("f"), "x").unwrap();
        Command::new("git")
            .args(["add", "f"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "c"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["remote", "add", "origin", origin.to_str().unwrap()])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();

        Command::new("git")
            .args(["checkout", "-b", "feature/x"])
            .current_dir(&repo)
            .output()
            .unwrap();
        fs::write(repo.join("g"), "y").unwrap();
        Command::new("git")
            .args(["add", "g"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "c2"])
            .current_dir(&repo)
            .output()
            .unwrap();

        // When
        push_new_branch_to_origin(&repo, "feature/x").expect("push should succeed");

        // Then — the branch now exists on the remote, and the local branch tracks origin/feature/x.
        let ls = Command::new("git")
            .args(["ls-remote", "origin", "refs/heads/feature/x"])
            .current_dir(&repo)
            .output()
            .unwrap();
        let ls_out = String::from_utf8_lossy(&ls.stdout);
        assert!(
            ls_out.contains("refs/heads/feature/x"),
            "expected feature/x on the remote, ls-remote was: {ls_out:?}"
        );

        let upstream = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "feature/x@{upstream}"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&upstream.stdout).trim(),
            "origin/feature/x",
            "expected feature/x to track origin/feature/x"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
