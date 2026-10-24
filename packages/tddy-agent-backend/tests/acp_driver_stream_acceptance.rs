//! `AcpDriver::spawn_over` — the driver serving an agent it did not spawn, reached over a stream.
//!
//! What a supervisor relays when it starts a coding agent as another OS user. Here the "relay" is a
//! socket pair with the real `tddy-acp-stub` on its far end, so what is shown is the driver's half:
//! it opens a session and takes a turn over a stream, and ends the agent by closing it.

use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::{AcpDriver, AcpStopReason, NoHooks, OpenSession, PermissionDecision};

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

struct Deny;

#[async_trait::async_trait]
impl tddy_agent_backend::PermissionHandler for Deny {
    async fn decide(&self, _: tddy_agent_backend::PermissionRequest) -> PermissionDecision {
        PermissionDecision::Cancelled
    }
}

/// The stub, running with its stdin and stdout on one end of a socket pair; the other end returned.
fn an_agent_behind_a_stream(dir: &std::path::Path) -> (UnixStream, std::process::Child) {
    let scenario = dir.join("scenario.json");
    std::fs::write(
        &scenario,
        serde_json::json!({ "responses": [{ "chunks": ["over ", "a stream"] }] }).to_string(),
    )
    .expect("write scenario");
    let (ours, theirs) = UnixStream::pair().expect("socket pair");
    let child = Command::new(stub_agent_path())
        .env("TDDY_ACP_SCENARIO", &scenario)
        .stdin(Stdio::from(std::os::fd::OwnedFd::from(
            theirs.try_clone().expect("clone"),
        )))
        .stdout(Stdio::from(std::os::fd::OwnedFd::from(theirs)))
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the stub");
    (ours, child)
}

#[tokio::test]
#[serial]
async fn a_turn_is_taken_over_a_stream_with_no_child_of_the_drivers() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let (stream, mut agent) = an_agent_behind_a_stream(dir.path());
    let driver = AcpDriver::spawn_over(stream, Arc::new(Deny), Arc::new(NoHooks)).expect("driver");
    assert_eq!(driver.agent_pid(), None, "the driver owns no child");

    // When
    let session = driver
        .open_session(OpenSession {
            cwd: dir.path().to_path_buf(),
            ..OpenSession::default()
        })
        .await
        .expect("open session");
    let end = tokio::time::timeout(
        Duration::from_secs(10),
        driver.prompt(&session.session_id, "hi".to_string(), None, None),
    )
    .await
    .expect("the turn ended")
    .expect("the turn succeeds");

    // Then
    assert_eq!(end.stop_reason, AcpStopReason::EndTurn);
    assert_eq!(end.text, "over a stream");
    drop(driver);
    let _ = agent.kill();
    let _ = agent.wait();
}

#[tokio::test]
#[serial]
async fn dropping_the_driver_closes_the_stream_the_agent_reads() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let (stream, mut agent) = an_agent_behind_a_stream(dir.path());
    let driver = AcpDriver::spawn_over(stream, Arc::new(Deny), Arc::new(NoHooks)).expect("driver");
    driver
        .open_session(OpenSession {
            cwd: dir.path().to_path_buf(),
            ..OpenSession::default()
        })
        .await
        .expect("open session");

    // When
    drop(driver);

    // Then the agent saw end of input and exited on its own
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = agent.try_wait().expect("try_wait") {
            break Some(status);
        }
        if std::time::Instant::now() > deadline {
            break None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    if status.is_none() {
        let _ = agent.kill();
        let _ = agent.wait();
    }
    assert!(status.is_some(), "the agent exits when its stream closes");
}
