//! What removing a session's worktree would lose — the reader behind `PreviewCloseSession`
//! (docs/ft/daemon/session-liveness.md § Closing a session), against real repositories: a bare
//! `origin`, and the session's clone on `feat/work`, published and tracking it.
//!
//! The target reader runs the same commands through a remote shell; here the shell is this machine
//! (`LocalShell`), so both readers are asked about one worktree and must agree.

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_remote_shell::{InMemoryShell, LocalShell};
use tddy_worktree_service::worktree_loss::{
    parse_porcelain_status, read_local_worktree_loss, read_target_worktree_loss, WorktreeLoss,
    LISTED_UNPUSHED_COMMITS,
};
use tddy_worktree_service::worktree_sync::{CommitLine, DirtyChange, DirtyFile};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "dev")
        .env("GIT_AUTHOR_EMAIL", "dev@example.com")
        .env("GIT_COMMITTER_NAME", "dev")
        .env("GIT_COMMITTER_EMAIL", "dev@example.com")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A tracked file of the session's branch.
const TRACKED: &str = "src/lib.rs";
/// A second tracked file, for the changes that need one more.
const OTHER_TRACKED: &str = "README.md";

/// The session's clone: `feat/work` pushed to a bare `origin` and tracking it, holding two tracked
/// files and a `.gitignore` that ignores `*.log`.
struct ASessionWorktree {
    _root: tempfile::TempDir,
    path: PathBuf,
}

fn a_session_worktree() -> ASessionWorktree {
    let root = tempfile::tempdir().unwrap();
    let origin = root.path().join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--bare", "-b", "master"]);
    git(root.path(), &["clone", origin.to_str().unwrap(), "session"]);
    let path = root.path().join("session");
    git(&path, &["checkout", "-b", "feat/work"]);
    std::fs::create_dir_all(path.join("src")).unwrap();
    std::fs::write(path.join(TRACKED), "pub fn answer() -> u8 { 42 }\n").unwrap();
    std::fs::write(path.join(OTHER_TRACKED), "# work\n").unwrap();
    std::fs::write(path.join(".gitignore"), "*.log\n").unwrap();
    git(&path, &["add", "."]);
    git(&path, &["commit", "-m", "start the work"]);
    git(&path, &["push", "-u", "origin", "feat/work"]);
    ASessionWorktree { _root: root, path }
}

impl ASessionWorktree {
    fn modifies(&self, file: &str) -> &Self {
        std::fs::write(self.path.join(file), "changed\n").unwrap();
        self
    }

    fn stages_a_change_to(&self, file: &str) -> &Self {
        self.modifies(file);
        git(&self.path, &["add", file]);
        self
    }

    fn adds_an_untracked(&self, file: &str) -> &Self {
        std::fs::write(self.path.join(file), "new\n").unwrap();
        self
    }

    fn deletes(&self, file: &str) -> &Self {
        std::fs::remove_file(self.path.join(file)).unwrap();
        self
    }

    fn renames(&self, from: &str, to: &str) -> &Self {
        git(&self.path, &["mv", from, to]);
        self
    }

    /// Commit a new file `file` with `subject`, and answer the commit's sha.
    fn commits(&self, file: &str, subject: &str) -> String {
        std::fs::write(self.path.join(file), format!("{subject}\n")).unwrap();
        git(&self.path, &["add", file]);
        git(&self.path, &["commit", "-m", subject]);
        git(&self.path, &["rev-parse", "HEAD"])
    }

    /// Commit `count` empty commits, subjects `commit 1` … `commit <count>`.
    fn commits_many(&self, count: usize) {
        for n in 1..=count {
            git(
                &self.path,
                &["commit", "--allow-empty", "-m", &format!("commit {n}")],
            );
        }
    }

    fn pushes(&self) {
        git(&self.path, &["push", "origin", "feat/work"]);
    }

    /// Start a branch `name` at `start`, never published.
    fn switches_to_a_new_branch(&self, name: &str, start: &str) {
        git(&self.path, &["checkout", "-q", "-b", name, start]);
    }

    fn loss(&self) -> WorktreeLoss {
        read_local_worktree_loss(&self.path, None).expect("the worktree reads")
    }

    fn loss_above(&self, base: &str) -> WorktreeLoss {
        read_local_worktree_loss(&self.path, Some(base)).expect("the worktree reads")
    }
}

