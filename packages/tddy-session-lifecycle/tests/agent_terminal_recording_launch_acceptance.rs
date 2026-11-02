//! Acceptance: a launched or resumed claude-cli session's main terminal is recorded to its session
//! directory (`terminals/main.capture`), so its history outlives the process
//! (PRD: docs/ft/daemon/1-WIP/PRD-2026-11-02-agent-terminal-history-persists.md).
//!
//! The agent is a stub that prints and then stays alive, so the periodic write (50 ms here) is what
//! the tests observe: registration happens at launch and at resume, not by hand.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::session_dir_recorded_history::main_capture_recording_path;
use tddy_task::CaptureRecording;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";

/// The OS user the test process runs as — a real, resolvable user (same-user, so the interactive
/// claude-cli spawn needs no privilege drop). Fixtures use this rather than a fabricated name so
/// impersonation resolves during the spawn.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn write_config_with_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
allowed_tools:
  - path: /bin/true
    label: true
claude_cli:
  binary_path: {stub_binary}
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service_with_manager(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver = Arc::new(move |token| {
        if token == VALID_TOKEN {
            Some(resolved_user.clone())
        } else {
            None
        }
    });
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    )
}

fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(dir);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("git command failed");
    };
    let author_env = &[
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "t@t.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "t@t.com"),
    ];
    run(&["init", "-b", "main"], &[]);
    run(&["config", "user.email", "t@t.com"], &[]);
    run(&["config", "user.name", "Test"], &[]);
    run(&["commit", "--allow-empty", "-m", "init"], author_env);
    run(&["remote", "add", "origin", dir.to_str().unwrap()], &[]);
    run(&["push", "-u", "origin", "main"], &[]);
}

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn write_staying_alive_stub(dir: &std::path::Path) -> PathBuf {
    let script_path = dir.join("stub_claude.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\necho STUB-AGENT-OUTPUT\nsleep 60\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    script_path
}

fn a_manager_recording_every_50_ms() -> Arc<ClaudeCliSessionManager> {
    Arc::new(ClaudeCliSessionManager::new().with_recording_interval(Duration::from_millis(50)))
}

/// The recording at `path` once it holds the stub's output.
async fn recording_of_the_stub(path: &std::path::Path) -> CaptureRecording {
    for _ in 0..400 {
        if let Ok(recording) = CaptureRecording::read(path) {
            if String::from_utf8_lossy(&recording.data).contains("STUB-AGENT-OUTPUT") {
                return recording;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "no recording of the stub agent appeared at {}",
        path.display()
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_started_session_records_its_main_terminal() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = write_staying_alive_stub(stub_dir.path());
    let (_cfg_dir, config) = write_config_with_binary(stub.to_str().unwrap());
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        a_manager_recording_every_50_ms(),
    );

    let started = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            ..Default::default()
        }))
        .await
        .expect("StartSession must succeed")
        .into_inner();

    let path = main_capture_recording_path(sessions_tmp.path(), &started.session_id);
    recording_of_the_stub(&path).await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_resumed_session_records_its_main_terminal() {
    let worktree_dir = tempfile::tempdir().unwrap();
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000005";
    let sessions_base = sessions_tmp.path().join(current_os_user());
    let session_dir = sessions_base.join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        updated_at: "2026-06-06T10:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: Some(worktree_dir.path().to_str().unwrap().to_string()),
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: None,
        managed_codebase: false,
        session_role: None,
    };
    write_session_metadata(&session_dir, &meta).unwrap();
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = write_staying_alive_stub(stub_dir.path());
    let (_cfg_dir, config) = write_config_with_binary(stub.to_str().unwrap());
    let service = minimal_service_with_manager(
        config,
        sessions_base.clone(),
        a_manager_recording_every_50_ms(),
    );

    service
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("ResumeSession must succeed");

    let path = main_capture_recording_path(&sessions_base, session_id);
    recording_of_the_stub(&path).await;
}
