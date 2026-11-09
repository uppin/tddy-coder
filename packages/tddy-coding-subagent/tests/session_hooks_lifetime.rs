//! A `CodingAgentSession` owns the hooks its launch hands it for exactly its lifetime
//! (`#agent-tools 27/28`): they own whatever the launch prepared for the agent — `codex-acp-login`'s
//! retry-once login, and its open bracket — so they must outlive every turn and go with the session.
//!
//! Drives a real `tddy-acp-stub` child through the ACP driver. Needs `target/debug/tddy-acp-stub`
//! (`cargo build -p tddy-acp-stub`).
//!
//! Changeset: `docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::adapters::PreparedAcpHooks;
use tddy_agent_backend::{AcpAgentCommand, AcpAgentHooks, OpenSession};
use tddy_coding_subagent::CodingAgentSession;

/// How long a dropped session's driver may take to let its hooks go.
const RELEASE_DEADLINE: Duration = Duration::from_secs(10);

/// `tddy-acp-stub` in the target directory cargo builds into.
fn stub_agent_path() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|dir| workspace.join(dir))
        .unwrap_or_else(|| workspace.join("target"));
    let stub = target.join("debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// Hooks that change nothing; the test watches whether anything still holds them.
struct LaunchHooks;

impl AcpAgentHooks for LaunchHooks {}

/// Launch hooks handed to a session, and a watch on whether anything still holds them.
fn some_launch_hooks() -> (PreparedAcpHooks, Weak<LaunchHooks>) {
    let hooks = Arc::new(LaunchHooks);
    let watch = Arc::downgrade(&hooks);
    (PreparedAcpHooks { hooks, opens: None }, watch)
}

/// A session over the stub, started with `hooks`, working in `dir`.
async fn a_session_started_with(
    hooks: PreparedAcpHooks,
    dir: &tempfile::TempDir,
) -> CodingAgentSession {
    let command = AcpAgentCommand {
        program: stub_agent_path(),
        args: Vec::new(),
        env: vec![(
            "TDDY_ACP_STUB_RECORD".to_string(),
            dir.path().join("record.jsonl").display().to_string(),
        )],
        cwd: None,
    };
    let open = OpenSession {
        cwd: dir.path().to_path_buf(),
        ..OpenSession::default()
    };
    CodingAgentSession::start(command, open, "stub-model".to_string(), hooks)
        .await
        .expect("the coding agent starts")
}

/// Wait until nothing holds the hooks `watch` watches, failing past [`RELEASE_DEADLINE`].
async fn wait_until_released(watch: &Weak<LaunchHooks>) {
    tokio::time::timeout(RELEASE_DEADLINE, async {
        while watch.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the dropped session let its hooks go in time");
}

#[tokio::test]
#[serial]
async fn a_started_session_holds_its_launch_hooks_while_it_lives() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let (hooks, watch) = some_launch_hooks();

    // When
    let session = a_session_started_with(hooks, &dir).await;

    // Then
    assert!(
        watch.upgrade().is_some(),
        "a live session must hold the hooks that own what its launch wrote"
    );
    drop(session);
}

#[tokio::test]
#[serial]
async fn a_dropped_session_lets_its_launch_hooks_go() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let (hooks, watch) = some_launch_hooks();
    let session = a_session_started_with(hooks, &dir).await;

    // When
    drop(session);

    // Then
    wait_until_released(&watch).await;
}

/// An open bracket that records each open it was asked to bracket: the session directory each
/// began with, and how many it was asked to end.
#[derive(Default)]
struct RecordingOpenScope {
    begun_with: std::sync::Mutex<Vec<Option<PathBuf>>>,
    ended: std::sync::Mutex<u32>,
}

#[async_trait::async_trait]
impl tddy_agent_backend::adapters::AcpOpenScope for RecordingOpenScope {
    async fn begin_open(&self, session_dir: Option<PathBuf>) {
        self.begun_with.lock().expect("lock").push(session_dir);
    }

    async fn end_open(&self) -> Option<tddy_agent_backend::adapters::AdapterError> {
        *self.ended.lock().expect("lock") += 1;
        None
    }
}

#[tokio::test]
#[serial]
async fn a_session_brackets_its_open_with_its_launch_hooks_open_scope() {
    // Given — launch hooks that bracket each open (as `codex-acp-login`'s do)
    let dir = tempfile::tempdir().expect("tempdir");
    let scope = Arc::new(RecordingOpenScope::default());
    let hooks = PreparedAcpHooks {
        hooks: Arc::new(LaunchHooks),
        opens: Some(scope.clone()),
    };

    // When
    let session = a_session_started_with(hooks, &dir).await;

    // Then — its one `session/new` was bracketed once, begun in the directory the agent works in
    assert_eq!(
        *scope.begun_with.lock().expect("lock"),
        vec![Some(dir.path().to_path_buf())]
    );
    assert_eq!(*scope.ended.lock().expect("lock"), 1);
    drop(session);
}
