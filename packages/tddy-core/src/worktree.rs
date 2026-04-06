//! Git worktree management for daemon sessions.
//!
//! Worktrees are stored in `.worktrees/` relative to the repo root.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::branch_worktree_intent;
use crate::changeset::{read_changeset, write_changeset, BranchWorktreeIntent};

/// Default remote-tracking ref used for integration worktrees when a project does not specify
/// `main_branch_ref` in the daemon project registry (legacy YAML rows).
///
/// This matches the historical hardcoded contract (`origin/master`) before per-project base refs.
pub const DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF: &str = "origin/master";

/// Validates a per-project integration base ref: a single remote-tracking ref `origin/<branch>` with
/// no shell metacharacters or extra git arguments.
pub fn validate_integration_base_ref(s: &str) -> Result<(), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("integration base ref must not be empty".to_string());
    }
    let rest = s
        .strip_prefix("origin/")
        .ok_or_else(|| "integration base ref must start with origin/".to_string())?;
    if rest.is_empty() {
        return Err("integration base ref must be origin/<branch-name>".to_string());
    }
    if rest.contains('/') {
        return Err(
            "integration base ref must be a single remote branch segment: origin/<branch-name>"
                .to_string(),
        );
    }
    if rest.chars().any(|c| c.is_whitespace()) {
        return Err("integration base ref must not contain whitespace".to_string());
    }
    for forbidden in [';', '|', '&', '$', '`', '\n', '\r'] {
        if rest.contains(forbidden) {
            return Err(format!(
                "integration base ref contains forbidden character: {:?}",
                forbidden
            ));
        }
    }
    if rest.contains("--") {
        return Err("integration base ref must not contain `--`".to_string());
    }
    Ok(())
}

/// Validates a chain-PR integration base ref: `origin/<branch-path>` where `<branch-path>` may
/// contain `/` (e.g. `origin/feature/foo`). Rejects empty strings, shell metacharacters, and `..`.
pub fn validate_chain_pr_integration_base_ref(s: &str) -> Result<(), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("chain PR integration base ref must not be empty".to_string());
    }
    let rest = s
        .strip_prefix("origin/")
        .ok_or_else(|| "chain PR integration base ref must start with origin/".to_string())?;
    if rest.is_empty() {
        return Err("chain PR integration base ref must be origin/<branch-path>".to_string());
    }
    if rest.contains("..") {
        return Err("chain PR integration base ref must not contain `..`".to_string());
    }
    if rest.contains("--") {
        return Err("chain PR integration base ref must not contain `--`".to_string());
    }
    if rest.chars().any(|c| c.is_whitespace()) {
        return Err("chain PR integration base ref must not contain whitespace".to_string());
    }
    for forbidden in [';', '|', '&', '$', '`', '\n', '\r'] {
        if rest.contains(forbidden) {
            return Err(format!(
                "chain PR integration base ref contains forbidden character: {:?}",
                forbidden
            ));
        }
    }
    for segment in rest.split('/') {
        if segment.is_empty() {
            return Err(
                "chain PR integration base ref must not contain empty path segments".to_string(),
            );
        }
    }
    Ok(())
}

/// Fetches a remote-tracking ref for chain PRs (multi-segment `origin/...` allowed).
fn fetch_chain_pr_integration_base(
    repo_root: &Path,
    integration_base_ref: &str,
) -> Result<(), String> {
    validate_chain_pr_integration_base_ref(integration_base_ref)?;
    let branch_path = integration_base_ref
        .strip_prefix("origin/")
        .expect("validate_chain_pr_integration_base_ref ensures origin/ prefix");
    log::info!(
        "fetch_chain_pr_integration_base: repo={} integration_base_ref={}",
        repo_root.display(),
        integration_base_ref
    );
    let output = Command::new("git")
        .args(["fetch", "origin", branch_path])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git fetch origin {}: {}", branch_path, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::debug!(
            "fetch_chain_pr_integration_base: git fetch failed stderr={}",
            stderr.trim()
        );
        return Err(format!(
            "git fetch origin {} failed: {}",
            branch_path, stderr
        ));
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

/// Fetch origin/master. Must succeed before creating worktree from origin/master.
pub fn fetch_origin_master(repo_root: &Path) -> Result<(), String> {
    log::debug!("fetch_origin_master: repo_root={}", repo_root.display());
    fetch_integration_base(repo_root, DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF)
}

