//! Acceptance: `ResumeSession` must fail with a clear, specific error when a session's
//! `.session.yaml` has no recorded `tool` path — never guess a default and attempt to spawn
//! with it.
//!
//! Two real bugs motivated this: (1) a hardcoded bare `"tddy-coder"` fallback that only resolves
//! when the binary happens to be on `PATH`, and (2) once that was replaced with a config-driven
//! default, the *daemon-spawned child itself* was found to unconditionally record its own `tool`
//! as a hardcoded literal too — so the guess was never even reached; a stale, wrong value was
//! already persisted. Both are symptoms of the same underlying issue: resuming a session should
//! never need to guess what to spawn. If the recorded value is missing, that's a broken session
//! record, and the fix belongs at whoever failed to record it — not at a silent fallback here.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ResumeSessionRequest,
};
use tddy_testing_commons::builders::a_session_metadata;
use tddy_testing_commons::fs::write_session_yaml;

const VALID_TOKEN: &str = "valid-token";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A `ConnectionServiceImpl` with LiveKit configured (so the only thing missing for resume is
/// the session's own recorded tool path — an unconfigured LiveKit section would otherwise fail
/// first with an unrelated error, masking the behavior this test is about).
fn service_with_livekit_configured(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: target/debug/tddy-coder
    label: debug
livekit:
  url: ws://127.0.0.1:1
  api_key: dummy-key
  api_secret: dummy-secret
"#,
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");

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
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

#[tokio::test]
async fn resume_session_fails_with_a_specific_error_when_no_tool_is_recorded() {
    // Given — a session whose .session.yaml predates (or otherwise lacks) a recorded tool path,
    // exactly the on-disk shape found in production after the daemon-spawned child hardcoded a
    // wrong value instead of recording the path it was actually invoked with
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = unified_session_dir_path(sessions_tmp.path(), "no-tool-recorded");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = a_session_metadata()
        .with_session_id("no-tool-recorded")
        .with_repo_path("/tmp")
        .with_livekit_room("test-room")
        .build();
    write_session_yaml(&session_dir, &metadata);

    let service = service_with_livekit_configured(sessions_tmp.path().to_path_buf());

    // When
    let result = service
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: "no-tool-recorded".to_string(),
        }))
        .await;

    // Then — a specific, immediate error naming the real problem, not a spawn attempt with a
    // guessed tool path
    let err = result.expect_err("resume must fail when no tool is recorded, not guess one");
    assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    assert!(
        err.message.contains("tool"),
        "error message should name the actual problem (missing recorded tool path), got: {}",
        err.message
    );
}
