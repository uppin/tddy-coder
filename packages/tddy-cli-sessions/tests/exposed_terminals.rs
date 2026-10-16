//! The session manager exposes a session's terminal through the `TerminalExposure` port — once,
//! however many reaches arrive together, and again after an exposure has ended. PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-terminal-bridge.md`.
//!
//! The PTY is real (a stub agent that sleeps); the exposure is in memory, so no LiveKit is needed.

use std::path::Path;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::session_participant_metadata::{session_metadata_json, SessionParticipantMetadata};
use tddy_daemon_extensions::{ExposedTerminal, TerminalAddress, TerminalExposure};
use tddy_rpc::Status;
use tokio::sync::oneshot;

const SESSION: &str = "1780828020298-exposed";

/// Records each exposure; each one runs until the test ends it.
#[derive(Default)]
struct RecordingExposure {
    exposed: Mutex<Vec<(TerminalAddress, Option<String>)>>,
    enders: Mutex<Vec<oneshot::Sender<()>>>,
}

impl RecordingExposure {
    fn exposures(&self) -> Vec<(TerminalAddress, Option<String>)> {
        self.exposed.lock().unwrap().clone()
    }

    /// End every running exposure, as a room closing would.
    async fn end_all(&self) {
        for ender in self.enders.lock().unwrap().drain(..) {
            let _ = ender.send(());
        }
        // Let the serving tasks observe it.
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[async_trait::async_trait]
impl TerminalExposure for RecordingExposure {
    async fn expose(
        &self,
        at: &TerminalAddress,
        _terminal: Arc<dyn tddy_rpc::RpcService>,
        metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        self.exposed
            .lock()
            .unwrap()
            .push((at.clone(), metadata_json));
        let (end, ended) = oneshot::channel();
        self.enders.lock().unwrap().push(end);
        Ok(ExposedTerminal::new(tokio::spawn(async move {
            let _ = ended.await;
        })))
    }
}

fn the_address() -> TerminalAddress {
    TerminalAddress {
        room: "tddy-lobby".to_string(),
        identity: format!("daemon-host-{SESSION}"),
    }
}

fn the_metadata_block() -> SessionParticipantMetadata {
    SessionParticipantMetadata {
        agent: "claude".to_string(),
        session_id: SESSION.to_string(),
        ..Default::default()
    }
}

/// A stub agent that holds its PTY open by sleeping.
fn a_sleeping_agent(dir: &Path) -> String {
    let path = dir.join("agent.sh");
    std::fs::write(&path, "#!/bin/sh\nexec sleep 30\n").expect("write the stub agent");
    tddy_testing_commons::stub_scripts::make_executable(&path);
    path.display().to_string()
}

/// A manager running one session whose terminal was recorded as exposable.
async fn a_manager_with_a_recorded_terminal(dir: &Path) -> Arc<CliSessionManager> {
    let manager = Arc::new(CliSessionManager::new());
    manager
        .start(
            SESSION,
            dir.to_path_buf(),
            "claude-opus-4-8",
            &a_sleeping_agent(dir),
            None,
            None,
        )
        .await
        .expect("the stub agent starts under a PTY");
    manager
        .expose_terminal_to_livekit(SESSION, the_metadata_block())
        .await;
    manager
}

#[tokio::test]
async fn a_recorded_terminal_is_exposed_once_however_many_reaches_arrive_together() {
    // Given a session whose terminal was recorded as exposable
    let dir = tempfile::tempdir().unwrap();
    let manager = a_manager_with_a_recorded_terminal(dir.path()).await;
    let exposure = Arc::new(RecordingExposure::default());

    // When two reaches ask for it at the same moment
    let at = the_address();
    let (first, second) = tokio::join!(
        manager.ensure_exposed_terminal(SESSION, &at, &*exposure),
        manager.ensure_exposed_terminal(SESSION, &at, &*exposure),
    );

    // Then both are told it is exposed, and it was exposed exactly once
    assert_eq!((first.unwrap(), second.unwrap()), (true, true));
    assert_eq!(exposure.exposures().len(), 1);
    manager.kill_all().await;
}

#[tokio::test]
async fn a_terminal_whose_exposure_ended_is_exposed_again() {
    // Given a terminal exposed once, whose exposure then ended
    let dir = tempfile::tempdir().unwrap();
    let manager = a_manager_with_a_recorded_terminal(dir.path()).await;
    let exposure = Arc::new(RecordingExposure::default());
    manager
        .ensure_exposed_terminal(SESSION, &the_address(), &*exposure)
        .await
        .unwrap();
    exposure.end_all().await;

    // When it is reached again
    let again = manager
        .ensure_exposed_terminal(SESSION, &the_address(), &*exposure)
        .await
        .unwrap();

    // Then it is exposed a second time
    assert!(again);
    assert_eq!(exposure.exposures().len(), 2);
    manager.kill_all().await;
}

#[tokio::test]
async fn a_session_that_recorded_no_terminal_is_not_exposed() {
    // Given a manager that recorded nothing for this session
    let manager = CliSessionManager::new();
    let exposure = RecordingExposure::default();

    // When it is reached
    let exposed = manager
        .ensure_exposed_terminal(SESSION, &the_address(), &exposure)
        .await
        .unwrap();

    // Then nothing is exposed — and that is not an error
    assert!(!exposed);
    assert_eq!(exposure.exposures(), Vec::new());
}

#[tokio::test]
async fn the_exposure_advertises_the_session_metadata_block_at_the_given_address() {
    // Given a session whose terminal was recorded with its metadata block
    let dir = tempfile::tempdir().unwrap();
    let manager = a_manager_with_a_recorded_terminal(dir.path()).await;
    let exposure = RecordingExposure::default();

    // When it is exposed
    manager
        .ensure_exposed_terminal(SESSION, &the_address(), &exposure)
        .await
        .unwrap();

    // Then the exposure is at that address, advertising exactly that block
    assert_eq!(
        exposure.exposures(),
        vec![(
            the_address(),
            Some(session_metadata_json(&the_metadata_block()))
        )]
    );
    manager.kill_all().await;
}
