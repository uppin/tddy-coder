//! The session manager exposes a session's terminal through the `TerminalExposure` port — once,
//! however many reaches arrive together, and again after an exposure has ended. See
//! `packages/tddy-daemon-extensions/docs/extension-ports.md` (the terminal-exposure port).
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

/// An exposure that refuses every request with the status it was given.
struct RefusingExposure(Status);

#[async_trait::async_trait]
impl TerminalExposure for RefusingExposure {
    async fn expose(
        &self,
        _at: &TerminalAddress,
        _terminal: Arc<dyn tddy_rpc::RpcService>,
        _metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        Err(Status {
            code: self.0.code,
            message: self.0.message.clone(),
        })
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
    a_manager_with_recorded_terminals(dir, &[SESSION]).await
}

/// A manager running each of `sessions`, every one with its terminal recorded as exposable.
async fn a_manager_with_recorded_terminals(
    dir: &Path,
    sessions: &[&str],
) -> Arc<CliSessionManager> {
    let manager = Arc::new(CliSessionManager::new());
    for session in sessions {
        manager
            .start(
                session,
                dir.to_path_buf(),
                "claude-opus-4-8",
                &a_sleeping_agent(dir),
                None,
                None,
            )
            .await
            .expect("the stub agent starts under a PTY");
        manager
            .expose_terminal_to_livekit(
                session,
                SessionParticipantMetadata {
                    session_id: session.to_string(),
                    ..the_metadata_block()
                },
            )
            .await;
    }
    manager
}

/// An exposure that completes only once `parties` exposures are running at the same moment — so
/// it can never complete if the manager lets one exposure wait for another.
struct MeetingExposure {
    meeting: tokio::sync::Barrier,
}

#[async_trait::async_trait]
impl TerminalExposure for MeetingExposure {
    async fn expose(
        &self,
        _at: &TerminalAddress,
        _terminal: Arc<dyn tddy_rpc::RpcService>,
        _metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        self.meeting.wait().await;
        Ok(ExposedTerminal::new(tokio::spawn(
            std::future::pending::<()>(),
        )))
    }
}

#[tokio::test]
async fn two_sessions_are_exposed_at_the_same_time() {
    // Given two sessions with recorded terminals, and an exposure that completes only once both
    // of their exposures are running together
    let dir = tempfile::tempdir().unwrap();
    let manager = a_manager_with_recorded_terminals(dir.path(), &["session-a", "session-b"]).await;
    let exposure = MeetingExposure {
        meeting: tokio::sync::Barrier::new(2),
    };
    let at = the_address();

    // When both are reached at the same moment
    let both = async {
        tokio::join!(
            manager.ensure_exposed_terminal("session-a", &at, &exposure),
            manager.ensure_exposed_terminal("session-b", &at, &exposure),
        )
    };
    let (a, b) = tokio::time::timeout(std::time::Duration::from_secs(10), both)
        .await
        .expect("one session's join must not hold up another session's join");

    // Then both are exposed
    assert_eq!((a.unwrap(), b.unwrap()), (true, true));
    manager.kill_all().await;
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

#[tokio::test]
async fn an_exposure_with_nothing_wired_leaves_the_terminal_unexposed_without_failing() {
    // Given a recorded terminal and an exposure that has nothing to expose through
    let dir = tempfile::tempdir().unwrap();
    let manager = a_manager_with_a_recorded_terminal(dir.path()).await;
    let nothing_wired = RefusingExposure(Status::failed_precondition("nothing wired"));

    // When it is reached
    let exposed = manager
        .ensure_exposed_terminal(SESSION, &the_address(), &nothing_wired)
        .await;

    // Then it is simply not exposed
    assert_eq!(exposed.unwrap(), false);
    manager.kill_all().await;
}

#[tokio::test]
async fn an_exposure_that_fails_for_any_other_reason_reports_the_failure() {
    // Given a recorded terminal and an exposure whose server cannot be reached
    let dir = tempfile::tempdir().unwrap();
    let manager = a_manager_with_a_recorded_terminal(dir.path()).await;
    let unreachable = RefusingExposure(Status::internal("server unreachable"));

    // When it is reached
    let refused = manager
        .ensure_exposed_terminal(SESSION, &the_address(), &unreachable)
        .await
        .expect_err("a failure with credentials in hand is reported");

    // Then the caller is told why
    assert_eq!(refused.message, "server unreachable");
    manager.kill_all().await;
}

/// An exposure whose first request is held until the test releases it; every later one completes
/// at once. Each exposure runs until the test ends.
struct HeldFirstExposure {
    entered: Mutex<Option<oneshot::Sender<()>>>,
    release: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
    exposures: std::sync::atomic::AtomicUsize,
}

impl HeldFirstExposure {
    fn new(entered: oneshot::Sender<()>, release: oneshot::Receiver<()>) -> Self {
        Self {
            entered: Mutex::new(Some(entered)),
            release: tokio::sync::Mutex::new(Some(release)),
            exposures: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn exposures(&self) -> usize {
        self.exposures.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl TerminalExposure for HeldFirstExposure {
    async fn expose(
        &self,
        _at: &TerminalAddress,
        _terminal: Arc<dyn tddy_rpc::RpcService>,
        _metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        self.exposures
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let entered = self.entered.lock().unwrap().take();
        if let Some(entered) = entered {
            let _ = entered.send(());
            if let Some(release) = self.release.lock().await.take() {
                let _ = release.await;
            }
        }
        Ok(ExposedTerminal::new(tokio::spawn(
            std::future::pending::<()>(),
        )))
    }
}

#[tokio::test]
async fn a_terminal_recorded_again_while_a_reach_is_in_flight_is_exposed_afresh() {
    // Given a reach to a recorded terminal whose exposure is still joining
    let dir = tempfile::tempdir().unwrap();
    let manager = a_manager_with_a_recorded_terminal(dir.path()).await;
    let (entered, joining) = oneshot::channel();
    let (release, held) = oneshot::channel();
    let exposure = Arc::new(HeldFirstExposure::new(entered, held));
    let first_reach = tokio::spawn({
        let (manager, exposure) = (Arc::clone(&manager), Arc::clone(&exposure));
        async move {
            manager
                .ensure_exposed_terminal(SESSION, &the_address(), &*exposure)
                .await
        }
    });
    joining.await.expect("the first reach starts its exposure");

    // When the session records its terminal again (a relaunch under the same id) before that
    // join completes, and the record is reached once the join has
    let recorded_again = tokio::spawn({
        let manager = Arc::clone(&manager);
        async move {
            manager
                .expose_terminal_to_livekit(SESSION, the_metadata_block())
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _ = release.send(());
    assert!(first_reach.await.unwrap().unwrap());
    recorded_again.await.unwrap();
    let again = manager
        .ensure_exposed_terminal(SESSION, &the_address(), &*exposure)
        .await
        .unwrap();

    // Then the new record is exposed itself, not answered by the old record's exposure
    assert!(again);
    assert_eq!(exposure.exposures(), 2);
    manager.kill_all().await;
}
