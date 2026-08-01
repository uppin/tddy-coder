//! Git worktree management for daemon sessions.
//!
//! Worktrees are stored in `.worktrees/` relative to the repo root.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::branch_worktree_intent;
use crate::changeset::{read_changeset, write_changeset, BranchWorktreeIntent};

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

/// The local branch name behind a branch reference offered by a remote-branch picker.
///
/// Branch *pickers* deal in remote-tracking names — [`list_recent_remote_branches`] reads
/// `refs/remotes/<remote>`, so `ListProjectBranches` (and the Telegram branch picker) offer
/// `<remote>/<branch>`. Everything that operates on a branch needs the local name instead:
/// `git worktree add <path> <remote>/feature/x` succeeds with a **detached HEAD**, so a session
/// started that way looks healthy while every commit it makes is unreachable, whereas the
/// unprefixed name makes git create or reuse the local branch that tracks `<remote>/feature/x`.
///
/// This legacy helper strips one leading `origin/` and is retained for the `origin` last-resort
/// fallback path. Code that has resolved the actual default remote should call
/// [`local_branch_name_for_remote`] instead so a non-`origin` remote is normalized correctly.
///
/// Exactly one leading prefix is stripped: a repository may legitimately hold
/// `refs/heads/origin/foo`, and stripping repeatedly would rename it.
#[must_use]
pub fn local_branch_name(reference: &str) -> &str {
    local_branch_name_for_remote(reference, "origin")
}

/// The local branch name behind a `<remote>/<branch>` reference: strips one leading
/// `<remote>/` when present, and leaves any other name (already local, or carrying a different
/// remote prefix) unchanged.
///
/// Exactly one leading `<remote>/` is stripped: a repository may legitimately hold a local branch
/// whose name starts with `<remote>/`, and stripping repeatedly would rename it.
#[must_use]
pub fn local_branch_name_for_remote<'a>(reference: &'a str, remote: &str) -> &'a str {
    let reference = reference.trim();
    let prefix = format!("{remote}/");
    reference.strip_prefix(&prefix).unwrap_or(reference)
}

/// Characters that must never appear in a remote name or branch path segment passed to a `git`
/// invocation — they could widen a single `git fetch <remote> <path>` into something else.
const FORBIDDEN_REF_CHARS: [char; 7] = [';', '|', '&', '$', '`', '\n', '\r'];

/// Validates a remote name segment (the part before the first `/` in a remote-tracking ref):
/// non-empty, no whitespace, no `..`, no `--`, and none of [`FORBIDDEN_REF_CHARS`]. Pure string
/// rules — no git probe, so the remote is not required to exist in any particular repository.
fn validate_remote_segment(remote: &str) -> Result<(), String> {
    if remote.is_empty() {
        return Err("integration base ref must be <remote>/<branch>: remote is empty".to_string());
    }
    if remote.chars().any(|c| c.is_whitespace()) {
        return Err("integration base ref remote must not contain whitespace".to_string());
    }
    if remote.contains("..") {
        return Err("integration base ref remote must not contain `..`".to_string());
    }
    if remote.contains("--") {
        return Err("integration base ref remote must not contain `--`".to_string());
    }
    for forbidden in FORBIDDEN_REF_CHARS {
        if remote.contains(forbidden) {
            return Err(format!(
                "integration base ref remote contains forbidden character: {:?}",
                forbidden
            ));
        }
    }
    Ok(())
}

/// Validates a single branch path segment: non-empty, no whitespace, no `..`, no `--`, no
/// [`FORBIDDEN_REF_CHARS`].
fn validate_branch_segment(segment: &str) -> Result<(), String> {
    if segment.is_empty() {
        return Err("integration base ref must not contain empty path segments".to_string());
    }
    if segment.chars().any(|c| c.is_whitespace()) {
        return Err("integration base ref must not contain whitespace".to_string());
    }
    if segment.contains("..") {
        return Err("integration base ref must not contain `..`".to_string());
    }
    if segment.contains("--") {
        return Err("integration base ref must not contain `--`".to_string());
    }
    for forbidden in FORBIDDEN_REF_CHARS {
        if segment.contains(forbidden) {
            return Err(format!(
                "integration base ref contains forbidden character: {:?}",
                forbidden
            ));
        }
    }
    Ok(())
}

/// Splits a remote-tracking ref into `(remote, branch_path)` after validating the remote segment.
/// Returns `Err` when there is no `/` (no remote segment) or the remote segment is unsafe.
fn split_remote_ref(s: &str) -> Result<(&str, &str), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("integration base ref must not be empty".to_string());
    }
    let Some((remote, rest)) = s.split_once('/') else {
        return Err("integration base ref must be <remote>/<branch-name>".to_string());
    };
    validate_remote_segment(remote)?;
    Ok((remote, rest))
}

/// Validates a per-project integration base ref: a single remote-tracking ref
/// `<remote>/<branch>` with no shell metacharacters or extra git arguments. The remote is not
/// required to be `origin` — any safe remote name is accepted (pure string rules, no git probe).
pub fn validate_integration_base_ref(s: &str) -> Result<(), String> {
    let (remote, rest) = split_remote_ref(s)?;
    if rest.is_empty() {
        return Err("integration base ref must be <remote>/<branch-name>".to_string());
    }
    if rest.contains('/') {
        return Err(
            "integration base ref must be a single remote branch segment: <remote>/<branch-name>"
                .to_string(),
        );
    }
    validate_branch_segment(rest)?;
    let _ = remote;
    Ok(())
}

/// Validates a chain-PR integration base ref: `<remote>/<branch-path>` where `<branch-path>` may
/// contain `/` (e.g. `upstream/feature/foo`). Rejects empty strings, shell metacharacters, and
/// `..`. The remote is not required to be `origin` — any safe remote name is accepted (pure string
/// rules, no git probe).
pub fn validate_chain_pr_integration_base_ref(s: &str) -> Result<(), String> {
    let (remote, rest) = split_remote_ref(s)?;
    if rest.is_empty() {
        return Err("chain PR integration base ref must be <remote>/<branch-path>".to_string());
    }
    for segment in rest.split('/') {
        validate_branch_segment(segment)?;
    }
    let _ = remote;
    Ok(())
}

