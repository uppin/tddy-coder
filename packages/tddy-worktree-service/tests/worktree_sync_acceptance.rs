//! A session worktree's sync state and its Pull / Push actions (`#workqueue` 4/14, the inspector's
//! Worktree tab), against real repositories: a bare `origin`, the session's clone, and a second
//! clone standing in for whoever else pushes.

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_worktree_service::worktree_sync::{
    pull_fast_forward, push_branch, read_sync_state, DirtyChange, DirtyFile, SyncStatus,
};

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

/// An `origin` with `feat/work` pushed, the session's worktree on that branch tracking it, and
/// another clone that can push to it.
struct ARepoWorld {
    _root: tempfile::TempDir,
    worktree: PathBuf,
    elsewhere: PathBuf,
}

fn a_repo_world() -> ARepoWorld {
    let root = tempfile::tempdir().unwrap();
    let origin = root.path().join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--bare", "-b", "master"]);
    let worktree = root.path().join("session");
    git(root.path(), &["clone", origin.to_str().unwrap(), "session"]);
    git(&worktree, &["checkout", "-b", "feat/work"]);
    std::fs::write(worktree.join("a.txt"), "a\n").unwrap();
    git(&worktree, &["add", "a.txt"]);
    git(&worktree, &["commit", "-m", "first"]);
    git(&worktree, &["push", "-u", "origin", "feat/work"]);
    git(
        root.path(),
        &[
            "clone",
            "-b",
            "feat/work",
            origin.to_str().unwrap(),
            "elsewhere",
        ],
    );
    let elsewhere = root.path().join("elsewhere");
    ARepoWorld {
        _root: root,
        worktree,
        elsewhere,
    }
}

fn commit(dir: &Path, file: &str, subject: &str) {
    std::fs::write(dir.join(file), format!("{subject}\n")).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", subject]);
}

fn subjects(lines: &[tddy_worktree_service::worktree_sync::CommitLine]) -> Vec<&str> {
    lines.iter().map(|line| line.subject.as_str()).collect()
}

#[test]
fn a_clean_branch_level_with_its_upstream_is_in_sync() {
    // Given
    let world = a_repo_world();

    // When
    let state = read_sync_state(&world.worktree).expect("sync state");

    // Then
    assert_eq!(
        (
            state.status,
            state.upstream.as_deref(),
            state.can_pull(),
            state.can_push()
        ),
        (SyncStatus::InSync, Some("origin/feat/work"), false, false)
    );
}

#[test]
fn local_commits_not_on_the_upstream_are_unpushed_and_listed() {
    // Given
    let world = a_repo_world();
    commit(&world.worktree, "b.txt", "second");
    commit(&world.worktree, "c.txt", "third");

    // When
    let state = read_sync_state(&world.worktree).expect("sync state");

    // Then
    assert_eq!(
        (state.status, subjects(&state.unpushed), state.can_push()),
        (SyncStatus::Unpushed, vec!["second", "third"], true)
    );
}

#[test]
fn upstream_commits_not_local_are_unpulled_and_listed() {
    // Given — someone else pushed to the branch
    let world = a_repo_world();
    commit(&world.elsewhere, "z.txt", "from elsewhere");
    git(&world.elsewhere, &["push"]);

    // When
    let state = read_sync_state(&world.worktree).expect("sync state");

    // Then
    assert_eq!(
        (state.status, subjects(&state.unpulled), state.can_pull()),
        (SyncStatus::Unpulled, vec!["from elsewhere"], true)
    );
}

#[test]
fn both_sides_ahead_is_diverged_and_offers_neither_pull_nor_push() {
    // Given
    let world = a_repo_world();
    commit(&world.elsewhere, "z.txt", "from elsewhere");
    git(&world.elsewhere, &["push"]);
    commit(&world.worktree, "b.txt", "local");

    // When
    let state = read_sync_state(&world.worktree).expect("sync state");

    // Then
    assert_eq!(
        (state.status, state.can_pull(), state.can_push()),
        (SyncStatus::Diverged, false, false)
    );
}

#[test]
fn a_branch_with_no_upstream_is_unpublished_and_offers_push() {
    // Given
    let world = a_repo_world();
    git(&world.worktree, &["checkout", "-b", "feat/new"]);
    commit(&world.worktree, "n.txt", "new work");

    // When
    let state = read_sync_state(&world.worktree).expect("sync state");

    // Then
    assert_eq!(
        (state.status, state.upstream.clone(), state.can_push()),
        (SyncStatus::Unpublished, None, true)
    );
}

#[test]
fn dirty_files_include_modified_staged_and_untracked_paths() {
    // Given
    let world = a_repo_world();
    std::fs::write(world.worktree.join("a.txt"), "changed\n").unwrap();
    std::fs::write(world.worktree.join("staged.txt"), "staged\n").unwrap();
    git(&world.worktree, &["add", "staged.txt"]);
    std::fs::write(world.worktree.join("new.txt"), "untracked\n").unwrap();

    // When
    let mut dirty = read_sync_state(&world.worktree)
        .expect("sync state")
        .dirty_files;
    dirty.sort_by(|a, b| a.path.cmp(&b.path));

    // Then
    assert_eq!(
        dirty,
        vec![
            DirtyFile {
                path: "a.txt".to_string(),
                change: DirtyChange::Modified
            },
            DirtyFile {
                path: "new.txt".to_string(),
                change: DirtyChange::Untracked
            },
            DirtyFile {
                path: "staged.txt".to_string(),
                change: DirtyChange::Staged
            },
        ]
    );
}

#[test]
fn pull_fast_forwards_and_refuses_when_not_fast_forwardable() {
    // Given
    let behind = a_repo_world();
    commit(&behind.elsewhere, "z.txt", "from elsewhere");
    git(&behind.elsewhere, &["push"]);
    let diverged = a_repo_world();
    commit(&diverged.elsewhere, "z.txt", "from elsewhere");
    git(&diverged.elsewhere, &["push"]);
    commit(&diverged.worktree, "b.txt", "local");

    // When
    let pulled = pull_fast_forward(&behind.worktree).expect("a fast-forward pull");
    let refused = pull_fast_forward(&diverged.worktree);

    // Then
    assert_eq!(pulled.status, SyncStatus::InSync);
    assert!(
        refused.is_err(),
        "a diverged branch is not pulled: {refused:?}"
    );
}

#[test]
fn push_publishes_an_unpublished_branch_and_leaves_it_in_sync() {
    // Given
    let world = a_repo_world();
    git(&world.worktree, &["checkout", "-b", "feat/new"]);
    commit(&world.worktree, "n.txt", "new work");

    // When
    let pushed = push_branch(&world.worktree).expect("a push");

    // Then
    assert_eq!(
        (pushed.status, pushed.upstream.as_deref()),
        (SyncStatus::InSync, Some("origin/feat/new"))
    );
}
