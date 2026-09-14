//! Serve pack verbs through the session's Shell: local `main_repo_path`, or RemoteShell when
//! `ssh_config_host` is set. Clients keep using `tddy-remote-git-repo`.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-09-14-remote-git-ssh.md

use std::path::PathBuf;
use std::time::Duration;

use tddy_rpc::Code;
use tddy_worktree_service::remote_git_service::{
    pack_execution_for_session, resolve_git_verb, GitChildRelay, PackExecution,
};

fn host_path_env() -> Vec<(String, String)> {
    vec![(
        "PATH".to_string(),
        std::env::var("PATH").expect("PATH must be set"),
    )]
}

async fn stdout_of(mut rx: tddy_worktree_service::remote_git_service::GitServerFrames) -> String {
    let mut out = Vec::new();
    while let Ok(Some(frame)) = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
        let frame = frame.expect("relay must not fault");
        out.extend_from_slice(&frame.stdout);
        if frame.done {
            break;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn pack_execution_without_an_alias_stays_local() {
    // Given / When
    let execution = pack_execution_for_session(PathBuf::from("/local/repo"), "", "");

    // Then
    assert_eq!(
        execution,
        PackExecution::Local {
            repo_path: PathBuf::from("/local/repo")
        }
    );
}

#[test]
fn pack_execution_with_an_alias_targets_the_ssh_host() {
    // Given / When
    let execution = pack_execution_for_session(
        PathBuf::from("/local/repo"),
        "buildbox",
        "/home/dev/repo/.worktrees/sess",
    );

    // Then
    assert_eq!(
        execution,
        PackExecution::Remote {
            ssh_config_host: "buildbox".to_string(),
            remote_repo_path: "/home/dev/repo/.worktrees/sess".to_string(),
        }
    );
}

#[tokio::test]
async fn serve_without_ssh_config_host_still_packs_the_local_repo() {
    // Given — today's path: pack verbs run on this host
    let tmp = tempfile::tempdir().expect("tempdir");
    let execution = PackExecution::Local {
        repo_path: tmp.path().to_path_buf(),
    };
    let argv = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "printf local-pack".to_string(),
    ];

    // When
    let (relay, frames) =
        GitChildRelay::spawn_pack_verb(&execution, argv, host_path_env()).expect("local spawn");
    let stdout = stdout_of(frames).await;
    drop(relay);

    // Then
    assert!(
        stdout.contains("local-pack"),
        "unset ssh_config_host must keep packing locally; got {stdout:?}"
    );
}

#[tokio::test]
async fn serve_upload_pack_with_ssh_config_host_packs_the_remote_repo() {
    // Given — the bits live on T, not on this daemon's disk
    let execution = PackExecution::Remote {
        ssh_config_host: "buildbox".to_string(),
        remote_repo_path: "/home/dev/repo/.worktrees/sess".to_string(),
    };
    let argv = vec![
        "git".to_string(),
        "upload-pack".to_string(),
        "--".to_string(),
        "/home/dev/repo/.worktrees/sess".to_string(),
    ];

    // When
    let (relay, frames) =
        GitChildRelay::spawn_pack_verb(&execution, argv, host_path_env()).expect("remote spawn");
    let stdout = stdout_of(frames).await;
    drop(relay);

    // Then — pack advertisement from the SSH target, not this host
    assert!(
        stdout.contains("from-the-ssh-target") || stdout.contains("refs/heads"),
        "upload-pack over RemoteShell must advertise the target repo; got {stdout:?}"
    );
}

#[test]
fn the_verb_whitelist_still_refuses_anything_but_pack_verbs() {
    // When
    let err = resolve_git_verb("git-status").expect_err("status is not a pack verb");

    // Then
    assert_eq!(err.code, Code::PermissionDenied);
    assert!(
        err.message.contains("git-upload-pack"),
        "refusal must name the whitelist; got {}",
        err.message
    );
}
