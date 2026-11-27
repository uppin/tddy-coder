// Test-only: included via `mod` under `#[cfg(test)]`, so the filesystem-port guard reads
// the whole file as test code and the real-disk fixtures below are the tests' fixtures, not
// production I/O.
#![cfg(test)]

use super::*;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};

fn make_unit_config() -> crate::config::DaemonConfig {
    let yaml = "";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::fs::write(
        std::path::Path::new(&(path))
            .parent()
            .expect("a config directory")
            .join("users.yaml"),
        "- github_user: \"u\"\n  os_user: \"u\"\n",
    )
    .unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_service(sessions_base: std::path::PathBuf) -> DaemonSessionHost {
    let config = make_unit_config();
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == "valid" {
            Some("u".to_string())
        } else {
            None
        }
    });
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        sessions_base.clone(),
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        crate::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        crate::test_util::builtin_coding_agents(),
    )
}

fn write_unit_session(session_dir: &std::path::Path, pid: u32) {
    let session_id = session_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let metadata = SessionMetadata {
        owner_subject: None,
        display_name: None,
        effort: None,
        session_id,
        project_id: "proj-unit".to_string(),
        created_at: "2026-03-21T00:00:00Z".to_string(),
        updated_at: "2026-03-21T00:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp".to_string()),
        pid: Some(pid),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: None,
        model: None,
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
        target: None,
        managed_codebase: false,
        session_role: None,
        tmux_session: None,
        orchestration_mark: None,
        agent_conversations: Vec::new(),
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

/// Unit: signal_session rejects an invalid (empty) session token.
#[tokio::test]
async fn signal_session_unit_rejects_invalid_token() {
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let request = Request::direct(SignalSessionRequest {
        session_token: "bad-token".to_string(),
        session_id: "any".to_string(),
        signal: Signal::Sigint as i32,
        control_token: String::new(),
    });
    let result = service.signal_session(request).await;
    assert!(result.is_err(), "invalid token should return error");
    assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
}

/// Unit: signal_session returns not-found for a session that has no yaml file.
#[tokio::test]
async fn signal_session_unit_returns_error_for_missing_session() {
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let request = Request::direct(SignalSessionRequest {
        session_token: "valid".to_string(),
        session_id: "no-such-session".to_string(),
        signal: Signal::Sigterm as i32,
        control_token: String::new(),
    });
    let result = service.signal_session(request).await;
    assert!(result.is_err(), "missing session should return error");
    assert_eq!(result.unwrap_err().code, tddy_rpc::Code::NotFound);
}

impl DaemonSessionHost {
    /// Hold a long-running main agent for `session_id`; its pid.
    async fn holds_a_main_agent_for(&self, session_id: &str) -> u32 {
        self.cli_manager
            .start_with_options(
                session_id,
                std::env::temp_dir(),
                "test-model",
                vec!["/bin/sleep".to_string(), "600".to_string()],
                "claude-cli",
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
            )
            .await
            .expect("a main agent terminal starts")
            .pid
    }
}

/// Unit: signal_session with SIGKILL sends correct signal to a live process.
#[tokio::test]
async fn signal_session_unit_sigkill_reaches_live_process() {
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "sigkill-session");
    std::fs::create_dir_all(&session_dir).unwrap();
    write_unit_session(&session_dir, 0);

    let service = make_unit_service(sessions_base);
    let main = service.holds_a_main_agent_for("sigkill-session").await;
    let request = Request::direct(SignalSessionRequest {
        session_token: "valid".to_string(),
        session_id: "sigkill-session".to_string(),
        signal: Signal::Sigkill as i32,
        control_token: String::new(),
    });
    let response = service.signal_session(request).await.unwrap();
    assert!(response.into_inner().ok);

    // the held main process was killed: the daemon stops serving it
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while service.cli_manager.get("sigkill-session").await.is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the killed main (pid {main}) is still served"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
