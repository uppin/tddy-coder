//! Acceptance tests for tddy-daemon.
//!
//! These tests define the desired behavior. Some may fail until
//! the full implementation is complete.

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::session_reader;

/// Acceptance: Daemon loads YAML config with users and allowed_tools.
#[test]
fn acceptance_config_loads_users_and_tools() {
    let yaml = r#"
listen:
  web_port: 8899
  web_host: "0.0.0.0"
users:
  - github_user: "octocat"
    os_user: "dev1"
  - github_user: "torvalds"
    os_user: "dev2"
allowed_tools:
  - path: "target/debug/tddy-coder"
    label: "tddy-coder (debug)"
  - path: "target/release/tddy-coder"
    label: "tddy-coder (release)"
"#;
    let path = std::env::temp_dir().join("tddy-daemon-acceptance-config.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config should load");
    assert_eq!(config.users.len(), 2);
    assert_eq!(config.users[0].github_user, "octocat");
    assert_eq!(config.users[0].os_user, "dev1");
    assert_eq!(config.allowed_tools.len(), 2);
    assert_eq!(config.allowed_tools[0].path, "target/debug/tddy-coder");
    assert!(
        config.spawn_mouse,
        "spawn_mouse defaults to true when omitted"
    );
}

/// Acceptance: spawn_mouse can be disabled in YAML.
#[test]
fn acceptance_config_spawn_mouse_false() {
    let yaml = r#"
users:
  - github_user: "a"
    os_user: "b"
spawn_mouse: false
"#;
    let path = std::env::temp_dir().join("tddy-daemon-acceptance-mouse.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config should load");
    assert!(!config.spawn_mouse);
}

/// Acceptance: GitHub user maps to OS user; unmapped returns None.
#[test]
fn acceptance_user_mapping_github_to_os() {
    let yaml = r#"
users:
  - github_user: "octocat"
    os_user: "dev1"
"#;
    let path = std::env::temp_dir().join("tddy-daemon-acceptance-mapping.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).unwrap();
    assert_eq!(config.os_user_for_github("octocat"), Some("dev1"));
    assert_eq!(config.os_user_for_github("unknown"), None);
}

/// Acceptance: Session reader returns sessions from directory containing .session.yaml.
#[test]
fn acceptance_session_reader_lists_sessions_from_dir() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("sessions");
    let session_dir = sessions_base.join("session-123");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = r#"
session_id: "session-123"
project_id: "proj-1"
created_at: "2026-03-19T10:00:00Z"
updated_at: "2026-03-19T10:30:00Z"
status: "active"
repo_path: "/home/dev1/projects/myapp"
pid: 12345
tool: "target/release/tddy-coder"
livekit_room: "daemon-session-123"
"#;
    std::fs::write(session_dir.join(".session.yaml"), metadata).unwrap();

    let sessions = session_reader::list_sessions_in_dir(&sessions_base).unwrap();
    assert!(
        !sessions.is_empty(),
        "expected at least one session when .session.yaml exists"
    );
    assert_eq!(sessions[0].session_id, "session-123");
    assert_eq!(sessions[0].repo_path, "/home/dev1/projects/myapp");
    assert_eq!(sessions[0].project_id, "proj-1");
}

/// When `changeset.yaml` exists, `ListSessions` / session reader still exposes **metadata** `status`
/// only — workflow state is not merged from disk; live workflow comes from `TddyRemote` stream.
#[test]
fn acceptance_session_reader_lists_metadata_status_only_even_with_changeset_on_disk() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("sessions");
    let session_dir = sessions_base.join("session-workflow");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = r#"
session_id: "session-workflow"
project_id: "proj-1"
created_at: "2026-03-19T10:00:00Z"
updated_at: "2026-03-19T10:30:00Z"
status: "active"
repo_path: "/var/tddy/Code/tddy-coder"
pid: 999999001
tool: "target/release/tddy-coder"
livekit_room: "daemon-session-workflow"
"#;
    std::fs::write(session_dir.join(".session.yaml"), metadata).unwrap();

    let mut cs = tddy_core::Changeset::default();
    cs.state.current = "GreenComplete".to_string();
    tddy_core::write_changeset(&session_dir, &cs).unwrap();

    let sessions = session_reader::list_sessions_in_dir(&sessions_base).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].status, "active",
        "connection list uses .session.yaml status; workflow lives on TddyRemote stream, not changeset.yaml"
    );
}

