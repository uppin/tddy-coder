//! Acceptance: sandboxed session lifecycle — delete stops child + removes worktree; resume re-dials.

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    DeleteSessionRequest, ResumeSessionRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::process_is_alive;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "sandbox-lifecycle-project";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn tddy_tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

fn write_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: /bin/true
    label: true
"#;
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    (
        dir,
        DaemonConfig::load(&config_path).expect("config must parse"),
    )
}

fn minimal_service(config: DaemonConfig, sessions_base: PathBuf) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
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
        Arc::new(CliSessionManager::new()),
    )
}

/// [`minimal_service`] with the daemon's coding agents installed, as the runtime installs them: a
/// sandboxed `claude-cli` session is launched from the `claude` tool definition, which resolves the
/// jail's `tddy-tools` on a `PATH` of `<sessions_base>/stub-bin` alone. `claude` itself stays the
/// the stub passed in. A sandboxed launch reaches the jail over the sandbox RPC
/// bridge, which the daemon runtime installs at mount.
fn a_service_with_coding_agents(
    config: DaemonConfig,
    sessions_base: PathBuf,
    stub: &std::path::Path,
) -> Arc<DaemonSessionHost> {
    let stub_bin = sessions_base.join("stub-bin");
    std::fs::create_dir_all(&stub_bin).unwrap();
    let stub_bin = std::fs::canonicalize(stub_bin).unwrap();
    std::os::unix::fs::symlink(tddy_tools_binary(), stub_bin.join("tddy-tools")).unwrap();
    // The stub stands for `claude` on the declared `PATH`.
    std::os::unix::fs::symlink(stub, stub_bin.join("claude")).unwrap();
    let registry = tddy_coding_agent_specs::load_agent_registry(&sessions_base.join("tools"))
        .expect("the agent registry loads");
    let host = Arc::new(
        minimal_service(config, sessions_base).with_coding_agents(Arc::new(
            tddy_agent_launch::spec_launch::HostCodingAgents::new(
                registry,
                Arc::new(APathHost(vec![stub_bin])),
            ),
        )),
    );
    host.install_sandbox_rpc_bridge();
    host
}

/// A tool host with a declared `PATH` over the real filesystem: nothing of the real machine's
/// `PATH` or `TDDY_*_BINARY` variables reaches resolution.
struct APathHost(Vec<PathBuf>);

impl tddy_actions::tool::ToolHost for APathHost {
    fn env_var(&self, _name: &str) -> Option<String> {
        None
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.0.clone()
    }

    fn is_executable_file(&self, path: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &std::path::Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
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
        "projects:\n  - project_id: {}\n    name: lifecycle\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn write_stub_claude(dir: &std::path::Path) -> std::path::PathBuf {
    let script = dir.join("stub_claude.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 3600\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

fn sandbox_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        tool_path: String::new(),
        project_id: TEST_PROJECT_ID.to_string(),
        agent: String::new(),
        daemon_instance_id: String::new(),
        recipe: String::new(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        branch_worktree_intent: String::new(),
        new_branch_name: String::new(),
        selected_integration_base_ref: String::new(),
        selected_branch_to_work_on: String::new(),
        initial_prompt: String::new(),
        permission_mode: String::new(),
        stack_parent: String::new(),
        sandbox: true,
        managed_codebase: false,
        specialized_agents: vec![],
        ..Default::default()
    }
}

/// **delete_sandbox_session_stops_child_and_removes_directory**: `DeleteSession` terminates the
/// sandbox-exec child and removes the session directory including the git switch metadata path.
#[tokio::test]
async fn delete_sandbox_session_stops_child_and_removes_directory() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_stub_claude(repo_dir.path());
    let (_cfg_dir, config) = write_config();
    let service = a_service_with_coding_agents(config, sessions_tmp.path().to_path_buf(), &stub);

    let resp = service
        .start_session(Request::direct(sandbox_start_request()))
        .await
        .expect("StartSession");
    let session_id = resp.into_inner().session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let meta = read_session_metadata(&session_dir).expect("metadata");
    let pid = meta.pid.expect("pid recorded");

    // When
    service
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession must succeed");

    // Then
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!session_dir.exists(), "session directory must be removed");
    assert!(
        !process_is_alive(pid),
        "sandbox child pid must be terminated"
    );
}

/// **resume_sandbox_session_respawns_and_updates_pid**: `ResumeSession` re-spawns the sandbox
/// runner and records a new active pid in `.session.yaml`.
#[tokio::test]
async fn resume_sandbox_session_respawns_and_updates_pid() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_stub_claude(repo_dir.path());
    let (_cfg_dir, config) = write_config();
    let service = a_service_with_coding_agents(config, sessions_tmp.path().to_path_buf(), &stub);

    let session_id = service
        .start_session(Request::direct(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let old_pid = read_session_metadata(&session_dir)
        .expect("metadata")
        .pid
        .expect("pid");

    // When
    service
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ResumeSession must succeed");

    // Then
    let new_meta = read_session_metadata(&session_dir).expect("metadata after resume");
    let new_pid = new_meta.pid.expect("new pid");
    assert_ne!(old_pid, new_pid, "resume must spawn a fresh sandbox child");
    assert!(process_is_alive(new_pid));
}
