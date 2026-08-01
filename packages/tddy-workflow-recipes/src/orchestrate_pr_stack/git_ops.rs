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

/// Rebase `branch` onto `new_base`, replacing `old_base` as the fork point.
/// Aborts the rebase on conflict and returns `Err`.
#[allow(dead_code)]
pub fn rebase_onto(
    repo_root: &std::path::Path,
    new_base: &str,
    old_base: &str,
    branch: &str,
) -> Result<(), tddy_core::WorkflowError> {
    // Checkout the branch first
    let checkout = run_git(repo_root, &["checkout", branch])?;
    if !checkout.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git checkout {branch}: {}",
            String::from_utf8_lossy(&checkout.stderr)
        )));
    }

    let rebase = run_git(repo_root, &["rebase", "--onto", new_base, old_base, branch])?;
    if !rebase.status.success() {
        // Abort so the repo is left clean
        let _ = run_git(repo_root, &["rebase", "--abort"]);
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
/// [`tddy_core::worktree::detect_default_remote_name`] and uses it for the fetch and the lease: a
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
/// of `pr_resolve_conflicts`, which deliberately leaves them for an agent that is about to be asked
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
pub fn build_integration_ref(
    _repo_root: &std::path::Path,
    _node_id: &str,
    _parent_branches: &[String],
) -> Result<String, tddy_core::WorkflowError> {
    unimplemented!("build_integration_ref: not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A checkout with an identity and a first commit, plus whatever else a test asks it for.
    ///
    /// The `with_*` methods are the only place these tests run git themselves — a test body states
    /// what the working copy holds and then calls the primitive under test.
    struct WorkingCopy {
        tmp: tempfile::TempDir,
    }

    /// A repository holding two committed files: something for a test to edit, and something for it
    /// to leave alone.
    fn a_working_copy() -> WorkingCopy {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_commit(tmp.path());
        let copy = WorkingCopy { tmp };
        copy.with_a_committed_file("README.md", "the readme\n");
        copy.with_a_committed_file("notes.md", "notes to self\n");
        copy
    }

    impl WorkingCopy {
        fn path(&self) -> &std::path::Path {
            self.tmp.path()
        }

        fn git(&self, args: &[&str]) -> String {
            let out = run_git(self.path(), args).expect("git must run");
            assert!(
                out.status.success(),
                "`git {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }

        fn with_a_committed_file(&self, path: &str, contents: &str) -> &Self {
            std::fs::write(self.path().join(path), contents).unwrap();
            self.git(&["add", path]);
            self.git(&["commit", "--quiet", "-m", &format!("add {path}")]);
            self
        }

        /// A tracked file edited and left unstaged — the outstanding work a pull would have to
        /// commit before it can start.
        fn with_an_unstaged_edit_to(&self, path: &str, contents: &str) -> &Self {
            std::fs::write(self.path().join(path), contents).unwrap();
            self
        }

        /// A tracked file edited and staged — what a child agent that ran `git add` mid-turn leaves
        /// behind, and which must still be staged if the commit is refused.
        fn with_a_staged_edit_to(&self, path: &str, contents: &str) -> &Self {
            std::fs::write(self.path().join(path), contents).unwrap();
            self.git(&["add", path]);
            self
        }

        /// A `pre-commit` hook that refuses every commit — the repository's own verification saying
        /// no, which `--no-verify` is not an option for.
        fn with_a_pre_commit_hook_that_refuses(&self) -> &Self {
            let hook = self.path().join(".git").join("hooks").join("pre-commit");
            std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
            std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            self
        }

        /// A merge stopped on a conflict, left exactly where git stopped: `MERGE_HEAD` present and
        /// the conflicted path unmerged in the index.
        fn with_a_conflicting_merge_in_progress(&self) -> &Self {
            self.git(&["checkout", "--quiet", "-b", "theirs"]);
            self.with_a_committed_file("README.md", "their readme\n");
            self.git(&["checkout", "--quiet", "master"]);
            self.with_a_committed_file("README.md", "our readme\n");
            let merge = run_git(self.path(), &["merge", "--no-edit", "theirs"]).unwrap();
            assert!(
                !merge.status.success(),
                "the fixture must leave a *conflicting* merge behind"
            );
            self
        }

        /// The index locked by another git process — the child agent working in this very worktree,
        /// mid-`git commit`. Nothing that needs to write the index can run until it lets go.
        fn with_the_index_locked_by_another_process(&self) -> &Self {
            let lock =
                std::path::PathBuf::from(self.git(&["rev-parse", "--git-path", "index.lock"]));
            std::fs::write(self.path().join(lock), "").unwrap();
            self
        }

        /// The paths currently staged in the index.
        fn staged_paths(&self) -> Vec<String> {
            self.git(&["diff", "--cached", "--name-only"])
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    fn assert_write_failed<T: std::fmt::Debug>(
        result: Result<T, tddy_core::WorkflowError>,
    ) -> String {
        match result {
            Err(e) => e.to_string(),
            Ok(value) => panic!("expected the call to fail, but it succeeded with {value:?}"),
        }
    }

    /// `a_refused_commit_leaves_nothing_staged` — `git add --update` stages every tracked
    /// modification there is, so a commit the repository refuses must not leave that staging behind.
    #[test]
    fn a_refused_commit_leaves_nothing_staged() {
        // Given — outstanding work in the tree, and a repository that will not accept a commit
        let repo = a_working_copy();
        repo.with_an_unstaged_edit_to("README.md", "the operator's edit\n");
        repo.with_a_pre_commit_hook_that_refuses();

        // When
        let result = commit_all_tracked(repo.path(), "wip: save the operator's edit");

        // Then — the refusal leaves the working copy exactly as it was found
        assert_write_failed(result);
        assert_eq!(repo.staged_paths(), Vec::<String>::new());
    }

    /// `a_refused_commit_leaves_work_that_was_already_staged_staged` — the restore puts the index
    /// back to what it held, which is not the same as emptying it: an agent mid-turn may have staged
    /// its own work deliberately.
    #[test]
    fn a_refused_commit_leaves_work_that_was_already_staged_staged() {
        // Given — an agent has staged one file, the operator has edited another, and the repository
        // will not accept a commit
        let repo = a_working_copy();
        repo.with_a_staged_edit_to("notes.md", "the agent's staged work\n");
        repo.with_an_unstaged_edit_to("README.md", "the operator's edit\n");
        repo.with_a_pre_commit_hook_that_refuses();

        // When
        let result = commit_all_tracked(repo.path(), "wip: save the operator's edit");

        // Then — exactly what was staged before is staged after
        assert_write_failed(result);
        assert_eq!(repo.staged_paths(), vec!["notes.md".to_string()]);
    }

    /// `an_abort_that_cannot_run_is_reported_rather_than_read_as_a_clean_worktree` — both callers of
    /// [`abort_in_progress`] tell their own caller that "nothing was left half-applied", so an abort
    /// that could not run has to be raised. A concurrent agent holding the index lock in the very
    /// worktree the pull is touching is exactly the case this exists for.
    #[test]
    fn an_abort_that_cannot_run_is_reported_rather_than_read_as_a_clean_worktree() {
        // Given — a merge stopped on a conflict, and another git process holding the index lock
        let repo = a_working_copy();
        repo.with_a_conflicting_merge_in_progress();
        repo.with_the_index_locked_by_another_process();

        // When
        let result = abort_in_progress(repo.path(), Sync::Merge);

        // Then — the worktree is still mid-merge, and the failure says so rather than being dropped
        let reason = assert_write_failed(result);
        assert!(
            reason.contains("merge") && reason.contains("needs manual attention"),
            "a failed abort must name the operation still in progress and say the worktree needs \
             attention, was '{reason}'"
        );
    }

    /// `nothing_is_aborted_when_the_operation_never_started` — a `git merge` that refuses to begin
    /// leaves no `MERGE_HEAD`, so there is nothing to abort. Aborting anyway answers "there is no
    /// merge to abort", which would bury the reason the merge was refused under a bogus one.
    #[test]
    fn nothing_is_aborted_when_the_operation_never_started() {
        // Given — a worktree with no merge in progress at all
        let repo = a_working_copy();

        // When
        let result = abort_in_progress(repo.path(), Sync::Merge);

        // Then
        result.expect("an operation that never started leaves nothing to abort");
    }

    fn init_repo_with_commit(dir: &std::path::Path) -> String {
        std::process::Command::new("git")
            .args(["init", "--quiet", "-b", "master"])
            .current_dir(dir)
            .status()
            .expect("git init");
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .status()
            .expect("git config email");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .status()
            .expect("git config name");
        let f = dir.join("file.txt");
        std::fs::write(&f, "initial").expect("write file");
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .status()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "initial"])
            .current_dir(dir)
            .status()
            .expect("git commit");
        // Return HEAD sha
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .expect("git rev-parse");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// `merge_base_returns_common_ancestor_in_real_repo` — given a repo where `branch` and
    /// `master` share an initial commit, `merge_base` must return that commit's SHA.
    #[test]
    fn merge_base_returns_common_ancestor_in_real_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let base_sha = init_repo_with_commit(root);

        // Create a feature branch with one more commit
        std::process::Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(root)
            .status()
            .expect("git checkout feature");
        std::fs::write(root.join("feature.txt"), "feature").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "feature commit"])
            .current_dir(root)
            .status()
            .unwrap();

        // When — merge_base(feature, master) should return the initial commit SHA
        let got = merge_base(root, "feature", "master")
            .expect("merge_base must succeed for branches sharing a common ancestor");

        assert_eq!(
            got.trim(),
            base_sha.trim(),
            "merge_base(feature, master) must return the shared initial commit SHA"
        );
    }

    /// `rebase_onto_succeeds_for_clean_rebase` — a branch with no conflicting changes can be
    /// cleanly rebased onto a new base.
    #[test]
    fn rebase_onto_succeeds_for_clean_rebase() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo_with_commit(root);
        // Record the initial commit as old_base
        let old_base_sha = {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };

        // Create new_base branch (adds base.txt)
        std::process::Command::new("git")
            .args(["checkout", "-b", "new-base"])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("base.txt"), "base change").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "new base change"])
            .current_dir(root)
            .status()
            .unwrap();

        // Create feature branch off old_base (adds feature.txt — no conflict with base.txt)
        std::process::Command::new("git")
            .args(["checkout", "-b", "feature", &old_base_sha])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("feature.txt"), "feature change").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "feature change"])
            .current_dir(root)
            .status()
            .unwrap();

        // When — rebase feature onto new-base, replacing old_base as fork point
        let result = rebase_onto(root, "new-base", &old_base_sha, "feature");

        assert!(
            result.is_ok(),
            "rebase_onto must succeed when there are no conflicts; got: {result:?}"
        );
    }

    /// `rebase_onto_returns_err_and_aborts_on_conflict` — when a rebase produces a conflict,
    /// `rebase_onto` must return `Err` and leave the repo in a clean state (no in-progress rebase).
    #[test]
    fn rebase_onto_returns_err_and_aborts_on_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo_with_commit(root);

        let old_base_sha = {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };

        // new-base changes file.txt (same file as feature will change → conflict)
        std::process::Command::new("git")
            .args(["checkout", "-b", "new-base"])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("file.txt"), "new base version").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "new base version"])
            .current_dir(root)
            .status()
            .unwrap();

        // feature branch also changes file.txt differently → guaranteed conflict
        std::process::Command::new("git")
            .args(["checkout", "-b", "feature", &old_base_sha])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("file.txt"), "feature version").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "feature version"])
            .current_dir(root)
            .status()
            .unwrap();

        // When — rebase with conflict
        let result = rebase_onto(root, "new-base", &old_base_sha, "feature");

        // Then — must return Err
        assert!(
            result.is_err(),
            "rebase_onto must return Err on conflict; got Ok"
        );

        // …and no in-progress rebase must remain (git status clean enough to run git commands)
        let _status_out = std::process::Command::new("git")
            .args(["rebase", "--show-current-patch"])
            .current_dir(root)
            .output();
        // If rebase was properly aborted, `git rebase --show-current-patch` returns non-zero or
        // exits immediately. The real check: .git/rebase-merge must not exist.
        let rebase_merge_dir = root.join(".git").join("rebase-merge");
        let rebase_apply_dir = root.join(".git").join("rebase-apply");
        assert!(
            !rebase_merge_dir.exists() && !rebase_apply_dir.exists(),
            "git rebase state directories must be absent after rebase_onto returns Err; \
             rebase-merge: {}, rebase-apply: {}",
            rebase_merge_dir.exists(),
            rebase_apply_dir.exists()
        );
    }
}