fn a_dirty(path: &str, change: DirtyChange) -> DirtyFile {
    DirtyFile {
        path: path.to_string(),
        change,
    }
}

fn subjects(lines: &[CommitLine]) -> Vec<&str> {
    lines.iter().map(|line| line.subject.as_str()).collect()
}

// ---------------------------------------------------------------------------
// Uncommitted files
// ---------------------------------------------------------------------------

#[test]
fn a_clean_published_worktree_loses_nothing() {
    // Given
    let worktree = a_session_worktree();

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(loss, WorktreeLoss::default());
    assert!(loss.is_empty());
}

#[test]
fn a_modified_tracked_file_is_reported_as_modified() {
    // Given
    let worktree = a_session_worktree();
    worktree.modifies(TRACKED);

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        loss.dirty_files,
        vec![a_dirty(TRACKED, DirtyChange::Modified)]
    );
}

#[test]
fn a_staged_change_is_reported_as_staged() {
    // Given
    let worktree = a_session_worktree();
    worktree.stages_a_change_to(TRACKED);

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        loss.dirty_files,
        vec![a_dirty(TRACKED, DirtyChange::Staged)]
    );
}

#[test]
fn an_untracked_file_is_reported_as_untracked() {
    // Given
    let worktree = a_session_worktree();
    worktree.adds_an_untracked("notes.md");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        loss.dirty_files,
        vec![a_dirty("notes.md", DirtyChange::Untracked)]
    );
}

#[test]
fn an_untracked_file_in_a_new_directory_is_reported_by_its_own_path() {
    // Given
    let worktree = a_session_worktree();
    std::fs::create_dir_all(worktree.path.join("docs/drafts")).unwrap();
    worktree.adds_an_untracked("docs/drafts/plan.md");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        loss.dirty_files,
        vec![a_dirty("docs/drafts/plan.md", DirtyChange::Untracked)]
    );
}

#[test]
fn a_deleted_tracked_file_is_reported_as_deleted() {
    // Given
    let worktree = a_session_worktree();
    worktree.deletes(OTHER_TRACKED);

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        loss.dirty_files,
        vec![a_dirty(OTHER_TRACKED, DirtyChange::Deleted)]
    );
}

#[test]
fn a_renamed_file_is_reported_once_by_its_new_path() {
    // Given
    let worktree = a_session_worktree();
    worktree.renames(OTHER_TRACKED, "GUIDE.md");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        loss.dirty_files,
        vec![a_dirty("GUIDE.md", DirtyChange::Renamed)]
    );
}

#[test]
fn an_ignored_file_is_not_reported() {
    // Given a worktree whose `.gitignore` ignores `*.log`
    let worktree = a_session_worktree();
    worktree.adds_an_untracked("build.log");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(loss, WorktreeLoss::default());
}

#[test]
fn every_change_of_a_worktree_is_reported_with_its_kind() {
    // Given
    let worktree = a_session_worktree();
    worktree
        .modifies(TRACKED)
        .deletes(OTHER_TRACKED)
        .adds_an_untracked("notes.md")
        .adds_an_untracked("build.log");

    // When
    let mut reported = worktree.loss().dirty_files;
    reported.sort_by(|a, b| a.path.cmp(&b.path));

    // Then
    assert_eq!(
        reported,
        vec![
            a_dirty(OTHER_TRACKED, DirtyChange::Deleted),
            a_dirty("notes.md", DirtyChange::Untracked),
            a_dirty(TRACKED, DirtyChange::Modified),
        ]
    );
}

// ---------------------------------------------------------------------------
// Commits no remote holds
// ---------------------------------------------------------------------------

#[test]
fn a_commit_no_remote_holds_is_reported_with_its_sha_and_subject() {
    // Given
    let worktree = a_session_worktree();
    let sha = worktree.commits("feature.rs", "add the feature");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        (loss.unpushed, loss.unpushed_count, loss.dirty_files),
        (
            vec![CommitLine {
                sha,
                subject: "add the feature".to_string()
            }],
            1,
            Vec::new()
        )
    );
}

