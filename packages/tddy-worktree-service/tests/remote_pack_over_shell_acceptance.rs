//! A pack verb on a remote checkout is streamed through the remote-shell port — here the in-memory
//! shell, so nothing is spawned.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-shell-port.md

use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_remote_shell::{InMemoryShell, ShellCall};
use tddy_service::proto::remote_git::GitServerFrame;
use tddy_worktree_service::remote_git_service::{GitChildRelay, GitServerFrames};

/// Bounds a relay that never closes its stream. Far above what an in-memory relay needs.
const DRAIN_BUDGET: Duration = Duration::from_secs(10);

/// Everything a completed relay emitted, reassembled in arrival order.
#[derive(Debug, PartialEq)]
struct RelayedStreams {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    done_frames: Vec<GitServerFrame>,
}

/// Drain a relay's frames until the stream closes.
async fn drained(mut frames: GitServerFrames) -> RelayedStreams {
    let mut streams = RelayedStreams {
        stdout: Vec::new(),
        stderr: Vec::new(),
        done_frames: Vec::new(),
    };
    while let Some(frame) = tokio::time::timeout(DRAIN_BUDGET, frames.recv())
        .await
        .expect("the relay closes its stream within the drain budget")
    {
        let frame = frame.expect("the relay must not fault mid-stream");
        streams.stdout.extend_from_slice(&frame.stdout);
        streams.stderr.extend_from_slice(&frame.stderr);
        streams.done_frames.extend(frame.done.then_some(frame));
    }
    streams
}

#[tokio::test]
async fn a_remote_pack_verb_streams_through_the_remote_shell() {
    // Given a remote target where `git upload-pack` on the session's checkout can be streamed
    let shell = InMemoryShell::new("buildbox");
    let upload_pack = shell.streams("git upload-pack -- '/home/dev/repo'");
    let local_argv = ["git", "upload-pack", "--", "/var/tddy/projects/repo"]
        .map(String::from)
        .to_vec();

    // When the verb is spawned through the shell, sent a request, and the target answers and exits
    let (relay, frames) =
        GitChildRelay::spawn_remote_pack_verb(&shell, local_argv, "/home/dev/repo")
            .await
            .expect("the pack verb starts on the target");
    relay
        .send_stdin(b"0032want".to_vec())
        .await
        .expect("the request is written");
    let received = upload_pack.stdin_exact(8).await;
    upload_pack.prints(b"PACK\x00\x00\x00\x02").await;
    upload_pack.complains("Counting objects: 3, done.\n").await;
    upload_pack.ends_with(0).await;
    let relayed = drained(frames).await;

    // Then the target got the request, on the authorized checkout and nowhere else
    assert_eq!(received, b"0032want");
    assert_eq!(
        shell.calls(),
        vec![ShellCall::ExecStreamed(
            "git upload-pack -- '/home/dev/repo'".to_string()
        )]
    );

    // And the pack and the progress came back apart, ended by one done frame with the exit status
    assert_eq!(
        relayed,
        RelayedStreams {
            stdout: b"PACK\x00\x00\x00\x02".to_vec(),
            stderr: b"Counting objects: 3, done.\n".to_vec(),
            done_frames: vec![GitServerFrame {
                exit_code: 0,
                done: true,
                ..Default::default()
            }],
        }
    );
}
