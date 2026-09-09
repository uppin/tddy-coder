use super::*;
use tddy_service::proto::connection::DeleteSessionRequest;

fn make_unit_config() -> crate::config::DaemonConfig {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
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
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        sessions_base.clone(),
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

/// Unit: delete_session rejects an invalid session token before touching the filesystem.
#[tokio::test]
async fn delete_session_unit_rejects_invalid_token() {
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let request = Request::new(DeleteSessionRequest {
        session_token: "bad-token".to_string(),
        session_id: "any-session".to_string(),
    });
    let result = service.delete_session(request).await;
    assert!(result.is_err(), "invalid token should return error");
    assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
}

/// Daemon-direct contract (changeset 2026-07-12-fast-session-change): the web routes
/// `DeleteSession` directly to the daemon participant (`daemon-{instanceId}`) with the
/// caller's `session_token` — the coder is not on the path, so lifecycle control still works
/// when the coder participant is stuck. A caller with a valid token passes auth and reaches
/// session processing; with no such session on disk the result is a downstream error
/// (FailedPrecondition from `delete_session_directory`), NOT `Unauthenticated`. Behaviour is
/// unchanged from today (delete was always daemon-served); this test locks the contract.
#[tokio::test]
async fn delete_session_unit_accepts_daemon_direct_caller_with_valid_token() {
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let request = Request::new(DeleteSessionRequest {
        session_token: "valid".to_string(),
        session_id: "no-such-session".to_string(),
    });
    let result = service.delete_session(request).await;
    assert!(result.is_err(), "missing session should return an error");
    let code = result.unwrap_err().code;
    assert_ne!(
        code,
        tddy_rpc::Code::Unauthenticated,
        "relay caller with a valid token must pass the auth boundary (got {code:?}); \
             the daemon must treat the coder relay's forwarded session_token identically to a \
             direct web call"
    );
}
