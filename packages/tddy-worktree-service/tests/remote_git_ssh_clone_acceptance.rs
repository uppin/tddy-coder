//! A specialized-agent clone through `tddy-remote-git-repo` must pack from the SSH target when
//! the code-managing session has `ssh_config_host` set. The client binary is unchanged.
//!
//! Green drives the real `tddy-remote-git-repo` against a fixture sshd. Wave 2 publishes the
//! spawn seam; Remote pack spawn is still `TODO(remote-git)`.

use std::path::PathBuf;

use tddy_worktree_service::remote_git_service::{
    pack_execution_for_session, GitChildRelay, GitVerb, PackExecution,
};

#[tokio::test]
async fn tddy_remote_git_repo_clones_an_ssh_backed_project() {
    // Given — AC37 talks to this daemon's Serve; the objects live on buildbox
    let execution = pack_execution_for_session(
        PathBuf::from("/var/empty"),
        "buildbox",
        "/home/dev/repo/.worktrees/sess",
    );
    assert!(
        matches!(execution, PackExecution::Remote { .. }),
        "a set alias must not pack the daemon's empty local path"
    );
    let argv = vec![
        "git".to_string(),
        GitVerb::UploadPack.git_subcommand().to_string(),
        "--".to_string(),
        "/home/dev/repo/.worktrees/sess".to_string(),
    ];

    // When — the same spawn Serve will use for tddy-remote-git-repo
    let (_relay, _frames) = GitChildRelay::spawn_pack_verb(&execution, argv, Vec::new())
        .expect("upload-pack on the SSH target must spawn so a clone can proceed");
}
