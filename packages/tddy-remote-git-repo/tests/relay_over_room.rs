//! The git relay pipes stdio through a room duplex — see `packages/tddy-room-client/docs/room-client.md`.

use std::io::Cursor;

use pretty_assertions::assert_eq;
use prost::Message as _;
use tddy_remote_git_repo::relay::relay_over_duplex;
use tddy_room_client::in_memory::{a_session_with_one_duplex, DuplexPeer};
use tddy_service::proto::remote_git::{GitClientFrame, GitOpen, GitServerFrame};

const SERVICE: &str = "remote_git.RemoteGitService";
const METHOD: &str = "Serve";

fn the_open_frame() -> GitOpen {
    GitOpen {
        session_token: "a-token".to_string(),
        project_ref: "my-app".to_string(),
        verb: "git-upload-pack".to_string(),
    }
}

/// The remote service: reads what the relay sends until its stdin ends, then answers with `replies`.
/// Returns what it read.
async fn the_remote_service_answering_once_stdin_ends(
    mut peer: DuplexPeer,
    replies: Vec<GitServerFrame>,
) -> Vec<GitClientFrame> {
    let mut received = Vec::new();
    while let Some(bytes) = peer.from_client.recv().await {
        let frame = GitClientFrame::decode(&bytes[..]).expect("the relay sends git frames");
        let stdin_ended = frame.stdin_eof;
        received.push(frame);
        if stdin_ended {
            break;
        }
    }
    for reply in replies {
        peer.to_client
            .send(Ok(reply.encode_to_vec()))
            .await
            .expect("the relay is still listening");
    }
    received
}

#[tokio::test]
async fn the_relay_pipes_stdio_through_a_duplex_to_the_remote_git_service() {
    // Given a duplex to the remote git service, which answers with pack bytes, a warning and an exit
    // status once it has seen the relay's stdin end
    let (session, peer) = a_session_with_one_duplex();
    let duplex = session.open_duplex(SERVICE, METHOD).expect("one duplex");
    let remote = the_remote_service_answering_once_stdin_ends(
        peer,
        vec![
            GitServerFrame {
                stdout: b"PACK".to_vec(),
                ..Default::default()
            },
            GitServerFrame {
                stderr: b"warning: x".to_vec(),
                ..Default::default()
            },
            GitServerFrame {
                done: true,
                exit_code: 7,
                ..Default::default()
            },
        ],
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    // When the relay runs with `want abc` on its stdin
    let (exit, received) = tokio::join!(
        relay_over_duplex(
            duplex,
            the_open_frame(),
            "daemon-host",
            Cursor::new(b"want abc\n".to_vec()),
            &mut stdout,
            &mut stderr,
        ),
        remote,
    );

    // Then the remote saw the open frame, the stdin bytes and the end of stdin, in that order
    assert_eq!(
        received,
        vec![
            GitClientFrame {
                open: Some(the_open_frame()),
                ..Default::default()
            },
            GitClientFrame {
                stdin: b"want abc\n".to_vec(),
                ..Default::default()
            },
            GitClientFrame {
                stdin_eof: true,
                ..Default::default()
            },
        ]
    );
    // And the remote's output reached stdout and stderr, and its exit status is the relay's
    assert_eq!(
        (exit, stdout, stderr),
        (Ok(7), b"PACK".to_vec(), b"warning: x".to_vec())
    );
}

#[tokio::test]
async fn a_remote_that_ends_the_stream_without_an_exit_status_is_reported() {
    // Given a duplex whose far end goes away without a final frame
    let (session, peer) = a_session_with_one_duplex();
    let duplex = session.open_duplex(SERVICE, METHOD).expect("one duplex");
    drop(peer.to_client);

    // When the relay runs
    let outcome = relay_over_duplex(
        duplex,
        the_open_frame(),
        "daemon-host",
        Cursor::new(Vec::new()),
        Vec::new(),
        Vec::new(),
    )
    .await;

    // Then no exit status is invented
    assert_eq!(
        outcome,
        Err(tddy_remote_git_repo::relay::RelayError::StreamEndedWithoutExitStatus)
    );
}
