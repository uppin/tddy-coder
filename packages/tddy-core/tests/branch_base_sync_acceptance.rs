//! PRD acceptance: `docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md` — the
//! branch/base sync probe behind the PR-stack panel's "behind base / conflicts" badge.
//!
//! Pins `tddy_core::base_sync::branch_base_sync`: how a branch is compared against its base
//! (remote-first for the base, local-first for the head, `<remote>/` prefixes normalised off),
//! what the ahead/behind counts and conflicting paths mean, that the probe is read-only — it
//! never touches the working tree, the index or `HEAD` — and that every "could not tell" case
//! is an error rather than a zeroed success that would render as "clean".

use std::path::Path;
use std::process::Command;

use tddy_core::base_sync::{branch_base_sync, BranchBaseSync};

// ---------------------------------------------------------------------------
// Fixture — a real git repository built by a fluent builder. No test body below
// contains a raw `git` invocation.
// ---------------------------------------------------------------------------

/// A fresh repository on an unborn `master`, with an identity configured so commits succeed.
fn a_repo() -> RepoFixture {
    let dir = tempfile::tempdir().expect("a temporary directory for the repository");
    let fixture = RepoFixture {
        dir,
        current_branch: "master".to_string(),
    };
    fixture.git(&["-c", "init.defaultBranch=master", "init", "--quiet"]);
    fixture.git(&["config", "user.email", "fixture@tddy.test"]);
    fixture.git(&["config", "user.name", "Fixture"]);
    fixture.git(&["config", "commit.gpgsign", "false"]);
    fixture
}

/// A directory that was never initialised as a repository.
fn a_directory_that_is_not_a_repository() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory that is not a repository")
}

struct RepoFixture {
    dir: tempfile::TempDir,
    current_branch: String,
}

impl RepoFixture {
    fn root(&self) -> &Path {
        self.dir.path()
    }

    // -- building -----------------------------------------------------------

    /// Commit `contents` into `file` on `branch`, creating the commit on that branch's tip.
    fn with_commit_on(mut self, branch: &str, file: &str, contents: &str) -> Self {
        self.checkout(branch);
        std::fs::write(self.root().join(file), contents)
            .unwrap_or_else(|e| panic!("writing {file} on {branch}: {e}"));
        let message = format!("{branch}: {file}");
        self.git(&["add", file]);
        self.git(&["commit", "--quiet", "-m", &message]);
        self
    }

    /// Branch `new_branch` off the current tip of `start_branch`.
    fn with_branch_from(self, new_branch: &str, start_branch: &str) -> Self {
        self.git(&["branch", new_branch, start_branch]);
        self
    }

    /// Start `branch` from a brand-new root commit, sharing no history with anything else.
    fn with_unrelated_root_commit_on(mut self, branch: &str, file: &str, contents: &str) -> Self {
        self.git(&["switch", "--quiet", "--orphan", branch]);
        self.current_branch = branch.to_string();
        std::fs::write(self.root().join(file), contents)
            .unwrap_or_else(|e| panic!("writing {file} on unrelated root {branch}: {e}"));
        let message = format!("{branch}: unrelated root {file}");
        self.git(&["add", file]);
        self.git(&["commit", "--quiet", "-m", &message]);
        self
    }

    /// Publish `branch` as `refs/remotes/origin/<branch>` at its current tip.
    fn with_origin_tracking(self, branch: &str) -> Self {
        self.with_origin_ref_at(branch, branch)
    }

    /// Point `refs/remotes/origin/<remote_branch>` at `target`, leaving local refs untouched —
    /// this is how the fixture makes a remote branch run ahead of its stale local namesake.
    fn with_origin_ref_at(self, remote_branch: &str, target: &str) -> Self {
        let remote_ref = format!("refs/remotes/origin/{remote_branch}");
        self.git(&["update-ref", &remote_ref, target]);
        self
    }

    /// Delete the local branch, leaving only whatever remote-tracking ref exists for it.
    fn with_local_branch_removed(mut self, branch: &str) -> Self {
        self.checkout("master");
        self.git(&["branch", "-D", branch]);
        self
    }

    /// Leave an untracked file behind, so `git status` has something to report.
    fn with_uncommitted_file(self, file: &str, contents: &str) -> Self {
        std::fs::write(self.root().join(file), contents)
            .unwrap_or_else(|e| panic!("writing uncommitted {file}: {e}"));
        self
    }

    // -- reading ------------------------------------------------------------

    fn status_porcelain(&self) -> String {
        self.git(&["status", "--porcelain"])
    }

