//! Acceptance tests for `ListSessionWorkflowFiles` and `ReadSessionWorkflowFile` RPCs.
//!
//! These assert allowlisted listing, safe rejection of traversal basenames, and exact
//! UTF-8 reads for workflow files. Handlers are not fully implemented yet (Red).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Code;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionWorkflowFilesRequest,
    ReadSessionWorkflowFileRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn test_config() -> DaemonConfig {
    let yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    DaemonConfig::load(&path).unwrap()
}

fn test_service(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let config = test_config();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        user_resolver,
        None,
        None,
        None,
    )
}

fn write_session_yaml(session_dir: &std::path::Path, session_id: &str, pid: u32) {
    let metadata = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "proj-1".to_string(),
        created_at: "2026-03-21T10:00:00Z".to_string(),
        updated_at: "2026-03-21T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp".to_string()),
        pid: Some(pid),
        tool: Some("test-tool".to_string()),
        livekit_room: Some("test-room".to_string()),
    };
    tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
}

/// Acceptance: list returns exactly the allowlisted workflow basenames for a fixture session dir.
#[tokio::test]
async fn list_session_workflow_files_returns_allowlisted_basenames() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "fixture-workflow-files";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(&session_dir, session_id, pid);

    std::fs::write(
        session_dir.join("changeset.yaml"),
        "goal: acceptance-list\n",
    )
    .unwrap();
    std::fs::write(session_dir.join(".session.yaml"), "session: fixture\n").unwrap();
    std::fs::write(session_dir.join("PRD.md"), "# Plan\n").unwrap();
    std::fs::write(session_dir.join("TODO.md"), "- [ ] item\n").unwrap();
    std::fs::write(session_dir.join(".env"), "SECRET=must-not-appear-in-list\n").unwrap();

    let service = test_service(sessions_base);
    let request = Request::new(ListSessionWorkflowFilesRequest {
        session_token: "valid-token".to_string(),
        session_id: session_id.to_string(),
    });
    let response = service
        .list_session_workflow_files(request)
        .await
        .expect("ListSessionWorkflowFiles should succeed");
    let mut basenames: Vec<String> = response
        .into_inner()
        .files
        .into_iter()
        .map(|e| e.basename)
        .collect();
    basenames.sort();
    assert_eq!(
        basenames,
        vec![
            ".session.yaml".to_string(),
            "PRD.md".to_string(),
            "TODO.md".to_string(),
            "changeset.yaml".to_string(),
        ],
        "response must list only allowlisted workflow files (exclude secrets like .env)"
    );
}

/// Acceptance: traversal or non-allowlisted basename requests never yield a successful read outside the session root.
#[tokio::test]
async fn read_session_workflow_file_rejects_path_outside_session_dir() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "traversal-guard-session";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(&session_dir, session_id, pid);
    std::fs::write(session_dir.join("changeset.yaml"), "safe: true\n").unwrap();

    let service = test_service(sessions_base);
    for malicious in [
        "../.env",
        "..\\changeset.yaml",
        "foo/../changeset.yaml",
        "/etc/passwd",
    ] {
        let request = Request::new(ReadSessionWorkflowFileRequest {
            session_token: "valid-token".to_string(),
            session_id: session_id.to_string(),
            basename: malicious.to_string(),
        });
        let err = service
            .read_session_workflow_file(request)
            .await
            .unwrap_err();
        assert_ne!(err.code, Code::Ok);
        assert!(
            matches!(
                err.code,
                Code::InvalidArgument | Code::PermissionDenied | Code::FailedPrecondition
            ),
            "malicious basename {:?} must be rejected with a client/security error, got {:?}",
            malicious,
            err.code
        );
    }
}

/// Acceptance: reading `changeset.yaml` returns exact on-disk UTF-8 bytes (LF fixture).
#[tokio::test]
async fn read_session_workflow_file_returns_utf8_content_for_yaml() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let golden = "workflow_root: /tmp/acceptance\nnested:\n  unique_key: 42\n";
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_id = "read-yaml-session";
    let session_dir = unified_session_dir_path(&sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_yaml(&session_dir, session_id, pid);
    std::fs::write(session_dir.join("changeset.yaml"), golden).unwrap();

    let service = test_service(sessions_base);
    let request = Request::new(ReadSessionWorkflowFileRequest {
        session_token: "valid-token".to_string(),
        session_id: session_id.to_string(),
        basename: "changeset.yaml".to_string(),
    });
    let response = service
        .read_session_workflow_file(request)
        .await
        .expect("ReadSessionWorkflowFile should succeed for allowlisted yaml");
    assert_eq!(response.into_inner().content_utf8, golden);
}
