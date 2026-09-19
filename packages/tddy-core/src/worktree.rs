//! Git worktree management for daemon sessions.
//!
//! Worktrees are stored in `.worktrees/` relative to the repo root.
//!
//! The `git` plumbing these functions are built on lives in [`tddy_git`] and is re-exported here,
//! so every `tddy_core::worktree::…` path keeps resolving.

use std::path::{Path, PathBuf};

pub use tddy_git::*;

use crate::branch_worktree_intent;
use crate::changeset::{read_changeset, write_changeset, BranchWorktreeIntent};

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