    fn head_commit(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    fn unmerged_index_entries(&self) -> String {
        self.git(&["ls-files", "-u"])
    }

    fn has_a_merge_in_progress(&self) -> bool {
        self.root().join(".git").join("MERGE_HEAD").exists()
    }

    // -- plumbing -----------------------------------------------------------

    fn checkout(&mut self, branch: &str) {
        if self.current_branch == branch {
            return;
        }
        self.git(&["checkout", "--quiet", branch]);
        self.current_branch = branch.to_string();
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.root())
            .output()
            .unwrap_or_else(|e| panic!("fixture could not run `git {}`: {e}", args.join(" ")));
        assert!(
            output.status.success(),
            "fixture command `git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("git output must be utf-8")
    }
}

/// The failure message of a comparison that could not be made. Panics if the probe reported
/// success instead — a comparison that could not be made must never look like a clean one.
fn the_failure_message_of(result: Result<BranchBaseSync, String>) -> String {
    match result {
        Err(message) => message,
        Ok(sync) => panic!(
            "a comparison that could not be made must be an error, not a success reading \
             behind={} conflicts={}",
            sync.behind_count, sync.has_conflicts
        ),
    }
}

// ---------------------------------------------------------------------------
// Counts
// ---------------------------------------------------------------------------

#[test]
fn a_branch_that_forked_from_an_unchanged_base_is_ahead_and_not_behind() {
    // Given — a base that has not moved since the branch forked, and two commits on the branch
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "b.txt", "two")
        .with_commit_on("feature/x", "c.txt", "three");

    // When
    let sync = branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing a branch against its unchanged base must succeed");

    // Then
    assert_eq!(
        sync.ahead_count, 2,
        "ahead must count the commits the branch has and the base lacks"
    );
    assert_eq!(
        sync.behind_count, 0,
        "a base that has not moved leaves the branch behind by nothing"
    );
    assert!(
        !sync.has_conflicts,
        "a branch that is not behind cannot conflict with its base"
    );
    assert!(
        sync.conflicted_paths.is_empty(),
        "a branch that is not behind reports no conflicting paths"
    );
}

#[test]
fn a_branch_whose_base_moved_on_is_behind_by_the_commits_it_lacks() {
    // Given — the branch has one commit, the base gained two after the fork
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "b.txt", "two")
        .with_commit_on("master", "c.txt", "three")
        .with_commit_on("master", "d.txt", "four");

    // When
    let sync = branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing a branch against a base that moved on must succeed");

    // Then
    assert_eq!(
        sync.behind_count, 2,
        "behind must count the commits on the base that the branch lacks"
    );
    assert_eq!(
        sync.ahead_count, 1,
        "ahead must count the commits on the branch that the base lacks"
    );
}

// ---------------------------------------------------------------------------
// Conflicts
// ---------------------------------------------------------------------------

#[test]
fn a_branch_that_touched_the_same_lines_as_its_base_reports_the_conflicting_paths() {
    // Given — branch and base edited the same file differently after the fork
    let repo = a_repo()
        .with_commit_on("master", "shared.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "shared.txt", "branch rewrote this line")
        .with_commit_on("master", "shared.txt", "base rewrote this line");

    // When
    let sync = branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing a branch that conflicts with its base must succeed");

    // Then
    assert!(
        sync.has_conflicts,
        "a branch and base that rewrote the same lines must be reported as conflicting"
    );
    assert_eq!(
        sync.conflicted_paths,
        vec!["shared.txt".to_string()],
        "the conflicting paths must name exactly the files that cannot be merged"
    );
}

#[test]
fn a_branch_that_touched_different_files_from_its_base_is_clean_though_behind() {
    // Given — branch and base moved on, but in different files
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "branch-only.txt", "branch work")
        .with_commit_on("master", "base-only.txt", "base work");

    // When
    let sync = branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing a branch that diverged cleanly must succeed");

    // Then
    assert_eq!(
        sync.behind_count, 1,
        "behind must count the commit on the base that the branch lacks"
    );
    assert!(
        !sync.has_conflicts,
        "changes to different files must not be reported as a conflict"
    );
    assert!(
        sync.conflicted_paths.is_empty(),
        "a comparison without conflicts must name no conflicting paths"
    );
}

// ---------------------------------------------------------------------------
// Read-only guarantee
// ---------------------------------------------------------------------------

