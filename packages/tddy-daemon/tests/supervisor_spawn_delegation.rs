//! What the daemon asks `tddy-supervisor` to run, and what it does when the supervisor it was told
//! about is not there.
//!
//! Changeset: `docs/dev/1-WIP/CS-2026-08-02-tddy-supervisor.md` (Milestone 6).
//!
//! The child's identity — program, argv, session id, LiveKit room, gRPC port — is decided once, by
//! `spawner::plan_session_child`, and started either by the forked spawn worker or by the
//! supervisor. These tests pin that plan and the request it becomes, plus the fail-closed rule: a
//! declared supervisor that cannot be reached fails the operation instead of quietly spawning the
//! session as the daemon's own user.
//!
//! Not covered here, because it needs root and a second OS account: that a supervisor-spawned
//! session actually runs as another user. That stays operator smoke.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon::multi_host::{EligibleDaemonSource, StubEligibleDaemonSource};
use tddy_daemon::spawner::{self, LiveKitCreds, SpawnOptions};
use tddy_daemon::supervisor_client::{spawn_worker_for, SpawnBackendChoice};
use tddy_daemon::supervisor_spawn;
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    AddProjectToHostRequest, ConnectionService as ConnectionServiceTrait, StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to resolve the target account")
}

fn a_livekit() -> LiveKitCreds {
    LiveKitCreds {
        url: "ws://127.0.0.1:7880".to_string(),
        api_key: "test-key".to_string(),
        api_secret: "test-secret".to_string(),
        common_room: None,
        daemon_instance_id: None,
    }
}

