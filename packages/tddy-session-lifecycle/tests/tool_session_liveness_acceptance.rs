//! Acceptance tests: a tool session's `tddy-coder` is a process tab of its session — live until
//! the spawn backend holding it reports its exit — and delete and signal reach it through that tab,
//! never through the pid its `.session.yaml` records (`#session-liveness` 3/4).
//!
//! PRD: docs/ft/daemon/session-liveness.md
//!
//! The tool is a stand-in `tddy-coder` started by the real `StartSession` / `ResumeSession`. It
//! records its pid, records an interrupt it receives, and runs until the test releases it. It writes
//! no metadata of its own, so each test records the session the way a real coder does. Where a
//! recorded pid matters it names an **unrelated** process the test keeps running (`sleep`), standing
//! in for the process the OS reissued a dead coder's pid to. The spawns run serially.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    DeleteSessionRequest, ListSessionsRequest, ResumeSessionRequest,
    SessionService as SessionServiceTrait, Signal, SignalSessionRequest, StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

const PROJECT: &str = "11111111-2222-4333-8444-555555555555";
const OWNER: &str = "testuser";

/// A safety net for a spawned or exiting coder to be observed, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(10);

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to spawn as the current user")
}

fn process_is_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

// ---------------------------------------------------------------------------
// The unrelated process a recycled pid now names
// ---------------------------------------------------------------------------

/// A process that has nothing to do with any session, running for the whole test.
struct UnrelatedProcess(Child);

fn an_unrelated_process() -> UnrelatedProcess {
    UnrelatedProcess(
        Command::new("sleep")
            .arg("300")
            .spawn()
            .expect("sleep must start"),
    )
}

impl UnrelatedProcess {
    fn pid(&self) -> u32 {
        self.0.id()
    }

    /// Read from the test's own `Child`, so the answer is about this exact process.
    fn is_running(&mut self) -> bool {
        self.0.try_wait().expect("try_wait on sleep").is_none()
    }
}

impl Drop for UnrelatedProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// ---------------------------------------------------------------------------
// The stand-in tddy-coder
// ---------------------------------------------------------------------------

/// A stand-in `tddy-coder` that writes its pid to `coder.pid`, records an interrupt to
/// `interrupted`, and runs until `release` exists — all in `dir`.
fn a_coder_that_runs_until_released(dir: &Path) -> PathBuf {
    let path = dir.join("fake-tddy-coder.sh");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n\
             echo $$ > \"{dir}/coder.pid\"\n\
             trap 'echo interrupted > \"{dir}/interrupted\"' INT\n\
             until [ -e \"{dir}/release\" ]; do sleep 0.05; done\n",
            dir = dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

// ---------------------------------------------------------------------------
// The daemon
// ---------------------------------------------------------------------------

/// A daemon whose only allowed tool is the stand-in coder, spawning directly (no spawn worker).
fn a_daemon(data_dir: &Path, config_dir: &Path, tool: &Path) -> DaemonSessionHost {
    let config_path = config_dir.join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!(
            r#"
livekit:
  url: ws://127.0.0.1:1
  api_key: dummy-key
  api_secret: dummy-secret
spawn_startup_grace_period_ms: 300
spawn_startup_poll_interval_ms: 10
allowed_tools:
  - path: "{}"
"#,
            tool.display()
        ),
    )
    .unwrap();
    std::fs::write(
        config_dir.join("users.yaml"),
        format!(
            "- github_user: \"{OWNER}\"\n  os_user: \"{}\"\n",
            current_username()
        ),
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    let base = data_dir.to_path_buf();
    DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(base.clone())),
        data_dir.to_path_buf(),
        Arc::new(|token| (token == TEST_TOKEN).then(|| OWNER.to_string())),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(tddy_session_lifecycle::claude_cli_session::CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    )
}

/// Register the one project the tool sessions here start on, with no account assigned.
fn register_the_project(data_dir: &Path, repo: &Path) {
    let projects_dir = data_dir.join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT}\n    name: p\n    git_url: \"\"\n    main_repo_path: {}\n    accounts: []\n",
            repo.display()
        ),
    )
    .unwrap();
}

