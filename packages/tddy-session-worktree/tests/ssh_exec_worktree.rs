//! Worktree materialization on an SSH target (no tddy-daemon on T).

use tddy_remote_shell::{ExecOutput, InMemoryShell};
use tddy_session_worktree::setup_worktree_for_session_over_ssh;

#[tokio::test]
async fn setup_worktree_for_session_over_ssh_returns_the_path_on_the_target() {
    // Given a remote shell that answers the worktree script
    let shell = InMemoryShell::new("buildbox");
    shell.answers_when(|_request| Some(ExecOutput::succeeded("")));

    // When — the same clone + git worktree add today's local setup runs, through RemoteShell
    let path = setup_worktree_for_session_over_ssh(
        &shell,
        "git@example.com:org/repo.git",
        "019d105b-ac0f-78d3-9a89-409731145a42",
    )
    .await
    .expect("materialize over SSH must succeed");

    // Then
    assert_eq!(
        path,
        "/home/dev/repo/.worktrees/019d105b-ac0f-78d3-9a89-409731145a42"
    );
}