/// A long-running stand-in for `tddy-coder`: it ignores every flag the spawner appends, so a plan
/// can be computed (and, in the fail-closed tests, so a spawn that must not happen would leave a
/// trace if it did).
fn a_tool_that_stays_alive(dir: &Path, marker: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-tddy-coder.sh");
    std::fs::write(
        &path,
        format!("#!/bin/sh\ntouch \"{}\"\nsleep 5\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

// ---------------------------------------------------------------------------
// One place decides what the child is
// ---------------------------------------------------------------------------

#[test]
fn plans_the_argv_a_new_session_is_started_with() {
    // Given a new session with a fixed id, so the plan is fully determined
    let repo = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = a_tool_that_stays_alive(tools.path(), &tools.path().join("started"));

    // When
    let plan = spawner::plan_session_child(
        &current_username(),
        tool.to_str().unwrap(),
        data_dir.path(),
        repo.path(),
        &a_livekit(),
        SpawnOptions {
            new_session_id: Some("session-a"),
            project_id: Some("project-a"),
            mouse: true,
            ..Default::default()
        },
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
    )
    .expect("plan the session child");

    // Then the program is the resolved tool and the argv is exactly what the child receives
    assert_eq!(plan.program, tool);
    assert_eq!(
        plan.args,
        vec![
            "--daemon".to_string(),
            "--grpc".to_string(),
            plan.grpc_port.to_string(),
            "--livekit-url".to_string(),
            "ws://127.0.0.1:7880".to_string(),
            "--livekit-api-key".to_string(),
            "test-key".to_string(),
            "--livekit-api-secret".to_string(),
            "test-secret".to_string(),
            "--livekit-room".to_string(),
            "daemon-session-a".to_string(),
            "--livekit-identity".to_string(),
            "daemon-session-a".to_string(),
            "--tddy-data-dir".to_string(),
            data_dir.path().display().to_string(),
            "--session-id".to_string(),
            "session-a".to_string(),
            "--project-id".to_string(),
            "project-a".to_string(),
            "--mouse".to_string(),
            "--config".to_string(),
            repo.path()
                .join("tmp/logs/child/session-a.yaml")
                .display()
                .to_string(),
        ]
    );
    assert_eq!(plan.session_id, "session-a");
    assert_eq!(plan.livekit_room, "daemon-session-a");
    assert_eq!(plan.livekit_server_identity, "daemon-session-a");
    assert_eq!(plan.working_dir, repo.path());
}

#[test]
fn plans_a_resumed_session_to_reopen_its_own_id_rather_than_start_a_new_one() {
    // Given a resume of an existing session
    let repo = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = a_tool_that_stays_alive(tools.path(), &tools.path().join("started"));

    // When
    let plan = spawner::plan_session_child(
        &current_username(),
        tool.to_str().unwrap(),
        data_dir.path(),
        repo.path(),
        &a_livekit(),
        SpawnOptions {
            resume_session_id: Some("session-b"),
            ..Default::default()
        },
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
    )
    .expect("plan the session child");

    // Then the child is told to resume, and is never given a --session-id to create
    assert!(
        plan.args
            .windows(2)
            .any(|pair| pair == ["--resume-from".to_string(), "session-b".to_string()]),
        "a resumed session must be started with --resume-from, got: {:?}",
        plan.args
    );
    assert!(
        !plan.args.iter().any(|arg| arg == "--session-id"),
        "a resumed session must not also be given --session-id, got: {:?}",
        plan.args
    );
    assert_eq!(plan.session_id, "session-b");
}

// ---------------------------------------------------------------------------
// What the supervisor is asked for
// ---------------------------------------------------------------------------

#[test]
fn asks_the_supervisor_to_run_the_planned_program_and_argv_as_the_session_user() {
    // Given a planned child
    let program = PathBuf::from("/usr/bin/tddy-coder");
    let args = vec!["--daemon".to_string(), "--mouse".to_string()];

    // When
    let request = supervisor_spawn::spawn_session_request(
        "alice",
        &program,
        &args,
        Path::new("/srv/tddy/repos/alice/project"),
        None,
    );

    // Then the supervisor decides nothing about the child except that it may run
    assert_eq!(
        request,
        tddy_supervisor::request::SpawnSessionRequest {
            os_user: "alice".to_string(),
            tool_path: program,
            args,
            env: std::collections::BTreeMap::new(),
            working_dir: Some(PathBuf::from("/srv/tddy/repos/alice/project")),
            scope: None,
        }
    );
}

#[test]
fn asks_for_no_environment_at_all_so_an_empty_env_policy_still_permits_the_spawn() {
    // Given a target user whose `~/.tddy/config.yaml` asks for no PATH prefix
    let no_path_extra = None;

    // When
    let request = supervisor_spawn::spawn_session_request(
        "alice",
        Path::new("/usr/bin/tddy-coder"),
        &[],
        Path::new("/srv/tddy/repos/alice/project"),
        no_path_extra,
    );

    // Then nothing is named that `allowed_env_keys` would have to list — the shipped policy grants
    // no keys, and a request naming one it does not list is refused outright rather than stripped.
    assert!(
        request.env.is_empty(),
        "an unremarkable session spawn must name no environment variable, got: {:?}",
        request.env
    );
}

#[test]
fn asks_for_the_path_prefix_a_users_spawn_path_extra_declares() {
    // Given a target user whose `~/.tddy/config.yaml` prepends a toolchain to the child's PATH
    let path_extra = Some("/opt/toolchain/bin");

    // When
    let request = supervisor_spawn::spawn_session_request(
        "alice",
        Path::new("/usr/bin/tddy-coder"),
        &[],
        Path::new("/srv/tddy/repos/alice/project"),
        path_extra,
    );

    // Then PATH is the one variable asked for — which an operator must list in
    // `allowed_env_keys`, or the supervisor refuses the spawn rather than dropping the prefix.
    assert_eq!(
        request.env.keys().collect::<Vec<_>>(),
        vec!["PATH"],
        "only PATH may be requested, got: {:?}",
        request.env
    );
    assert!(
        request.env["PATH"].starts_with("/opt/toolchain/bin:"),
        "the user's prefix must come first in the child's PATH, got: {:?}",
        request.env["PATH"]
    );
}

#[test]
fn asks_the_supervisor_to_clone_a_repository_as_the_target_user() {
    // Given
    let git = PathBuf::from("/usr/bin/git");

    // When
    let request = supervisor_spawn::clone_request(
        "alice",
        &git,
        "git@github.com:owner/repo.git",
        Path::new("/srv/tddy/repos/alice/repo"),
    );

    // Then the clone is a plain allowlisted tool run — the tool path an operator must list in
    // `allowed_tool_paths` is git's own, not a shell's.
    assert_eq!(
        request,
        tddy_supervisor::request::SpawnSessionRequest {
            os_user: "alice".to_string(),
            tool_path: git,
            args: vec![
                "clone".to_string(),
                "git@github.com:owner/repo.git".to_string(),
                "/srv/tddy/repos/alice/repo".to_string(),
            ],
            env: std::collections::BTreeMap::new(),
            working_dir: None,
            scope: None,
        }
    );
}

#[test]
fn names_the_git_the_daemons_own_path_resolves_so_an_operator_can_allowlist_it() {
    // Given a PATH whose second entry is the only one holding a git
    let empty = tempfile::tempdir().unwrap();
    let with_git = tempfile::tempdir().unwrap();
    let git = with_git.path().join("git");
    std::fs::write(&git, "#!/bin/sh\nexit 0\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path_var = format!("{}:{}", empty.path().display(), with_git.path().display());

    // When
    let resolved = supervisor_spawn::program_in_path("git", &path_var);

    // Then
    assert_eq!(resolved, Some(git));
}

#[test]
fn finds_no_git_to_name_when_the_daemons_path_holds_none() {
    // Given a PATH with no git anywhere on it
    let empty = tempfile::tempdir().unwrap();

    // When
    let resolved = supervisor_spawn::program_in_path("git", &empty.path().display().to_string());

    // Then the caller has nothing to ask the supervisor to run, and must say so
    assert_eq!(resolved, None);
}

// ---------------------------------------------------------------------------
// A supervised daemon keeps no privileged machinery of its own
// ---------------------------------------------------------------------------

#[test]
fn forks_no_spawn_worker_when_a_supervisor_does_the_spawning() {
    // Given a host whose config declares a supervisor
    let choice = SpawnBackendChoice::Supervisor {
        socket_path: PathBuf::from("/run/tddy-supervisor.sock"),
    };

    // When
    let worker = spawn_worker_for(&choice).expect("deciding not to fork cannot fail");

    // Then the pre-tokio fork is not taken at all: the supervisor is the only thing that spawns
    assert!(
        worker.is_none(),
        "a supervised daemon must not fork a spawn worker"
    );
}

// ---------------------------------------------------------------------------
// Fail closed
// ---------------------------------------------------------------------------

fn a_supervised_config(os_user: &str, repos_base: &Path, missing_socket: &Path) -> DaemonConfig {
    let yaml = format!(
        r#"
repos_base_path: "{repos}"
supervisor:
  socket_path: "{socket}"
livekit:
  url: "ws://127.0.0.1:7880"
  api_key: "test-key"
  api_secret: "test-secret"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#,
        repos = repos_base.display(),
        socket = missing_socket.display(),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    // The config tempdir is only read by `load`; leaking it keeps the test binary simple.
    std::mem::forget(dir);
    DaemonConfig::load(&path).expect("config must parse")
}

fn a_service(config: DaemonConfig, tddy_data_dir: PathBuf) -> ConnectionServiceImpl {
    let sessions_base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));
    let eligible: Arc<dyn EligibleDaemonSource> = Arc::new(StubEligibleDaemonSource);
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: eligible,
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: alpha\n    git_url: \"\"\n    main_repo_path: {}\n",
        PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

#[tokio::test]
async fn refuses_to_start_a_session_when_the_declared_supervisor_is_unreachable() {
    // Given a host configured for a supervisor that is not running, and a tool that would leave a
    // marker behind if the daemon ever started it itself
    let os_user = current_username();
    let data_dir = tempfile::tempdir().unwrap();
    let repos_base = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let marker = tools.path().join("session-was-started-by-the-daemon");
    let tool = a_tool_that_stays_alive(tools.path(), &marker);
    let missing_socket = tools.path().join("tddy-supervisor.sock");
    register_project(&data_dir.path().join("projects"), repo.path());
    let service = a_service(
        a_supervised_config(&os_user, repos_base.path(), &missing_socket),
        data_dir.path().to_path_buf(),
    );

    // When
    let error = service
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            tool_path: tool.to_str().unwrap().to_string(),
            ..Default::default()
        }))
        .await
        .expect_err("an unreachable supervisor must fail the session start");

    // Then the outage is reported, and nothing was spawned as the daemon's own user — falling back
    // would turn an isolated session into one running as the daemon.
    assert!(
        error
            .message()
            .contains(&missing_socket.display().to_string()),
        "the error should name the unreachable supervisor socket, got: {}",
        error.message()
    );
    assert!(
        !marker.exists(),
        "no session may be spawned by the daemon itself when a supervisor is configured"
    );
}

#[tokio::test]
async fn refuses_to_clone_a_project_when_the_declared_supervisor_is_unreachable() {
    // Given a host configured for a supervisor that is not running
    let os_user = current_username();
    let data_dir = tempfile::tempdir().unwrap();
    let repos_base = tempfile::tempdir().unwrap();
    let sockets = tempfile::tempdir().unwrap();
    let missing_socket = sockets.path().join("tddy-supervisor.sock");
    let service = a_service(
        a_supervised_config(&os_user, repos_base.path(), &missing_socket),
        data_dir.path().to_path_buf(),
    );

    // When
    let error = service
        .add_project_to_host(Request::new(AddProjectToHostRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.invalid/owner/repo.git".to_string(),
            main_branch_ref: String::new(),
            daemon_instance_id: String::new(),
            user_relative_path: String::new(),
        }))
        .await
        .expect_err("an unreachable supervisor must fail the clone");

    // Then the outage is reported, and no working copy was cloned by the daemon itself
    assert!(
        error
            .message()
            .contains(&missing_socket.display().to_string()),
        "the error should name the unreachable supervisor socket, got: {}",
        error.message()
    );
    assert!(
        !repos_base.path().join("alpha").exists(),
        "no repository may be cloned by the daemon itself when a supervisor is configured"
    );
}
