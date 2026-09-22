//! Worktree materialization on an SSH target (no tddy-daemon on T).

use tddy_session_worktree::setup_worktree_for_session_over_ssh;

#[test]
fn setup_worktree_for_session_over_ssh_returns_the_path_on_the_target() {
    // When — the same clone + git worktree add today's local setup runs, through RemoteShell
    let path = setup_worktree_for_session_over_ssh(
        "buildbox",
        "git@example.com:org/repo.git",
        "019d105b-ac0f-78d3-9a89-409731145a42",
    )
    .expect("materialize over SSH must succeed");

    // Then
    assert!(
        path.contains(".worktrees"),
        "the returned path is the worktree on the SSH target; got {path}"
    );
}