struct ToolDaemon {
    daemon: DaemonSessionHost,
    data_dir: tempfile::TempDir,
    repo: tempfile::TempDir,
    coder_dir: tempfile::TempDir,
    coder: PathBuf,
    unrelated: UnrelatedProcess,
    _config_dir: tempfile::TempDir,
}

fn a_tool_daemon() -> ToolDaemon {
    let data_dir = tempfile::tempdir().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let coder_dir = tempfile::tempdir().unwrap();
    let coder = a_coder_that_runs_until_released(coder_dir.path());
    register_the_project(data_dir.path(), repo.path());
    let daemon = a_daemon(data_dir.path(), config_dir.path(), &coder);
    ToolDaemon {
        daemon,
        data_dir,
        repo,
        coder_dir,
        coder,
        unrelated: an_unrelated_process(),
        _config_dir: config_dir,
    }
}

/// How the session's `.session.yaml` records its pid.
enum RecordedPid {
    /// The unrelated process's: a recycled pid.
    OfTheUnrelatedProcess,
    None,
}

impl ToolDaemon {
    /// Start a tool session through `StartSession`, and record it on disk the way its coder does.
    async fn starts_a_tool_session(&self, recorded_pid: RecordedPid) -> String {
        let session_id = self
            .daemon
            .start_session(Request::direct(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: PROJECT.to_string(),
                session_type: "tool".to_string(),
                tool_path: self.coder.to_str().unwrap().to_string(),
                ..Default::default()
            }))
            .await
            .expect("the tool session starts")
            .into_inner()
            .session_id;
        self.record_session(&session_id, recorded_pid);
        session_id
    }

    fn record_session(&self, session_id: &str, recorded_pid: RecordedPid) {
        let session_dir = unified_session_dir_path(self.data_dir.path(), session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let builder = a_session_metadata()
            .with_session_id(session_id)
            .with_project_id(PROJECT)
            .with_session_type("tool")
            .with_repo_path(self.repo.path().display().to_string())
            .with_tool(self.coder.display().to_string());
        let metadata = match recorded_pid {
            RecordedPid::OfTheUnrelatedProcess => builder.with_pid(self.unrelated.pid()).build(),
            RecordedPid::None => builder.without_pid().build(),
        };
        write_session_yaml(&session_dir, &metadata);
    }

    async fn resumes(&self, session_id: &str) {
        self.daemon
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the tool session resumes");
    }

    fn coder_record(&self, name: &str) -> PathBuf {
        self.coder_dir.path().join(name)
    }

    /// The pid the running coder recorded (the coder is another process: this waits for it).
    async fn coders_pid(&self) -> u32 {
        let record = self.coder_record("coder.pid");
        eventually("the coder to record its pid", OBSERVE, || {
            std::fs::read_to_string(&record)
                .ok()
                .and_then(|pid| pid.trim().parse().ok())
                .ok_or_else(|| format!("{} not written", record.display()))
        })
        .await
    }

    /// Let every coder started so far exit, and wait until the session lists inactive.
    async fn the_coder_exits(&self, session_id: &str) {
        std::fs::write(self.coder_record("release"), "").unwrap();
        eventually_awaiting("the session to list inactive", OBSERVE, || async {
            match self.is_listed_active(session_id).await {
                false => Ok(()),
                true => Err("the session still lists active".to_string()),
            }
        })
        .await;
    }

    /// A coder started from now on runs until released again.
    fn holds_the_next_coder(&self) {
        std::fs::remove_file(self.coder_record("release")).unwrap();
        let _ = std::fs::remove_file(self.coder_record("coder.pid"));
    }

    async fn is_listed_active(&self, session_id: &str) -> bool {
        self.daemon
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: TEST_TOKEN.to_string(),
                include_peers: false,
            }))
            .await
            .expect("ListSessions must succeed")
            .into_inner()
            .sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .expect("the recorded session is listed")
            .is_active
    }

    async fn deletes(&self, session_id: &str) {
        self.daemon
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("DeleteSession must succeed");
    }

    async fn signals(&self, session_id: &str, signal: Signal) -> Result<(), tddy_rpc::Status> {
        self.daemon
            .signal_session(Request::direct(SignalSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                signal: signal as i32,
                control_token: String::new(),
            }))
            .await
            .map(|_| ())
    }
}