#[test]
fn commits_no_remote_holds_are_listed_newest_first() {
    // Given
    let worktree = a_session_worktree();
    worktree.commits("one.rs", "first unpushed");
    worktree.commits("two.rs", "second unpushed");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        subjects(&loss.unpushed),
        vec!["second unpushed", "first unpushed"]
    );
}

#[test]
fn a_commit_pushed_to_a_remote_is_not_a_loss() {
    // Given
    let worktree = a_session_worktree();
    worktree.commits("feature.rs", "add the feature");
    worktree.pushes();

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(loss, WorktreeLoss::default());
}

#[test]
fn a_branch_never_published_loses_only_the_commits_no_remote_holds() {
    // Given a new branch cut from the published one, never pushed itself
    let worktree = a_session_worktree();
    worktree.switches_to_a_new_branch("feat/unpublished", "feat/work");
    worktree.commits("draft.rs", "draft the idea");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        (subjects(&loss.unpushed), loss.unpushed_count),
        (vec!["draft the idea"], 1)
    );
}

#[test]
fn commits_the_recorded_base_holds_are_not_losses() {
    // Given a base commit no remote holds, and a session branch cut from it with work of its own
    let worktree = a_session_worktree();
    let base = worktree.commits("base.rs", "local base work");
    worktree.switches_to_a_new_branch("feat/on-base", &base);
    worktree.commits("session.rs", "session work");

    // When
    let loss = worktree.loss_above(&base);

    // Then
    assert_eq!(
        (subjects(&loss.unpushed), loss.unpushed_count),
        (vec!["session work"], 1)
    );
}

#[test]
fn without_a_recorded_base_the_base_commits_no_remote_holds_are_losses_too() {
    // Given the same branch, its base's commit pushed nowhere
    let worktree = a_session_worktree();
    let base = worktree.commits("base.rs", "local base work");
    worktree.switches_to_a_new_branch("feat/on-base", &base);
    worktree.commits("session.rs", "session work");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        subjects(&loss.unpushed),
        vec!["session work", "local base work"]
    );
}

#[test]
fn more_commits_than_are_listed_are_all_counted() {
    // Given
    let worktree = a_session_worktree();
    let made = LISTED_UNPUSHED_COMMITS + 3;
    worktree.commits_many(made);

    // When
    let loss = worktree.loss();

    // Then the newest are listed, up to the limit, and every one is counted
    assert_eq!(
        (
            loss.unpushed.len(),
            loss.unpushed_count,
            loss.unpushed.first().map(|line| line.subject.clone()),
        ),
        (
            LISTED_UNPUSHED_COMMITS,
            made as u32,
            Some(format!("commit {made}"))
        )
    );
}

#[test]
fn uncommitted_files_and_unpushed_commits_are_reported_together() {
    // Given
    let worktree = a_session_worktree();
    worktree.commits("feature.rs", "add the feature");
    worktree.adds_an_untracked("notes.md");

    // When
    let loss = worktree.loss();

    // Then
    assert_eq!(
        (
            loss.dirty_files,
            subjects(&loss.unpushed),
            loss.unpushed_count
        ),
        (
            vec![a_dirty("notes.md", DirtyChange::Untracked)],
            vec!["add the feature"],
            1
        )
    );
}

#[test]
fn a_directory_that_is_not_a_worktree_cannot_be_read() {
    // Given
    let not_a_repository = tempfile::tempdir().unwrap();

    // When
    let read = read_local_worktree_loss(not_a_repository.path(), None);

    // Then
    assert!(read.is_err(), "{read:?}");
}

// ---------------------------------------------------------------------------
// `git status --porcelain -z` parsing
// ---------------------------------------------------------------------------

#[rstest]
#[case::unstaged_modification(" M src/lib.rs\0", "src/lib.rs", DirtyChange::Modified)]
#[case::staged_and_modified_again("MM src/lib.rs\0", "src/lib.rs", DirtyChange::Modified)]
#[case::staged_modification("M  src/lib.rs\0", "src/lib.rs", DirtyChange::Staged)]
#[case::staged_addition("A  src/new.rs\0", "src/new.rs", DirtyChange::Staged)]
#[case::untracked("?? notes.md\0", "notes.md", DirtyChange::Untracked)]
#[case::unstaged_deletion(" D README.md\0", "README.md", DirtyChange::Deleted)]
#[case::staged_deletion("D  README.md\0", "README.md", DirtyChange::Deleted)]
#[case::path_with_spaces("?? my notes.md\0", "my notes.md", DirtyChange::Untracked)]
fn a_porcelain_entry_is_read_as_its_change(
    #[case] status: &str,
    #[case] path: &str,
    #[case] change: DirtyChange,
) {
    // When
    let files = parse_porcelain_status(status).expect("the status parses");

    // Then
    assert_eq!(files, vec![a_dirty(path, change)]);
}

