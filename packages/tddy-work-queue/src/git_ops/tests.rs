#![cfg(test)]
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
        let lock = std::path::PathBuf::from(self.git(&["rev-parse", "--git-path", "index.lock"]));
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

fn assert_write_failed<T: std::fmt::Debug>(result: Result<T, tddy_core::WorkflowError>) -> String {
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

// ── rebase in the worktree that holds the branch (`#orchestrators` 10/11) ─────────────────

mod node_worktree_rebase;
