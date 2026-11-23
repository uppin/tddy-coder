//! Git operations for orchestrate-pr-stack: rebase, force-push, merge-base, integration refs.

fn run_git(
    repo: &std::path::Path,
    args: &[&str],
) -> Result<std::process::Output, tddy_core::WorkflowError> {
    std::process::Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| {
            tddy_core::WorkflowError::WriteFailed(format!(
                "git {} in {}: {e}",
                args.join(" "),
                repo.display()
            ))
        })
}

/// Whether `branch` exists as a local ref in `repo`. Returns `false` when `repo` isn't a git
/// repository, so remote-only branches (and non-repo scratch dirs) short-circuit git ops cleanly.
pub fn local_branch_exists(repo: &std::path::Path, branch: &str) -> bool {
    run_git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .map(|out| out.status.success())
    .unwrap_or(false)
}

/// Rebase `branch` onto `new_base`, replacing `old_base` as the fork point, **in the worktree of
/// `repo_root` that holds `branch`** (`#orchestrators` 10/11) — a node's branch is checked out in
/// its session's worktree, where git refuses to check it out a second time, and `repo_root`'s own
/// checkout is never switched. A worktree with uncommitted changes is refused naming them, before
/// anything is rebased. Aborts the rebase on conflict and returns `Err`.
#[allow(dead_code)]
pub fn rebase_onto(
    repo_root: &std::path::Path,
    new_base: &str,
    old_base: &str,
    branch: &str,
) -> Result<(), tddy_core::WorkflowError> {
    let worktree = tddy_git::worktree_path_for_branch(repo_root, branch).ok_or_else(|| {
        tddy_core::WorkflowError::WriteFailed(format!(
            "branch '{branch}' has no worktree checked out in {}, so there is nowhere to rebase it",
            repo_root.display()
        ))
    })?;
    let checked_out = tddy_git::checked_out_branch_name(&worktree)
        .map_err(tddy_core::WorkflowError::WriteFailed)?;
    if checked_out.as_deref() != Some(branch) {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "the worktree found for '{branch}' ({}) has '{}' checked out, not '{branch}'",
            worktree.display(),
            checked_out.as_deref().unwrap_or("a detached HEAD")
        )));
    }
    let dirty = worktree_is_clean(&worktree)?;
    if !dirty.is_empty() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "the worktree for '{branch}' ({}) has uncommitted changes to {} — commit or stash them before rebasing",
            worktree.display(),
            dirty.join(", ")
        )));
    }

    let rebase = run_git(&worktree, &["rebase", "--onto", new_base, old_base, branch])?;
    if !rebase.status.success() {
        // Abort so the worktree is left clean
        let _ = run_git(&worktree, &["rebase", "--abort"]);
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git rebase --onto {new_base} {old_base} {branch} failed: {}",
            String::from_utf8_lossy(&rebase.stderr)
        )));
    }
    Ok(())
}