/// Acceptance: project_storage round-trips projects.yaml.
#[test]
fn acceptance_project_storage_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let projects_dir = temp.path().join("projects");
    let p = tddy_daemon::project_storage::ProjectData {
        project_id: "uuid-1".to_string(),
        name: "my-app".to_string(),
        git_url: "https://github.com/org/repo.git".to_string(),
        main_repo_path: "/home/u/repos/my-app".to_string(),
    };
    tddy_daemon::project_storage::add_project(&projects_dir, p.clone()).unwrap();
    let list = tddy_daemon::project_storage::read_projects(&projects_dir).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0], p);
}

/// Acceptance: Daemon config parses repos_base_path.
#[test]
fn acceptance_config_loads_repos_base_path() {
    let yaml = r#"
repos_base_path: "repos"
users:
  - github_user: "a"
    os_user: "b"
"#;
    let path = std::env::temp_dir().join("tddy-daemon-acceptance-repos.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = DaemonConfig::load(&path).expect("config should load");
    assert_eq!(config.repos_base_path.as_deref(), Some("repos"));
    assert_eq!(config.repos_base_path_or_default(), "repos");
}

/// Smoke: tddy-daemon starts and serves /api/config with daemon_mode: true.
#[test]
fn acceptance_daemon_starts_and_serves_config() {
    let tddy_daemon = std::env::var("CARGO_BIN_EXE_tddy-daemon")
        .unwrap_or_else(|_| "target/debug/tddy-daemon".to_string());
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let daemon_bin = workspace_root.join(&tddy_daemon);
    if !daemon_bin.exists() {
        eprintln!("Skipping: tddy-daemon not built. Run: cargo build -p tddy-daemon");
        return;
    }

    let web_dist = workspace_root.join("packages/tddy-web/dist");
    if !web_dist.exists() || !web_dist.join("index.html").exists() {
        eprintln!("Skipping: tddy-web dist not built. Run: bun run build in packages/tddy-web");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("daemon.yaml");
    let config = format!(
        r#"
listen:
  web_port: 0
  web_host: "127.0.0.1"
web_bundle_path: {}
"#,
        web_dist.display()
    );
    std::fs::write(&config_path, config).unwrap();

    let daemon = std::process::Command::new(&daemon_bin)
        .arg("-c")
        .arg(&config_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn tddy-daemon");

    let guard = scopeguard::guard(daemon, |mut p| {
        let _ = p.kill();
        let _ = p.wait();
    });

    // Wait for server to bind (port 0 = ephemeral; we need to find it)
    // tddy-daemon with port 0 would fail - config requires web_port. Use fixed port.
    let port = 18999u16;
    let config_path2 = temp.path().join("daemon2.yaml");
    let config2 = format!(
        r#"
listen:
  web_port: {}
  web_host: "127.0.0.1"
web_bundle_path: {}
"#,
        port,
        web_dist.display()
    );
    std::fs::write(&config_path2, config2).unwrap();

    drop(guard);
    let daemon2 = std::process::Command::new(&daemon_bin)
        .arg("-c")
        .arg(&config_path2)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn tddy-daemon");

    let _guard2 = scopeguard::guard(daemon2, |mut p| {
        let _ = p.kill();
        let _ = p.wait();
    });

    let url = format!("http://127.0.0.1:{}/api/config", port);
    for _ in 0..30 {
        if let Ok(resp) = reqwest::blocking::get(&url) {
            if resp.status().is_success() {
                let json: serde_json::Value = resp.json().unwrap();
                assert_eq!(json.get("daemon_mode"), Some(&serde_json::json!(true)));
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    panic!("tddy-daemon did not serve /api/config with daemon_mode within 6s");
}