/// Fetches the given remote-tracking integration base ref (e.g. `origin/main`).
pub fn fetch_integration_base(repo_root: &Path, integration_base_ref: &str) -> Result<(), String> {
    validate_integration_base_ref(integration_base_ref)?;
    let branch = integration_base_ref
        .strip_prefix("origin/")
        .expect("validate_integration_base_ref ensures origin/ prefix");
    log::info!(
        "fetch_integration_base: repo={} integration_base_ref={}",
        repo_root.display(),
        integration_base_ref
    );
    let output = Command::new("git")
        .args(["fetch", "origin", branch])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git fetch origin {}: {}", branch, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::debug!(
            "fetch_integration_base: git fetch failed stderr={}",
            stderr.trim()
        );
        return Err(format!("git fetch origin {} failed: {}", branch, stderr));
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
                    let branch_name = wf.selected_branch_to_work_on.clone().ok_or_else(|| {
                        "workflow.selected_branch_to_work_on required for work_on_selected_branch"
                            .to_string()
                    })?;
                    log::info!(
                        "setup_worktree_for_session_with_integration_base: intent=work_on_selected_branch branch={}",
                        branch_name
                    );
                    fetch_integration_base(repo_root, integration_base_ref)?;
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

/// Resolves which remote-tracking ref to use when no per-project override is supplied.
///
/// Runs `git fetch origin`, then prefers `origin/master` when present (legacy default contract),
/// otherwise `origin/main` if present, otherwise follows `refs/remotes/origin/HEAD`.
pub fn resolve_default_integration_base_ref(repo_root: &Path) -> Result<String, String> {
    log::info!(
        "resolve_default_integration_base_ref: fetching origin repo={}",
        repo_root.display()
    );
    let fetch_out = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git fetch origin: {}", e))?;
    if !fetch_out.status.success() {
        let stderr = String::from_utf8_lossy(&fetch_out.stderr);
        return Err(format!("git fetch origin failed: {}", stderr));
    }

    if remote_ref_exists(repo_root, "origin/master")? {
        log::debug!("resolve_default_integration_base_ref: chose origin/master");
        return Ok("origin/master".to_string());
    }
    if remote_ref_exists(repo_root, "origin/main")? {
        log::debug!("resolve_default_integration_base_ref: chose origin/main");
        return Ok("origin/main".to_string());
    }

    let sym = Command::new("git")
        .args(["symbolic-ref", "-q", "refs/remotes/origin/HEAD"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git symbolic-ref: {}", e))?;
    if sym.status.success() {
        let sym_ref = String::from_utf8_lossy(&sym.stdout).trim().to_string();
        log::debug!(
            "resolve_default_integration_base_ref: origin/HEAD -> {}",
            sym_ref
        );
        if let Some(rest) = sym_ref.strip_prefix("refs/remotes/") {
            validate_integration_base_ref(rest)?;
            return Ok(rest.to_string());
        }
    }

    Err(
        "could not resolve integration base ref: no origin/master, origin/main, or origin/HEAD"
            .to_string(),
    )
}

fn remote_ref_exists(repo_root: &Path, rev: &str) -> Result<bool, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", rev])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    Ok(out.status.success())
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
                    let branch_name = wf.selected_branch_to_work_on.clone().ok_or_else(|| {
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

/// Lists remote-tracking branches under `origin/`, most recent commit first, up to `limit` entries.
///
/// Uses `git branch -r --sort=-committerdate`. Excludes `origin/HEAD` and any ref that is not under
/// `origin/`. Entries that fail [`validate_chain_pr_integration_base_ref`] are skipped.
pub fn list_recent_remote_branches(repo_root: &Path, limit: usize) -> Result<Vec<String>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
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
    let mut out: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "origin/HEAD" {
            continue;
        }
        if !line.starts_with("origin/") {
            continue;
        }
        if validate_chain_pr_integration_base_ref(line).is_err() {
            continue;
        }
        if out.iter().any(|e| e == line) {
            continue;
        }
        out.push(line.to_string());
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

    /// Lower-level RED: resolve must read persisted `changeset.yaml` and return stored effective ref.
    #[test]
    fn chain_pr_resolve_persisted_reads_changeset_red() {
        let base = std::env::temp_dir().join("tddy-core-chain-pr-resolve-red");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let session_dir = base.join("session");
        fs::create_dir_all(&session_dir).unwrap();

        let mut cs = crate::changeset::Changeset::default();
        cs.effective_worktree_integration_base_ref = Some("origin/feature/pr-base".to_string());
        cs.worktree_integration_base_ref = Some("origin/feature/pr-base".to_string());
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

        let list = list_recent_remote_branches(&repo, 10).unwrap();
        assert!(
            list.iter()
                .any(|r| r == "origin/main" || r == "origin/feature/a"),
            "expected origin/main or origin/feature/a in {:?}",
            list
        );
        assert!(!list.contains(&"origin/HEAD".to_string()));

        let _ = fs::remove_dir_all(&base);
    }
}