/// Force-push `branch` to `remote`, aborting if `remote` no longer matches `expected_sha`.
///
/// `remote` is a parameter rather than a literal `origin` because the caller resolves it with
/// [`tddy_git::detect_default_remote_name`] and uses it for the fetch and the lease: a
/// clone whose default remote is `upstream` would otherwise take its lease against one remote and
/// push to another.
#[allow(dead_code)]
pub fn force_push_with_lease(
    repo_root: &std::path::Path,
    remote: &str,
    branch: &str,
    expected_sha: &str,
) -> Result<(), tddy_core::WorkflowError> {
    let lease_spec = format!("{branch}:{expected_sha}");
    let out = run_git(
        repo_root,
        &[
            "push",
            &format!("--force-with-lease={lease_spec}"),
            remote,
            branch,
        ],
    )?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git push --force-with-lease={lease_spec} {remote} {branch}: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

/// Compute `git merge-base a b`, returning the common ancestor SHA.
#[allow(dead_code)]
pub fn merge_base(
    repo_root: &std::path::Path,
    a: &str,
    b: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let out = run_git(repo_root, &["merge-base", a, b])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git merge-base {a} {b}: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// What applying a base onto a branch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The branch already contained every commit on the base — nothing was committed.
    AlreadyUpToDate,
    /// The base was applied; the branch's new tip.
    Applied(String),
    /// The base could not be applied. The primitive has already aborted, so the worktree is back
    /// where it started; these are the paths that could not be reconciled.
    Conflicted(Vec<String>),
}

/// Fetch exactly one branch from one remote, updating only that remote-tracking ref.
///
/// Scoped to the ref on purpose: a bare `git fetch <remote>` would also move
/// `refs/remotes/<remote>/<the node's own branch>`, which is the ref a subsequent
/// `--force-with-lease` is taken against — so a wholesale fetch would quietly refresh the lease and
/// turn "somebody else pushed while you were rebasing" from a refused push into a clobbering one.
pub fn fetch_ref(
    dir: &std::path::Path,
    remote: &str,
    branch: &str,
) -> Result<(), tddy_core::WorkflowError> {
    let refspec = format!("refs/heads/{branch}:refs/remotes/{remote}/{branch}");
    let out = run_git(dir, &["fetch", "--quiet", remote, &refspec])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git fetch {remote} {refspec}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// The tracked paths with outstanding changes in `dir` — empty means clean.
///
/// **Tracked only** (`--untracked-files=no`). An untracked file is not outstanding work that a
/// merge or a rebase can lose: git refuses loudly rather than clobbering one. Counting it as
/// dirtiness would leave the pull permanently blocked in any worktree an agent has ever written a
/// scratch file into, which is most of them.
pub fn worktree_is_clean(dir: &std::path::Path) -> Result<Vec<String>, tddy_core::WorkflowError> {
    let out = run_git(dir, &["status", "--porcelain", "--untracked-files=no"])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git status --porcelain in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(porcelain_path)
        .collect())
}

/// The path out of one `git status --porcelain` line: two status characters, a space, then the
/// path — or `<old> -> <new>` for a rename, of which the new name is the one that exists.
fn porcelain_path(line: &str) -> Option<String> {
    let path = line.get(3..)?.trim();
    if path.is_empty() {
        return None;
    }
    Some(
        path.rsplit_once(" -> ")
            .map(|(_, new)| new)
            .unwrap_or(path)
            .trim_matches('"')
            .to_string(),
    )
}

/// Commit every tracked change in `dir` under `message`, returning the new commit.
///
/// `git add --update` necessarily stages **all** tracked modifications, including a child agent's
/// in-flight edits — there is no narrower stage that still captures "whatever is outstanding". So a
/// commit that is refused (a `pre-commit` hook says no, and `--no-verify` is not an option here) must
/// not leave that wholesale staging behind: the index is recorded as a tree beforehand and read back
/// afterwards, which restores exactly what was staged when this was called and touches no file in the
/// working tree.
pub fn commit_all_tracked(
    dir: &std::path::Path,
    message: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let index_before = index_tree(dir)?;
    let add = run_git(dir, &["add", "--update"])?;
    if !add.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git add --update in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&add.stderr).trim()
        )));
    }
    let commit = run_git(dir, &["commit", "--quiet", "-m", message])?;
    if !commit.status.success() {
        let refusal = format!(
            "git commit -m {message:?} in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&commit.stderr).trim()
        );
        return Err(tddy_core::WorkflowError::WriteFailed(
            match restore_index(dir, &index_before) {
                Ok(()) => refusal,
                Err(e) => format!(
                    "{refusal} — and the index could not be restored to what it held before \
                     staging ({e}), so {} needs manual attention",
                    dir.display()
                ),
            },
        ));
    }
    head_sha(dir)
}

