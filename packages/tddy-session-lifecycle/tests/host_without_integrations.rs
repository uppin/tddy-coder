//! A session host built with nothing wired — every extension port at its null object — still
//! serves a local session end to end (`docs/ft/daemon/session-room.md` § Configuration):
//! the daemon's wiring may leave the LiveKit plugin out, and what is left is a working daemon.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Request, RequestMetadata, RequestTransport};
use tddy_service::proto::session::{
    ConnectSessionRequest, ConnectSessionResponse, DeleteSessionRequest, ResumeSessionRequest,
    SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::stub_scripts::a_stub_agent_script;

const PROJECT_ID: &str = "host-without-integrations";

fn os_user() -> String {
    std::env::var("USER").expect("USER must be set to resolve the session's OS user")
}

/// Runs `git` in `dir` with a fixed identity, so a machine with no git config still commits.
fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .expect("git must be on PATH");
    assert!(
        output.status.success(),
        "git {args:?} failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository whose `origin` is itself, so the worktree setup's fetch needs no server.
fn a_repository_with_origin(dir: &Path) {
    git(dir, &["init", "-b", "main"]);
    git(dir, &["commit", "--allow-empty", "-m", "seed"]);
    git(dir, &["remote", "add", "origin", dir.to_str().unwrap()]);
    git(dir, &["push", "-u", "origin", "main"]);
}

/// A daemon host constructed and given nothing else: no room integration, no reach, no terminal
/// exposure, no peers.
struct AHostWithNothingWired {
    host: DaemonSessionHost,
    sessions_base: PathBuf,
    _sessions: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _stubs: tempfile::TempDir,
    _config: tempfile::TempDir,
}

fn a_host_with_nothing_wired() -> AHostWithNothingWired {
    let repo = tempfile::tempdir().unwrap();
    a_repository_with_origin(repo.path());
    let stubs = tempfile::tempdir().unwrap();
    let claude = a_stub_agent_script(stubs.path(), "stub-claude.sh")
        .echoing_argv()
        .build();

    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("daemon.yaml");
    let user = os_user();
    std::fs::write(
        &config_path,
        format!(
            "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\n\
             allowed_tools:\n  - path: /bin/true\n    label: true\n\
             claude_cli:\n  binary_path: {}\n",
            claude.display()
        ),
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("daemon.yaml must load");

    let sessions = tempfile::tempdir().unwrap();
    let projects_dir = sessions.path().join("projects");
    std::fs::create_dir_all(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT_ID}\n    name: {PROJECT_ID}\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo.path().display()
        ),
    )
    .unwrap();
    let base = sessions.path().to_path_buf();
    let host = DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(base.clone())),
        sessions.path().to_path_buf(),
        Arc::new(move |token| (token == TEST_TOKEN).then(|| user.clone())),
        None,
        PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
    );
    AHostWithNothingWired {
        host,
        sessions_base: sessions.path().to_path_buf(),
        _sessions: sessions,
        _repo: repo,
        _stubs: stubs,
        _config: config_dir,
    }
}

#[tokio::test]
async fn a_host_built_with_nothing_wired_serves_a_local_session() {
    // Given a host with every extension port at its null object
    let daemon = a_host_with_nothing_wired();

    // When a session is started on it
    let started = daemon
        .host
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "claude-cli".to_string(),
            model: "claude-opus-5".to_string(),
            ..Default::default()
        }))
        .await
        .expect("a session starts with no integration wired")
        .into_inner();
    let session_dir = unified_session_dir_path(&daemon.sessions_base, &started.session_id);

    // Then it has no room, and connecting from this host attaches without one
    assert_eq!(started.livekit_room, "");
    let connected = daemon
        .host
        .connect_session(Request::with_metadata(
            ConnectSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: started.session_id.clone(),
            },
            RequestMetadata::over(RequestTransport::InProcess),
        ))
        .await
        .expect("a local connect needs no integration")
        .into_inner();
    assert_eq!(connected, ConnectSessionResponse::default());

    // And it resumes on the same host
    let resumed = daemon
        .host
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: started.session_id.clone(),
        }))
        .await
        .expect("a session resumes with no integration wired")
        .into_inner();
    assert_eq!(
        (resumed.session_id.as_str(), resumed.livekit_room.as_str()),
        (started.session_id.as_str(), "")
    );

    // And deleting it removes it, with no room to close
    daemon
        .host
        .delete_session(Request::direct(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: started.session_id.clone(),
        }))
        .await
        .expect("a session deletes with no integration wired");
    assert!(
        !session_dir.exists(),
        "the deleted session's directory {session_dir:?} must be gone"
    );
}