#[test]
fn the_probe_leaves_the_working_tree_the_index_and_the_head_exactly_as_it_found_them() {
    // Given — a conflicting pair, so the probe has to do the expensive merge work, plus an
    // uncommitted file so the working tree state is something the probe could destroy
    let repo = a_repo()
        .with_commit_on("master", "shared.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "shared.txt", "branch rewrote this line")
        .with_commit_on("master", "shared.txt", "base rewrote this line")
        .with_uncommitted_file("scratch.txt", "work in progress");
    let status_before = repo.status_porcelain();
    let head_before = repo.head_commit();
    let unmerged_before = repo.unmerged_index_entries();

    // When
    branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing a conflicting branch must succeed");

    // Then
    assert_eq!(
        repo.status_porcelain(),
        status_before,
        "the probe must leave the working tree exactly as it found it"
    );
    assert_eq!(
        repo.head_commit(),
        head_before,
        "the probe must leave HEAD exactly as it found it"
    );
    assert_eq!(
        repo.unmerged_index_entries(),
        unmerged_before,
        "the probe must leave the index exactly as it found it"
    );
    assert!(
        !repo.has_a_merge_in_progress(),
        "the probe must not leave a merge in progress behind"
    );
}

// ---------------------------------------------------------------------------
// Ref resolution
// ---------------------------------------------------------------------------

#[test]
fn the_base_is_compared_against_its_remote_tracking_ref_not_a_stale_local_branch() {
    // Given — origin/master carries two commits that the stale local master has never seen
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "b.txt", "two")
        .with_branch_from("published", "master")
        .with_commit_on("published", "c.txt", "three")
        .with_commit_on("published", "d.txt", "four")
        .with_origin_ref_at("master", "published");

    // When
    let sync = branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing against a published base must succeed");

    // Then
    assert_eq!(
        sync.base_ref, "origin/master",
        "the base must resolve to its remote-tracking ref before any local branch of that name"
    );
    assert_eq!(
        sync.behind_count, 2,
        "behind must be measured against the remote base, not the stale local one"
    );
}

#[test]
fn a_base_named_with_its_remote_prefix_resolves_to_the_same_ref_as_without_it() {
    // Given — a published base, named to the probe both ways
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "b.txt", "two")
        .with_commit_on("master", "c.txt", "three")
        .with_origin_tracking("master");

    // When
    let bare = branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing against a bare base name must succeed");
    let prefixed = branch_base_sync(repo.root(), "feature/x", "origin/master")
        .expect("comparing against a remote-prefixed base name must succeed");

    // Then
    assert_eq!(
        prefixed.base_ref, bare.base_ref,
        "a remote prefix on the requested base must be normalised off, not probed twice"
    );
    assert_eq!(
        (prefixed.behind_count, prefixed.ahead_count),
        (bare.behind_count, bare.ahead_count),
        "naming the base with its remote prefix must not change the comparison"
    );
}

#[test]
fn a_branch_that_exists_only_on_origin_is_compared_through_its_remote_ref() {
    // Given — the branch was published and its local copy deleted
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "b.txt", "two")
        .with_origin_tracking("feature/x")
        .with_local_branch_removed("feature/x");

    // When
    let sync = branch_base_sync(repo.root(), "feature/x", "master")
        .expect("comparing a branch that only exists on origin must succeed");

    // Then
    assert_eq!(
        sync.head_ref, "origin/feature/x",
        "a branch with no local ref must be compared through its remote-tracking ref"
    );
}

// ---------------------------------------------------------------------------
// Unavailability — never a zeroed success
// ---------------------------------------------------------------------------

#[test]
fn a_base_that_names_no_ref_is_unavailable_rather_than_clean() {
    // Given — a repository where nothing is called `release/never-created`
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "b.txt", "two");

    // When
    let result = branch_base_sync(repo.root(), "feature/x", "release/never-created");

    // Then
    let message = the_failure_message_of(result);
    assert!(
        message.contains("release/never-created"),
        "the failure must name the base it could not resolve, was '{message}'"
    );
}

#[test]
fn an_unnamed_base_is_unavailable_rather_than_clean() {
    // Given — a caller that supplied no base at all
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_branch_from("feature/x", "master")
        .with_commit_on("feature/x", "b.txt", "two");

    // When
    let result = branch_base_sync(repo.root(), "feature/x", "");

    // Then
    let message = the_failure_message_of(result);
    assert!(
        !message.is_empty(),
        "an empty base name must fail with an explanation, not silently read as clean"
    );
}

#[test]
fn a_path_that_is_not_a_repository_is_unavailable_rather_than_clean() {
    // Given — a directory that was never initialised as a repository
    let not_a_repo = a_directory_that_is_not_a_repository();

    // When
    let result = branch_base_sync(not_a_repo.path(), "feature/x", "master");

    // Then
    let message = the_failure_message_of(result);
    assert!(
        !message.is_empty(),
        "a path that is not a repository must fail with an explanation, not read as clean"
    );
}

#[test]
fn a_branch_with_no_common_ancestor_is_unavailable_rather_than_clean() {
    // Given — two root commits that share no history
    let repo = a_repo()
        .with_commit_on("master", "a.txt", "one")
        .with_unrelated_root_commit_on("feature/x", "b.txt", "two");

    // When
    let result = branch_base_sync(repo.root(), "feature/x", "master");

    // Then
    let message = the_failure_message_of(result);
    assert!(
        !message.is_empty(),
        "unrelated histories must fail with an explanation, not read as zero commits behind"
    );
}