impl Drop for ToolDaemon {
    /// Release any coder a failing test left running.
    fn drop(&mut self) {
        let _ = std::fs::write(self.coder_record("release"), "");
    }
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_running_tool_session_lists_active() {
    // Given a tool session whose coder runs, and whose `.session.yaml` records no pid
    let daemon = a_tool_daemon();
    let session_id = daemon.starts_a_tool_session(RecordedPid::None).await;

    // When the sessions are listed
    let active = daemon.is_listed_active(&session_id).await;

    // Then it is active
    assert!(active, "a running tddy-coder makes its tool session active");
}

#[tokio::test]
#[serial_test::serial]
async fn a_tool_session_whose_tddy_coder_exited_lists_inactive() {
    // Given a running tool session, listed active, whose recorded pid names an unrelated process
    let mut daemon = a_tool_daemon();
    let session_id = daemon
        .starts_a_tool_session(RecordedPid::OfTheUnrelatedProcess)
        .await;
    assert!(daemon.is_listed_active(&session_id).await);

    // When its coder exits
    daemon.the_coder_exits(&session_id).await;

    // Then it lists inactive, though its recorded pid still names a running process
    assert!(!daemon.is_listed_active(&session_id).await);
    assert!(daemon.unrelated.is_running());
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_tool_session_stops_its_tddy_coder_and_spares_its_recorded_pid() {
    // Given a running tool session whose recorded pid names an unrelated process
    let mut daemon = a_tool_daemon();
    let session_id = daemon
        .starts_a_tool_session(RecordedPid::OfTheUnrelatedProcess)
        .await;
    let coder = daemon.coders_pid().await;

    // When the session is deleted
    daemon.deletes(&session_id).await;

    // Then its coder is stopped, and the unrelated process is still running
    eventually("the coder to stop", OBSERVE, || {
        match process_is_running(coder) {
            false => Ok(()),
            true => Err(format!("coder pid {coder} still running")),
        }
    })
    .await;
    assert!(
        daemon.unrelated.is_running(),
        "deleting a session never signals the pid its yaml records"
    );
}

// ---------------------------------------------------------------------------
// Signal
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn signalling_a_running_tool_session_reaches_its_tddy_coder() {
    // Given a running tool session whose recorded pid names an unrelated process
    let mut daemon = a_tool_daemon();
    let session_id = daemon
        .starts_a_tool_session(RecordedPid::OfTheUnrelatedProcess)
        .await;
    daemon.coders_pid().await;

    // When the session is interrupted
    daemon
        .signals(&session_id, Signal::Sigint)
        .await
        .expect("SignalSession reaches a running tddy-coder");

    // Then the coder received it, and the unrelated process did not
    let record = daemon.coder_record("interrupted");
    eventually("the coder to record the interrupt", OBSERVE, || {
        record
            .exists()
            .then_some(())
            .ok_or_else(|| format!("{} not written", record.display()))
    })
    .await;
    assert!(
        daemon.unrelated.is_running(),
        "the recorded pid is never signalled"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn signalling_a_stopped_tool_session_is_refused() {
    // Given a tool session whose coder has exited, its recorded pid naming an unrelated process
    let mut daemon = a_tool_daemon();
    let session_id = daemon
        .starts_a_tool_session(RecordedPid::OfTheUnrelatedProcess)
        .await;
    daemon.the_coder_exits(&session_id).await;

    // When the session is terminated
    let signalled = daemon.signals(&session_id, Signal::Sigterm).await;

    // Then the signal is refused, and the unrelated process is untouched
    let refusal = signalled.expect_err("there is no tddy-coder to signal");
    assert_eq!(refusal.code, Code::FailedPrecondition);
    assert!(
        daemon.unrelated.is_running(),
        "the recorded pid is never signalled"
    );
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_resumed_tool_session_is_live_again() {
    // Given a tool session whose coder has exited
    let daemon = a_tool_daemon();
    let session_id = daemon.starts_a_tool_session(RecordedPid::None).await;
    daemon.the_coder_exits(&session_id).await;
    daemon.holds_the_next_coder();

    // When it is resumed
    daemon.resumes(&session_id).await;

    // Then it lists active again
    assert!(
        daemon.is_listed_active(&session_id).await,
        "the respawned tddy-coder makes the session active"
    );
}