#[test]
fn a_rename_consumes_the_entry_naming_its_original_path() {
    // Given a rename (new path, then its original) followed by an untracked file
    let status = "R  GUIDE.md\0README.md\0?? notes.md\0";

    // When
    let files = parse_porcelain_status(status).expect("the status parses");

    // Then
    assert_eq!(
        files,
        vec![
            a_dirty("GUIDE.md", DirtyChange::Renamed),
            a_dirty("notes.md", DirtyChange::Untracked),
        ]
    );
}

#[test]
fn an_empty_status_names_no_file() {
    // When
    let files = parse_porcelain_status("").expect("an empty status parses");

    // Then
    assert_eq!(files, Vec::<DirtyFile>::new());
}

#[test]
fn an_entry_too_short_to_hold_a_path_is_an_error() {
    // When
    let parsed = parse_porcelain_status("M\0");

    // Then
    assert!(parsed.is_err(), "{parsed:?}");
}

// ---------------------------------------------------------------------------
// A worktree on a target
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_worktree_read_through_a_shell_loses_what_the_local_reader_says() {
    // Given a worktree with a change of every kind and a commit no remote holds
    let worktree = a_session_worktree();
    worktree.commits("feature.rs", "add the feature");
    worktree
        .modifies(TRACKED)
        .renames(OTHER_TRACKED, "GUIDE.md")
        .adds_an_untracked("notes.md")
        .adds_an_untracked("build.log");

    // When it is read through a shell that reaches it
    let through_a_shell =
        read_target_worktree_loss(&LocalShell::new(), worktree.path.to_str().unwrap(), None)
            .await
            .expect("the worktree reads through the shell");

    // Then the answer is the local reader's
    assert_eq!(through_a_shell, worktree.loss());
}

#[tokio::test]
async fn a_worktree_read_through_a_shell_honours_the_recorded_base() {
    // Given
    let worktree = a_session_worktree();
    let base = worktree.commits("base.rs", "local base work");
    worktree.switches_to_a_new_branch("feat/on-base", &base);
    worktree.commits("session.rs", "session work");

    // When
    let through_a_shell = read_target_worktree_loss(
        &LocalShell::new(),
        worktree.path.to_str().unwrap(),
        Some(&base),
    )
    .await
    .expect("the worktree reads through the shell");

    // Then
    assert_eq!(subjects(&through_a_shell.unpushed), vec!["session work"]);
}

#[tokio::test]
async fn a_clean_worktree_read_through_a_shell_loses_nothing() {
    // Given
    let worktree = a_session_worktree();

    // When
    let through_a_shell =
        read_target_worktree_loss(&LocalShell::new(), worktree.path.to_str().unwrap(), None)
            .await
            .expect("the worktree reads through the shell");

    // Then
    assert_eq!(through_a_shell, WorktreeLoss::default());
}

#[tokio::test]
async fn an_unreachable_target_cannot_be_read() {
    // Given
    let target = InMemoryShell::new("ssh.buildbox");
    target.is_unreachable("Could not resolve hostname buildbox");

    // When
    let read = read_target_worktree_loss(&target, "/home/dev/repo/.worktrees/s-1", None).await;

    // Then the error says why
    let refusal = read.expect_err("an unreachable target is not read as losing nothing");
    assert!(
        refusal
            .message()
            .contains("Could not resolve hostname buildbox"),
        "{}",
        refusal.message()
    );
}

#[tokio::test]
async fn a_target_path_that_is_not_a_worktree_cannot_be_read() {
    // Given
    let not_a_repository = tempfile::tempdir().unwrap();

    // When
    let read = read_target_worktree_loss(
        &LocalShell::new(),
        not_a_repository.path().to_str().unwrap(),
        None,
    )
    .await;

    // Then
    assert!(read.is_err(), "{read:?}");
}