/// Fetches a remote-tracking ref for chain PRs (multi-segment `<remote>/<path>` allowed).
fn fetch_chain_pr_integration_base(
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
fn fetch_ref_for_workflow(repo_root: &Path, start_ref: &str) -> Result<(), String> {
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

/// Path to the worktrees directory under repo root.
pub fn worktree_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".worktrees")
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

/// Create a new git worktree. Returns the absolute path to the worktree.
///
/// When `start_point` is `Some("origin/master")`, creates the branch from that ref.
/// Otherwise uses HEAD.
pub fn create_worktree(
    repo_root: &Path,
    name: &str,
    branch: &str,
    start_point: Option<&str>,
) -> Result<PathBuf, String> {
    log::debug!(
        "create_worktree: repo={} name={} branch={} start_point={:?}",
        repo_root.display(),
        name,
        branch,
        start_point
    );
    let worktrees = worktree_dir(repo_root);
    std::fs::create_dir_all(&worktrees).map_err(|e| format!("create worktrees dir: {}", e))?;

    let worktree_path = worktrees.join(name);
    if worktree_path.exists() {
        return Err(format!(
            "worktree path already exists at {} — reuse the existing worktree or confirm before proceeding",
            worktree_path.display()
        ));
    }

    let mut args = vec![
        "worktree",
        "add",
        worktree_path.to_str().unwrap(),
        "-b",
        branch,
    ];
    if let Some(sp) = start_point {
        args.push(sp);
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree add: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree add failed: {}", stderr));
    }

    Ok(worktree_path.canonicalize().unwrap_or(worktree_path))
}

/// If `worktree_path` exists and is already a linked worktree of `repo_root`, and its `HEAD`
/// matches `git rev-parse <branch>` in `repo_root`, return that path for resume (changeset lost
/// `worktree` but the directory remains registered with Git).
fn try_reuse_linked_worktree_at_path(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
) -> Result<Option<PathBuf>, String> {
    if !worktree_path.exists() {
        return Ok(None);
    }
    if !path_is_registered_worktree_of_repo(repo_root, worktree_path)? {
        return Ok(None);
    }
    let expected = git_rev_parse(repo_root, branch)?;
    let actual = git_rev_parse(worktree_path, "HEAD")?;
    if expected != actual {
        return Err(format!(
            "existing worktree at {} has HEAD {actual} but {branch} resolves to {expected}; \
             remove the directory or fix the worktree before retrying",
            worktree_path.display()
        ));
    }
    Ok(Some(
        worktree_path
            .canonicalize()
            .unwrap_or_else(|_| worktree_path.to_path_buf()),
    ))
}

fn path_is_registered_worktree_of_repo(
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<bool, String> {
    let want = worktree_path
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {}", worktree_path.display(), e))?;
    let out = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree list: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some(rest) = line.strip_prefix("worktree ") else {
            continue;
        };
        let p = PathBuf::from(rest.trim());
        let canon = p.canonicalize().unwrap_or(p);
        if canon == want {
            return Ok(true);
        }
    }
    Ok(false)
}

fn git_rev_parse(cwd: &Path, rev: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse {} in {} failed: {}",
            rev,
            cwd.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Short branch name for `rev` (e.g. `feature/a`), for comparison with [`git_head_branch_name`].
fn git_rev_parse_abbrev_ref(cwd: &Path, rev: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", rev])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git rev-parse --abbrev-ref: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse --abbrev-ref {} in {} failed: {}",
            rev,
            cwd.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s == "HEAD" {
        return Err(format!(
            "git rev-parse --abbrev-ref {} in {} resolved to detached HEAD",
            rev,
            cwd.display()
        ));
    }
    Ok(s)
}

/// The branch `worktree_dir` actually has checked out, or [`None`] when its `HEAD` is detached.
///
/// [`find_existing_worktree_for_branch_ref`] may answer with a worktree that merely *shares* the
/// branch's tip commit (its tier 2), which is fine for a caller that only displays an indicator and
/// wrong for one that is about to write. A mutation asks this first and refuses on a mismatch.
pub fn checked_out_branch_name(worktree_dir: &Path) -> Result<Option<String>, String> {
    git_head_branch_name(worktree_dir)
}

/// Current branch name in `cwd`, or [`None`] when `HEAD` is detached.
fn git_head_branch_name(cwd: &Path) -> Result<Option<String>, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git rev-parse --abbrev-ref HEAD: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse --abbrev-ref HEAD in {} failed: {}",
            cwd.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s == "HEAD" {
        return Ok(None);
    }
    Ok(Some(s))
}

/// Lists absolute paths of all registered worktrees (including the primary checkout).
fn registered_worktree_paths(repo_root: &Path) -> Result<Vec<PathBuf>, String> {
    let out = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree list: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut paths = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some(rest) = line.strip_prefix("worktree ") else {
            continue;
        };
        paths.push(PathBuf::from(rest.trim()));
    }
    Ok(paths)
}