/// The index of `dir` written out as a tree object — a snapshot of exactly what is staged.
fn index_tree(dir: &std::path::Path) -> Result<String, tddy_core::WorkflowError> {
    let out = run_git(dir, &["write-tree"])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git write-tree in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Set the index of `dir` back to `tree`, leaving every file in the working tree untouched.
fn restore_index(dir: &std::path::Path, tree: &str) -> Result<(), tddy_core::WorkflowError> {
    let out = run_git(dir, &["read-tree", tree])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git read-tree {tree} in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// Merge `base_ref` into whatever `dir` has checked out, aborting on conflict.
///
/// A conflict leaves nothing behind — no `MERGE_HEAD`, no markers in the tree. That is the opposite
/// of `work_item_resolve_conflicts`, which deliberately leaves them for an agent that is about to be asked
/// to resolve them; here the caller is a dashboard button that may be pressed while an agent is
/// mid-turn in this very worktree, with nobody in scope to resolve anything.
pub fn merge_ref_into_worktree(
    dir: &std::path::Path,
    base_ref: &str,
) -> Result<SyncOutcome, tddy_core::WorkflowError> {
    let before = head_sha(dir)?;
    let merge = run_git(dir, &["merge", "--no-edit", base_ref])?;
    if !merge.status.success() {
        let paths = unmerged_paths(dir)?;
        abort_in_progress(dir, Sync::Merge)?;
        if paths.is_empty() {
            return Err(tddy_core::WorkflowError::WriteFailed(format!(
                "git merge {base_ref} in {} could not start (not a conflict): {}",
                dir.display(),
                String::from_utf8_lossy(&merge.stderr).trim()
            )));
        }
        return Ok(SyncOutcome::Conflicted(paths));
    }

    let after = head_sha(dir)?;
    if after == before {
        return Ok(SyncOutcome::AlreadyUpToDate);
    }
    Ok(SyncOutcome::Applied(after))
}

/// Replay whatever `dir` has checked out on top of `base_ref`, aborting on conflict.
///
/// The abort matters more here than for a merge: a rebase that is left half-finished strands the
/// worktree on a detached `HEAD`, which is a state nothing else in the stack knows how to recover.
pub fn rebase_branch_onto_ref(
    dir: &std::path::Path,
    base_ref: &str,
) -> Result<SyncOutcome, tddy_core::WorkflowError> {
    let before = head_sha(dir)?;
    let rebase = run_git(dir, &["rebase", base_ref])?;
    if !rebase.status.success() {
        let paths = unmerged_paths(dir)?;
        abort_in_progress(dir, Sync::Rebase)?;
        if paths.is_empty() {
            return Err(tddy_core::WorkflowError::WriteFailed(format!(
                "git rebase {base_ref} in {} could not start (not a conflict): {}",
                dir.display(),
                String::from_utf8_lossy(&rebase.stderr).trim()
            )));
        }
        return Ok(SyncOutcome::Conflicted(paths));
    }

    let after = head_sha(dir)?;
    if after == before {
        return Ok(SyncOutcome::AlreadyUpToDate);
    }
    Ok(SyncOutcome::Applied(after))
}

/// Push `branch` to `remote` without rewriting anything on it.
pub fn push_branch(
    dir: &std::path::Path,
    remote: &str,
    branch: &str,
) -> Result<(), tddy_core::WorkflowError> {
    let out = run_git(dir, &["push", "--quiet", remote, branch])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git push {remote} {branch}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// The commit `HEAD` points at in `dir`.
pub fn head_sha(dir: &std::path::Path) -> Result<String, tddy_core::WorkflowError> {
    let out = run_git(dir, &["rev-parse", "HEAD"])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git rev-parse HEAD in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The commit a ref points at, or `None` when it does not resolve.
pub fn ref_sha(dir: &std::path::Path, reference: &str) -> Option<String> {
    let out = run_git(dir, &["rev-parse", "--verify", "--quiet", reference]).ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

/// A sync operation that can be left half-applied in a worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sync {
    Merge,
    Rebase,
}

impl Sync {
    fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }

    /// Whether `dir` still holds the state `--abort` works from: `MERGE_HEAD` for a merge, the
    /// `rebase-merge`/`rebase-apply` directory for a rebase.
    ///
    /// A failure that never started one — `git merge` refusing because local changes would be
    /// overwritten, say — leaves neither, and aborting then reports "there is no merge to abort",
    /// which would bury the real reason under a bogus one.
    fn left_state_in(self, dir: &std::path::Path) -> bool {
        match self {
            Self::Merge => ref_sha(dir, "MERGE_HEAD").is_some(),
            Self::Rebase => ["rebase-merge", "rebase-apply"]
                .iter()
                .any(|state| git_path(dir, state).is_some_and(|p| p.exists())),
        }
    }
}

/// The path `git` would use for `name` inside `dir`'s git directory — which for a linked worktree is
/// `<repo>/.git/worktrees/<name>/…`, not `<dir>/.git/…`.
fn git_path(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let out = run_git(dir, &["rev-parse", "--git-path", name]).ok()?;
    if !out.status.success() {
        return None;
    }
    let printed = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!printed.is_empty()).then(|| dir.join(printed))
}

/// Abort `operation` in `dir` when it left something behind, and say so when the abort fails.
///
/// The exit status is checked rather than discarded because both callers promise their own caller
/// that "nothing was left half-applied". An abort genuinely can fail — `index.lock` held by a
/// concurrent agent working in this very worktree is the case this exists for. Reporting a clean
/// worktree then would be a lie about the one state the operator most needs to know about, so the
/// failure is raised and names the worktree.
fn abort_in_progress(
    dir: &std::path::Path,
    operation: Sync,
) -> Result<(), tddy_core::WorkflowError> {
    if !operation.left_state_in(dir) {
        return Ok(());
    }
    let out = run_git(dir, &[operation.name(), "--abort"])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git {} --abort in {} failed, so the {} is still in progress there and the worktree \
             needs manual attention: {}",
            operation.name(),
            dir.display(),
            operation.name(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// The paths a merge or rebase left unmerged in the index, sorted and deduplicated.
fn unmerged_paths(dir: &std::path::Path) -> Result<Vec<String>, tddy_core::WorkflowError> {
    let out = run_git(dir, &["diff", "--name-only", "--diff-filter=U"])?;
    let mut paths: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

// TODO: implement — octopus merge of parent_branches into stack-int/<node_id>; used for multi-parent DAG nodes
/// Build or refresh a local integration ref (`stack-int/<node_id>`) from multiple parent tips.
/// Returns the SHA of the resulting ref.
#[allow(dead_code)]
pub(crate) fn build_integration_ref(
    _repo_root: &std::path::Path,
    _node_id: &str,
    _parent_branches: &[String],
) -> Result<String, tddy_core::WorkflowError> {
    unimplemented!("build_integration_ref: not yet implemented")
}

#[cfg(test)]
mod tests;