/// Finds a worktree to reuse for `branch_ref`:
/// 1. **Name match**: a registered worktree whose current branch name equals `git rev-parse
///    --abbrev-ref` of `branch_ref` in `repo_root` (e.g. `feature/x` checked out for `feature/x`).
/// 2. Else **linked + same tip**: a worktree path under **`.worktrees/`** whose `HEAD` equals the
///    resolved commit of `branch_ref`. This covers `origin/feature/x` vs a local `feature/x`
///    checkout without matching the **primary** checkout when it sits on another branch (e.g.
///    `master`) while an unused local branch exists at the same commit as `master` — the primary
///    is not under `.worktrees/` and is excluded from tier 2.
///
/// Preference order within a tier: paths under **`.worktrees/`** first, then others, then
/// lexicographic path order for stability.
pub fn find_existing_worktree_for_branch_ref(
    repo_root: &Path,
    branch_ref: &str,
) -> Result<Option<PathBuf>, String> {
    let target = git_rev_parse(repo_root, branch_ref)?;
    let want_branch = git_rev_parse_abbrev_ref(repo_root, branch_ref).ok();

    let mut by_name: Vec<PathBuf> = Vec::new();
    let mut by_commit_linked: Vec<PathBuf> = Vec::new();

    for p in registered_worktree_paths(repo_root)? {
        if !p.exists() {
            continue;
        }
        if let Some(ref w) = want_branch {
            if let Some(cur) = git_head_branch_name(&p)? {
                if cur == *w {
                    by_name.push(p.canonicalize().unwrap_or(p));
                    continue;
                }
            }
        }
        let Ok(head) = git_rev_parse(&p, "HEAD") else {
            continue;
        };
        if head != target {
            continue;
        }
        if p.to_string_lossy().contains("/.worktrees/") {
            by_commit_linked.push(p.canonicalize().unwrap_or(p));
        }
    }

    let sort_key = |a: &PathBuf, b: &PathBuf| {
        let aw = a.to_string_lossy().contains("/.worktrees/");
        let bw = b.to_string_lossy().contains("/.worktrees/");
        bw.cmp(&aw).then_with(|| a.cmp(b))
    };

    if !by_name.is_empty() {
        let mut v = by_name;
        v.sort_by(sort_key);
        return Ok(v.into_iter().next());
    }
    if !by_commit_linked.is_empty() {
        let mut v = by_commit_linked;
        v.sort_by(sort_key);
        return Ok(v.into_iter().next());
    }
    Ok(None)
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

/// Public, non-erroring wrapper over [`try_find_existing_worktree_for_branch_ref`]: returns the
/// on-disk worktree path checked out for `branch` in `repo_root`, or `None` when the branch does not
/// resolve or has no worktree. Errors (I/O, worktree enumeration) collapse to `None` — callers that
/// only want to display a worktree indicator do not need to distinguish "no worktree" from a
/// transient git error.
pub fn worktree_path_for_branch(repo_root: &Path, branch: &str) -> Option<PathBuf> {
    try_find_existing_worktree_for_branch_ref(repo_root, branch)
        .ok()
        .flatten()
}

/// Like [`find_existing_worktree_for_branch_ref`], but returns `Ok(None)` when `branch_ref` does not
/// resolve in `repo_root` (e.g. suggested branch not created yet). Propagates I/O and worktree
/// enumeration errors.
fn try_find_existing_worktree_for_branch_ref(
    repo_root: &Path,
    branch_ref: &str,
) -> Result<Option<PathBuf>, String> {
    let verify = Command::new("git")
        .args(["rev-parse", "--verify", branch_ref])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git rev-parse --verify: {}", e))?;
    if !verify.status.success() {
        return Ok(None);
    }
    find_existing_worktree_for_branch_ref(repo_root, branch_ref)
}

/// Add a linked worktree at `.worktrees/<name>` checked out to an **existing** local branch.
///
/// Uses `git worktree add <path> <branch>` when the branch is not already checked out in another
/// worktree. If Git refuses because the branch is in use (common when the primary repo already
/// has `main` checked out), falls back to `worktree add --detach` at the branch tip, then
/// `git switch --ignore-other-worktrees <branch>` in the new worktree so `branch --show-current`
/// matches the selected branch (PRD: work on selected branch).
///
/// When the path already exists, the error instructs the user to confirm reuse (PRD).
pub fn add_worktree_for_existing_branch(
    repo_root: &Path,
    name: &str,
    branch: &str,
) -> Result<PathBuf, String> {
    log::info!(
        "add_worktree_for_existing_branch: repo={} worktree_name={} branch={}",
        repo_root.display(),
        name,
        branch
    );
    let worktrees = worktree_dir(repo_root);
    std::fs::create_dir_all(&worktrees).map_err(|e| format!("create worktrees dir: {}", e))?;
    let worktree_path = worktrees.join(name);
    if worktree_path.exists() {
        match try_reuse_linked_worktree_at_path(repo_root, &worktree_path, branch) {
            Ok(Some(p)) => {
                log::info!(
                    "add_worktree_for_existing_branch: reusing existing linked worktree at {}",
                    p.display()
                );
                return Ok(p);
            }
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        return Err(format!(
            "worktree path already exists at {} — reuse the existing worktree or confirm before proceeding",
            worktree_path.display()
        ));
    }

    let try_direct = Command::new("git")
        .args(["worktree", "add", worktree_path.to_str().unwrap(), branch])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree add: {}", e))?;

    if try_direct.status.success() {
        return Ok(worktree_path.canonicalize().unwrap_or(worktree_path));
    }

    let stderr = String::from_utf8_lossy(&try_direct.stderr);
    log::debug!(
        "add_worktree_for_existing_branch: direct add failed stderr={}",
        stderr.trim()
    );

    let branch_in_use = stderr.contains("already used")
        || stderr.contains("is already checked out")
        || stderr.to_lowercase().contains("already");

    if !branch_in_use {
        return Err(format!("git worktree add failed: {}", stderr));
    }

    log::info!(
        "add_worktree_for_existing_branch: using detach+switch fallback for branch {}",
        branch
    );

    let rev_out = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    if !rev_out.status.success() {
        let rev_stderr = String::from_utf8_lossy(&rev_out.stderr);
        return Err(format!(
            "git rev-parse refs/heads/{branch} failed: {}",
            rev_stderr
        ));
    }
    let rev = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

    let detach = Command::new("git")
        .args([
            "worktree",
            "add",
            "--detach",
            worktree_path.to_str().unwrap(),
            &rev,
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree add --detach: {}", e))?;
    if !detach.status.success() {
        let e = String::from_utf8_lossy(&detach.stderr);
        return Err(format!("git worktree add --detach failed: {}", e));
    }

    let sw = Command::new("git")
        .args(["switch", "--ignore-other-worktrees", branch])
        .current_dir(&worktree_path)
        .output()
        .map_err(|e| format!("git switch: {}", e))?;
    if !sw.status.success() {
        let e = String::from_utf8_lossy(&sw.stderr);
        return Err(format!(
            "git switch --ignore-other-worktrees {branch} failed: {}",
            e
        ));
    }

    Ok(worktree_path.canonicalize().unwrap_or(worktree_path))
}

const MAX_WORKTREE_RETRIES: u32 = 20;

/// Try `create_worktree`; on "branch ... already exists" retry with `-1`, `-2`, etc.
/// Returns `(worktree_path, actual_branch_name)`.
fn create_worktree_with_retry(
    repo_root: &Path,
    name: &str,
    branch: &str,
    start_point: Option<&str>,
) -> Result<(PathBuf, String), String> {
    match create_worktree(repo_root, name, branch, start_point) {
        Ok(path) => return Ok((path, branch.to_string())),
        Err(e) if e.contains("already exists") => {
            log::debug!("worktree branch {branch:?} exists, retrying with suffix");
        }
        Err(e) => return Err(e),
    }
    for i in 1..=MAX_WORKTREE_RETRIES {
        let suffixed_branch = format!("{branch}-{i}");
        let suffixed_name = format!("{name}-{i}");
        match create_worktree(repo_root, &suffixed_name, &suffixed_branch, start_point) {
            Ok(path) => return Ok((path, suffixed_branch)),
            Err(e) if e.contains("already exists") => continue,
            Err(e) => return Err(e),
        }
    }
    Err(format!(
        "exhausted {MAX_WORKTREE_RETRIES} retries for branch {branch:?}"
    ))
}

/// The first `<branch>-<n>` (`n` from 1) that no local branch in `repo_root` holds — the name
/// [`create_worktree_with_retry`] would land on, computed without creating a branch or a worktree.
///
/// Used for the `suggested_branch_name` a refused session creation reports, which pre-fills the
/// operator's rename field, so the answer has to be usable as-is. A repo whose branches cannot be
/// listed (not a git repository, no `git`) reports no branch as taken and so suggests `<branch>-1`,
/// the same name the retry loop would try first.
///
/// See docs/ft/daemon/session-branch-conflict.md.
#[must_use]
pub fn first_free_suffixed_branch_name(repo_root: &Path, branch: &str) -> String {
    let taken = local_branch_names(repo_root);
    let mut suffix = 1u32;
    loop {
        let candidate = format!("{branch}-{suffix}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Every local branch name in `repo_root`, or an empty set when its refs cannot be listed.
fn local_branch_names(repo_root: &Path) -> HashSet<String> {
    let Ok(out) = Command::new("git")
        .args(["for-each-ref", "--format=%(refname)", "refs/heads"])
        .current_dir(repo_root)
        .output()
    else {
        return HashSet::new();
    };
    if !out.status.success() {
        return HashSet::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("refs/heads/"))
        .map(str::to_string)
        .collect()
}

/// Create worktree for a session using an explicit integration base ref (e.g. `origin/main`).
pub fn setup_worktree_for_session_with_integration_base(
    repo_root: &Path,
    session_dir: &Path,
    integration_base_ref: &str,
) -> Result<PathBuf, String> {
    validate_integration_base_ref(integration_base_ref)?;
    log::info!(
        "setup_worktree_for_session_with_integration_base: repo={} ref={}",
        repo_root.display(),
        integration_base_ref
    );
    let mut cs = read_changeset(session_dir).map_err(|e| e.to_string())?;

    branch_worktree_intent::validate_workflow_branch_intent(&cs)?;

    if let Some(ref wf) = cs.workflow {
        if let Some(intent) = wf.branch_worktree_intent {
            match intent {
                BranchWorktreeIntent::NewBranchFromBase => {
                    let new_name = wf.new_branch_name.clone().ok_or_else(|| {
                        "workflow.new_branch_name required for new_branch_from_base".to_string()
                    })?;
                    let start = wf
                        .selected_integration_base_ref
                        .as_deref()
                        .unwrap_or(integration_base_ref);
                    log::info!(
                        "setup_worktree_for_session_with_integration_base: intent=new_branch_from_base new_branch={} start_ref={}",
                        new_name,
                        start
                    );
                    fetch_ref_for_workflow(repo_root, start)?;
                    let worktree_name = cs
                        .worktree_directory_basename()
                        .ok_or_else(|| "no worktree suggestion or name for worktree".to_string())?;
                    let (worktree_path, actual_branch) = create_worktree_with_retry(
                        repo_root,
                        &worktree_name,
                        &new_name,
                        Some(start),
                    )?;
                    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
                    cs.branch = Some(actual_branch);
                    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
                    write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
                    log::debug!(
                        "setup_worktree_for_session_with_integration_base: worktree_path={}",
                        worktree_path.display()
                    );
                    return Ok(worktree_path);
                }
                BranchWorktreeIntent::WorkOnSelectedBranch => {
                    // The request may name the branch the way a remote-branch picker does
                    // (`origin/<branch>`); the worktree has to be put on the *local* branch.
                    let branch_name = wf
                        .selected_branch_to_work_on
                        .as_deref()
                        .map(|b| local_branch_name(b).to_string())
                        .filter(|b| !b.is_empty())
                        .ok_or_else(|| {
                            "workflow.selected_branch_to_work_on required for work_on_selected_branch"
                                .to_string()
                        })?;
                    log::info!(
                        "setup_worktree_for_session_with_integration_base: intent=work_on_selected_branch branch={}",
                        branch_name
                    );
                    fetch_integration_base(repo_root, integration_base_ref)?;
                    // A branch that exists only on `origin` — the state after the session that created
                    // it was deleted, or when it was pushed from another host — has no worktree here
                    // yet. That is not an error: `git worktree add` below creates the local tracking
                    // branch.
                    if let Some(existing) =
                        try_find_existing_worktree_for_branch_ref(repo_root, &branch_name)?
                    {
                        log::info!(
                            "setup_worktree_for_session_with_integration_base: reusing existing worktree {} for {} (no new git worktree add)",
                            existing.display(),
                            branch_name
                        );
                        cs.worktree = Some(existing.to_string_lossy().to_string());
                        cs.branch = Some(branch_name.clone());
                        cs.repo_path = Some(existing.to_string_lossy().to_string());
                        write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
                        return Ok(existing);
                    }
                    let worktree_name = cs
                        .worktree_directory_basename()
                        .ok_or_else(|| "no worktree suggestion or name for worktree".to_string())?;
                    let worktree_path =
                        add_worktree_for_existing_branch(repo_root, &worktree_name, &branch_name)?;
                    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
                    cs.branch = Some(branch_name.clone());
                    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
                    write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
                    log::debug!(
                        "setup_worktree_for_session_with_integration_base: worktree_path={}",
                        worktree_path.display()
                    );
                    return Ok(worktree_path);
                }
            }
        }
    }

    let branch = cs
        .branch_suggestion
        .clone()
        .or(cs.branch.clone())
        .or_else(|| {
            cs.name
                .as_ref()
                .map(|n| format!("feature/{}", slugify_for_branch(n)))
        })
        .ok_or("no branch suggestion or name for worktree")?;

    let worktree_name = cs
        .worktree_directory_basename()
        .ok_or_else(|| "no worktree suggestion or name for worktree".to_string())?;

    fetch_integration_base(repo_root, integration_base_ref)?;

    if let Some(existing) = try_find_existing_worktree_for_branch_ref(repo_root, &branch)? {
        log::info!(
            "setup_worktree_for_session_with_integration_base: reusing existing worktree {} for branch {} (no new git worktree add)",
            existing.display(),
            branch
        );
        cs.worktree = Some(existing.to_string_lossy().to_string());
        cs.branch = Some(branch.clone());
        cs.repo_path = Some(existing.to_string_lossy().to_string());
        write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
        return Ok(existing);
    }

    let (worktree_path, actual_branch) = create_worktree_with_retry(
        repo_root,
        &worktree_name,
        &branch,
        Some(integration_base_ref),
    )?;

    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
    cs.branch = Some(actual_branch);
    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
    write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;

    log::debug!(
        "setup_worktree_for_session_with_integration_base: worktree_path={}",
        worktree_path.display()
    );
    Ok(worktree_path)
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

/// Create worktree for a session. Fetches the resolved integration base, then creates,
/// updates changeset with worktree, branch, repo_path. Returns the worktree path.
///
/// When no project-specific ref is available, the ref is resolved with
/// [`resolve_default_integration_base_ref`] (prefers `origin/master`, then `origin/main`, then
/// `origin/HEAD`).
pub fn setup_worktree_for_session(repo_root: &Path, session_dir: &Path) -> Result<PathBuf, String> {
    log::info!(
        "setup_worktree_for_session: repo_root={}",
        repo_root.display()
    );
    let integration_base_ref = resolve_default_integration_base_ref(repo_root)?;
    setup_worktree_for_session_with_integration_base(repo_root, session_dir, &integration_base_ref)
}

/// Starts session worktree setup with an optional chain-PR base ref (`origin/...`).
///
/// When `optional_chain_base_ref` is `None`, behavior must match [`setup_worktree_for_session`]
/// (default integration base resolution). When `Some`, the worktree branch is created from that
/// ref after fetch, and the choice is persisted to `changeset.yaml` for resume.
///
/// When `optional_chain_base_ref` is `None`, resolves the default integration base (same as
/// [`setup_worktree_for_session`]), persists [`Changeset::effective_worktree_integration_base_ref`],
/// and leaves [`Changeset::worktree_integration_base_ref`] unset. When `Some`, validates and fetches
/// the multi-segment ref, creates the worktree from that tip, and persists both fields.
pub fn setup_worktree_for_session_with_optional_chain_base(
    repo_root: &Path,
    session_dir: &Path,
    optional_chain_base_ref: Option<&str>,
) -> Result<PathBuf, String> {
    log::info!(
        "setup_worktree_for_session_with_optional_chain_base: repo={} session_dir={} chain_opt_in={}",
        repo_root.display(),
        session_dir.display(),
        optional_chain_base_ref.is_some()
    );

    let (integration_base_ref, user_chain_ref): (String, Option<&str>) =
        match optional_chain_base_ref {
            None => {
                let resolved = resolve_default_integration_base_ref(repo_root)?;
                log::debug!(
                    "setup_worktree_for_session_with_optional_chain_base: no chain base; resolved effective ref={}",
                    resolved
                );
                (resolved, None)
            }
            Some(r) => {
                validate_chain_pr_integration_base_ref(r)?;
                log::info!(
                    "setup_worktree_for_session_with_optional_chain_base: user-selected chain base ref={}",
                    r
                );
                (r.to_string(), Some(r))
            }
        };

    let mut cs = read_changeset(session_dir).map_err(|e| e.to_string())?;

    branch_worktree_intent::validate_workflow_branch_intent(&cs)?;

    if let Some(ref wf) = cs.workflow {
        if let Some(intent) = wf.branch_worktree_intent {
            match intent {
                BranchWorktreeIntent::NewBranchFromBase => {
                    let new_name = wf.new_branch_name.clone().ok_or_else(|| {
                        "workflow.new_branch_name required for new_branch_from_base".to_string()
                    })?;
                    let start = wf
                        .selected_integration_base_ref
                        .as_deref()
                        .unwrap_or(integration_base_ref.as_str());
                    log::info!(
                        "setup_worktree_for_session_with_optional_chain_base: intent=new_branch_from_base new_branch={} start_ref={}",
                        new_name,
                        start
                    );
                    fetch_ref_for_workflow(repo_root, start)?;
                    let worktree_name = cs
                        .worktree_directory_basename()
                        .ok_or_else(|| "no worktree suggestion or name for worktree".to_string())?;
                    let (worktree_path, actual_branch) = create_worktree_with_retry(
                        repo_root,
                        &worktree_name,
                        &new_name,
                        Some(start),
                    )?;
                    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
                    cs.branch = Some(actual_branch);
                    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
                    cs.effective_worktree_integration_base_ref = Some(integration_base_ref.clone());
                    cs.worktree_integration_base_ref = user_chain_ref.map(|s| s.to_string());
                    write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
                    log::debug!(
                        "setup_worktree_for_session_with_optional_chain_base: worktree_path={} effective_base={}",
                        worktree_path.display(),
                        integration_base_ref
                    );
                    return Ok(worktree_path);
                }
                BranchWorktreeIntent::WorkOnSelectedBranch => {
                    // The request may name the branch the way a remote-branch picker does
                    // (`origin/<branch>`); the worktree has to be put on the *local* branch.
                    let branch_name = wf
                        .selected_branch_to_work_on
                        .as_deref()
                        .map(|b| local_branch_name(b).to_string())
                        .filter(|b| !b.is_empty())
                        .ok_or_else(|| {
                            "workflow.selected_branch_to_work_on required for work_on_selected_branch"
                                .to_string()
                        })?;
                    log::info!(
                        "setup_worktree_for_session_with_optional_chain_base: intent=work_on_selected_branch branch={}",
                        branch_name
                    );
                    if user_chain_ref.is_some() {
                        fetch_chain_pr_integration_base(repo_root, &integration_base_ref)?;
                    } else {
                        fetch_integration_base(repo_root, &integration_base_ref)?;
                    }
                    // A branch that exists only on `origin` — the state after the session that created
                    // it was deleted, or when it was pushed from another host — has no worktree here
                    // yet. That is not an error: `git worktree add` below creates the local tracking
                    // branch.
                    if let Some(existing) =
                        try_find_existing_worktree_for_branch_ref(repo_root, &branch_name)?
                    {
                        log::info!(
                            "setup_worktree_for_session_with_optional_chain_base: reusing existing worktree {} for {} (no new git worktree add)",
                            existing.display(),
                            branch_name
                        );
                        cs.worktree = Some(existing.to_string_lossy().to_string());
                        cs.branch = Some(branch_name.clone());
                        cs.repo_path = Some(existing.to_string_lossy().to_string());
                        cs.effective_worktree_integration_base_ref =
                            Some(integration_base_ref.clone());
                        cs.worktree_integration_base_ref = user_chain_ref.map(|s| s.to_string());
                        write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
                        return Ok(existing);
                    }
                    let worktree_name = cs
                        .worktree_directory_basename()
                        .ok_or_else(|| "no worktree suggestion or name for worktree".to_string())?;
                    let worktree_path =
                        add_worktree_for_existing_branch(repo_root, &worktree_name, &branch_name)?;
                    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
                    cs.branch = Some(branch_name.clone());
                    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
                    cs.effective_worktree_integration_base_ref = Some(integration_base_ref.clone());
                    cs.worktree_integration_base_ref = user_chain_ref.map(|s| s.to_string());
                    write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
                    log::debug!(
                        "setup_worktree_for_session_with_optional_chain_base: worktree_path={} effective_base={}",
                        worktree_path.display(),
                        integration_base_ref
                    );
                    return Ok(worktree_path);
                }
            }
        }
    }

    let branch = cs
        .branch_suggestion
        .clone()
        .or(cs.branch.clone())
        .or_else(|| {
            cs.name
                .as_ref()
                .map(|n| format!("feature/{}", slugify_for_branch(n)))
        })
        .ok_or("no branch suggestion or name for worktree")?;

    let worktree_name = cs
        .worktree_directory_basename()
        .ok_or_else(|| "no worktree suggestion or name for worktree".to_string())?;

    if user_chain_ref.is_some() {
        fetch_chain_pr_integration_base(repo_root, &integration_base_ref)?;
    } else {
        fetch_integration_base(repo_root, &integration_base_ref)?;
    }

    if let Some(existing) = try_find_existing_worktree_for_branch_ref(repo_root, &branch)? {
        log::info!(
            "setup_worktree_for_session_with_optional_chain_base: reusing existing worktree {} for branch {} (no new git worktree add)",
            existing.display(),
            branch
        );
        cs.worktree = Some(existing.to_string_lossy().to_string());
        cs.branch = Some(branch.clone());
        cs.repo_path = Some(existing.to_string_lossy().to_string());
        cs.effective_worktree_integration_base_ref = Some(integration_base_ref.clone());
        cs.worktree_integration_base_ref = user_chain_ref.map(|s| s.to_string());
        write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;
        return Ok(existing);
    }

    let (worktree_path, actual_branch) = create_worktree_with_retry(
        repo_root,
        &worktree_name,
        &branch,
        Some(integration_base_ref.as_str()),
    )?;

    cs.worktree = Some(worktree_path.to_string_lossy().to_string());
    cs.branch = Some(actual_branch);
    cs.repo_path = Some(worktree_path.to_string_lossy().to_string());
    cs.effective_worktree_integration_base_ref = Some(integration_base_ref.clone());
    cs.worktree_integration_base_ref = user_chain_ref.map(|s| s.to_string());

    write_changeset(session_dir, &cs).map_err(|e| e.to_string())?;

    log::debug!(
        "setup_worktree_for_session_with_optional_chain_base: worktree_path={} effective_base={}",
        worktree_path.display(),
        integration_base_ref
    );
    Ok(worktree_path)
}

/// Resolves which integration base ref resume / follow-up worktree operations must use for this session.
///
/// Prefers persisted [`Changeset::effective_worktree_integration_base_ref`], then
/// [`Changeset::worktree_integration_base_ref`], otherwise [`resolve_default_integration_base_ref`].
pub fn resolve_persisted_worktree_integration_base_for_session(
    session_dir: &Path,
    repo_root: &Path,
) -> Result<String, String> {
    log::info!(
        "resolve_persisted_worktree_integration_base_for_session: session_dir={} repo={}",
        session_dir.display(),
        repo_root.display()
    );
    let cs = read_changeset(session_dir).map_err(|e| e.to_string())?;
    if let Some(ref eff) = cs.effective_worktree_integration_base_ref {
        log::debug!(
            "resolve_persisted_worktree_integration_base_for_session: using persisted effective ref={}",
            eff
        );
        return Ok(eff.clone());
    }
    if let Some(ref user) = cs.worktree_integration_base_ref {
        log::debug!(
            "resolve_persisted_worktree_integration_base_for_session: using persisted user chain ref={}",
            user
        );
        return Ok(user.clone());
    }
    let resolved = resolve_default_integration_base_ref(repo_root)?;
    log::debug!(
        "resolve_persisted_worktree_integration_base_for_session: no persisted base; resolved default={}",
        resolved
    );
    Ok(resolved)
}

/// Remove an existing worktree. Uses `git worktree remove --force`.
pub fn remove_worktree(repo_root: &Path, worktree_path: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args([
            "worktree",
            "remove",
            "--force",
            worktree_path.to_str().unwrap_or(""),
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree remove: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If git worktree remove fails (e.g. not registered), fall back to removing the directory
        log::debug!(
            "git worktree remove failed ({}), removing directory directly",
            stderr.trim()
        );
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path)
                .map_err(|e| format!("remove worktree dir: {}", e))?;
        }
    }
    Ok(())
}

fn slugify_for_branch(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
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

/// Info about an existing worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
}

/// List worktrees under the repo. Returns the main worktree and any linked worktrees.
pub fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeInfo>, String> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git worktree list: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree list failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let (Some(p), b) = (current_path.take(), current_branch.take()) {
                worktrees.push(WorktreeInfo { path: p, branch: b });
            }
            current_path = Some(PathBuf::from(path.trim()));
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(branch.trim().to_string());
        }
    }
    if let Some(p) = current_path {
        worktrees.push(WorktreeInfo {
            path: p,
            branch: current_branch,
        });
    }

    Ok(worktrees)
}

#[cfg(test)]
mod integration_base_red_tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    /// `fetch_integration_base` runs `git fetch origin <branch>` for a valid remote-tracking ref.
    #[test]
    fn fetch_integration_base_succeeds_for_valid_origin_main_red() {
        let base = std::env::temp_dir().join("tddy-core-fetch-int-base-green");
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

        assert!(
            fetch_integration_base(&repo, "origin/main").is_ok(),
            "fetch_integration_base must succeed for a valid repo and ref"
        );
    }

    /// RED: session setup with explicit `origin/main` must complete worktree creation (skeleton returns Err).
    #[test]
    fn setup_worktree_with_integration_base_completes_red() {
        let base = std::env::temp_dir().join("tddy-core-setup-int-base-red");
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

        let session_dir = base.join("sess");
        fs::create_dir_all(&session_dir).unwrap();
        let cs = crate::changeset::Changeset {
            name: Some("n".to_string()),
            branch_suggestion: Some("feature/x".to_string()),
            worktree_suggestion: Some("feature-x".to_string()),
            ..Default::default()
        };
        crate::changeset::write_changeset(&session_dir, &cs).unwrap();

        let r =
            setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/main");
        assert!(
            r.is_ok(),
            "GREEN: must create worktree from origin/main; got {:?}",
            r.err()
        );
    }
}

/// The local branch name behind a reference offered by a remote-branch picker.
#[cfg(test)]
mod local_branch_name_tests {
    use super::*;

    #[test]
    fn strips_the_remote_prefix_from_a_remote_tracking_name() {
        // Given / When — the form `ListProjectBranches` offers
        let branch = local_branch_name("origin/feature/attach-docs/attach-store");

        // Then
        assert_eq!(branch, "feature/attach-docs/attach-store");
    }

    #[test]
    fn leaves_a_name_that_is_already_local_unchanged() {
        // Given / When
        let branch = local_branch_name("feature/attach-docs/attach-store");

        // Then
        assert_eq!(branch, "feature/attach-docs/attach-store");
    }

    #[test]
    fn strips_only_one_remote_prefix_so_a_local_origin_branch_keeps_its_name() {
        // Given / When — `refs/heads/origin/legacy` is a legal local branch; stripping twice renames it
        let branch = local_branch_name("origin/origin/legacy");

        // Then
        assert_eq!(branch, "origin/legacy");
    }

    #[test]
    fn trims_surrounding_whitespace() {
        // Given / When
        let branch = local_branch_name("  origin/master\n");

        // Then
        assert_eq!(branch, "master");
    }
}

/// RED: chain-PR validation and resume helpers (must fail until Green implements behavior).
#[cfg(test)]
mod chain_pr_red_tests {
    use super::*;
    use std::fs;

    /// Lower-level RED: multi-segment `origin/feature/foo` must validate once rules land.
    #[test]
    fn chain_pr_validate_accepts_multi_segment_origin_ref_red() {
        let r = validate_chain_pr_integration_base_ref("origin/feature/foo");
        assert!(
            r.is_ok(),
            "expected validate_chain_pr_integration_base_ref to accept safe multi-segment refs; got {:?}",
            r
        );
    }

    /// Lower-level RED: empty ref rejected with controlled error (distinct from \"not implemented\").
    #[test]
    fn chain_pr_validate_rejects_empty_red() {
        let r = validate_chain_pr_integration_base_ref("");
        assert!(r.is_err(), "expected empty ref to be rejected; got {:?}", r);
        let msg = r.unwrap_err();
        assert!(
            !msg.contains("not implemented"),
            "empty ref should fail with a real validation error, not stub; got {:?}",
            msg
        );
    }

    /// Lower-level regression: resolve must read persisted `changeset.yaml` and return stored effective ref.
    #[test]
    fn chain_pr_resolve_persisted_reads_changeset_red() {
        let base = std::env::temp_dir().join("tddy-core-chain-pr-resolve-red");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let session_dir = base.join("session");
        fs::create_dir_all(&session_dir).unwrap();

        let cs = crate::changeset::Changeset {
            effective_worktree_integration_base_ref: Some("origin/feature/pr-base".to_string()),
            worktree_integration_base_ref: Some("origin/feature/pr-base".to_string()),
            ..Default::default()
        };
        crate::changeset::write_changeset(&session_dir, &cs).unwrap();

        let resolved = resolve_persisted_worktree_integration_base_for_session(&session_dir, &repo);
        assert!(
            resolved.is_ok(),
            "expected resolve to return persisted base; got {:?}",
            resolved
        );
        assert_eq!(
            resolved.unwrap(),
            "origin/feature/pr-base",
            "resume must return the canonical persisted effective ref"
        );

        let _ = fs::remove_dir_all(&base);
    }
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

/// Remote-agnostic validation, detection, and resolution: the `origin` assumption is gone.
#[cfg(test)]
mod remote_agnostic_tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn repo_with_remote_master(repo: &Path, remote: &str) {
        fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "--initial-branch=master"]);
        git(repo, &["config", "user.email", "t@t.com"]);
        git(repo, &["config", "user.name", "T"]);
        fs::write(repo.join("f"), "x").unwrap();
        git(repo, &["add", "f"]);
        git(repo, &["commit", "-m", "initial"]);
        git(repo, &["remote", "add", remote, repo.to_str().unwrap()]);
        git(repo, &["push", "-u", remote, "master"]);
    }

    /// The single-segment validator accepts a non-`origin` remote.
    #[test]
    fn validate_integration_base_ref_accepts_a_non_origin_remote() {
        // Given / When
        let r = validate_integration_base_ref("upstream/main");

        // Then
        assert!(
            r.is_ok(),
            "validate_integration_base_ref must accept any safe <remote>/<branch>; got {:?}",
            r
        );
    }

    /// The chain-PR validator accepts a multi-segment ref under a non-`origin` remote.
    #[test]
    fn validate_chain_pr_integration_base_ref_accepts_a_non_origin_multi_segment_ref() {
        // Given / When
        let r = validate_chain_pr_integration_base_ref("upstream/feature/foo");

        // Then
        assert!(
            r.is_ok(),
            "validate_chain_pr_integration_base_ref must accept any safe <remote>/<path>; got {:?}",
            r
        );
    }

    /// A ref with no `/` (no remote segment) is rejected — the remote is mandatory.
    #[test]
    fn validate_integration_base_ref_rejects_a_ref_with_no_remote_segment() {
        // Given / When
        let r = validate_integration_base_ref("refs/heads/main");

        // Then
        assert!(
            r.is_err(),
            "a ref with no <remote>/<branch> form must be rejected; got {:?}",
            r
        );
    }

    /// `local_branch_name_for_remote` strips one leading `<remote>/` and leaves other names alone.
    #[test]
    fn local_branch_name_for_remote_strips_the_given_remote_once() {
        // Given / When
        let branch = local_branch_name_for_remote("upstream/feature/attach-docs", "upstream");

        // Then
        assert_eq!(branch, "feature/attach-docs");
    }

    /// A different remote prefix is left intact so the caller can detect the mismatch.
    #[test]
    fn local_branch_name_for_remote_leaves_a_foreign_remote_prefix_unchanged() {
        // Given / When
        let branch = local_branch_name_for_remote("origin/feature/x", "upstream");

        // Then
        assert_eq!(branch, "origin/feature/x");
    }

    /// `detect_default_remote_name` returns the remote the main worktree's branch tracks.
    #[test]
    fn detect_default_remote_name_returns_the_tracked_remote() {
        // Given — a repo whose `master` tracks `upstream/master`
        let base = std::env::temp_dir().join(format!(
            "tddy-core-detect-remote-tracked-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        repo_with_remote_master(&repo, "upstream");

        // When
        let remote = detect_default_remote_name(&repo);

        // Then
        assert_eq!(
            remote.as_deref(),
            Some("upstream"),
            "the main worktree's upstream remote must be detected"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `detect_default_remote_name` returns `None` on a detached HEAD (no upstream to read).
    #[test]
    fn detect_default_remote_name_returns_none_on_detached_head() {
        // Given
        let base = std::env::temp_dir().join(format!(
            "tddy-core-detect-remote-detached-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        repo_with_remote_master(&repo, "origin");
        git(&repo, &["checkout", "--detach", "master"]);

        // When
        let remote = detect_default_remote_name(&repo);

        // Then
        assert!(
            remote.is_none(),
            "a detached HEAD has no upstream; got {:?}",
            remote
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `resolve_default_integration_base_ref_with_remote` probes `<remote>/master` first.
    #[test]
    fn resolve_with_remote_chooses_remote_master_when_present() {
        // Given
        let base = std::env::temp_dir().join(format!(
            "tddy-core-resolve-with-remote-master-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        repo_with_remote_master(&repo, "upstream");

        // When
        let resolved = resolve_default_integration_base_ref_with_remote(&repo, Some("upstream"));

        // Then
        assert_eq!(
            resolved.as_deref(),
            Ok("upstream/master"),
            "must probe <remote>/master for a non-origin remote"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `resolve_default_integration_base_ref_with_remote` falls through to `<remote>/main` when
    /// `master` is absent.
    #[test]
    fn resolve_with_remote_chooses_remote_main_when_master_absent() {
        // Given — a repo whose only mainline branch is `main`, pushed under `upstream`
        let base = std::env::temp_dir().join(format!(
            "tddy-core-resolve-with-remote-main-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=main"]);
        git(&repo, &["config", "user.email", "t@t.com"]);
        git(&repo, &["config", "user.name", "T"]);
        fs::write(repo.join("f"), "x").unwrap();
        git(&repo, &["add", "f"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(
            &repo,
            &["remote", "add", "upstream", repo.to_str().unwrap()],
        );
        git(&repo, &["push", "-u", "upstream", "main"]);

        // When
        let resolved = resolve_default_integration_base_ref_with_remote(&repo, Some("upstream"));

        // Then
        assert_eq!(
            resolved.as_deref(),
            Ok("upstream/main"),
            "must fall through to <remote>/main when <remote>/master is absent"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `list_recent_remote_branches` filters under the requested remote, not hardcoded `origin`.
    #[test]
    fn list_recent_remote_branches_filters_under_the_requested_remote() {
        // Given — a repo with two remotes: `origin` and `upstream`, each carrying a distinct branch
        let base = std::env::temp_dir().join(format!(
            "tddy-core-list-recent-remote-multi-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=master"]);
        git(&repo, &["config", "user.email", "t@t.com"]);
        git(&repo, &["config", "user.name", "T"]);
        fs::write(repo.join("f"), "x").unwrap();
        git(&repo, &["add", "f"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(&repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
        git(&repo, &["push", "-u", "origin", "master"]);
        git(
            &repo,
            &["remote", "add", "upstream", repo.to_str().unwrap()],
        );
        git(&repo, &["checkout", "-b", "feature/up-only"]);
        fs::write(repo.join("g"), "y").unwrap();
        git(&repo, &["add", "g"]);
        git(&repo, &["commit", "-m", "up-only"]);
        git(&repo, &["push", "-u", "upstream", "feature/up-only"]);
        git(&repo, &["checkout", "master"]);

        // When
        let list = list_recent_remote_branches(&repo, "upstream", 10).unwrap();

        // Then
        assert!(
            list.iter().any(|r| r == "upstream/feature/up-only"),
            "expected upstream/feature/up-only in {:?}",
            list
        );
        assert!(
            !list.iter().any(|r| r.starts_with("origin/")),
            "origin/* must not appear when filtering for upstream: {:?}",
            list
        );

        let _ = fs::remove_dir_all(&base);
    }
}
