//! Acceptance tests for `ListSessionWorkflowFiles` and `ReadSessionWorkflowFile` RPCs.
//!
//! These assert allowlisted listing, safe rejection of traversal basenames, and exact
//! UTF-8 reads for workflow files. Handlers are not fully implemented yet (Red).

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Code;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionWorkflowFilesRequest,
    ReadSessionWorkflowFileRequest,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

/// Acceptance: list returns exactly the allowlisted workflow basenames for a fixture session dir.
#[tokio::test]
async fn list_session_workflow_files_returns_allowlisted_basenames() {
    // Given
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
    let metadata = a_session_metadata()
        .with_session_id(session_id)
        .with_pid(pid)
        .with_repo_path("/tmp")
        .with_tool("test-tool")
        .with_livekit_room("test-room")
        .build();
    write_session_yaml(&session_dir, &metadata);
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

    // When
    let response = service
        .list_session_workflow_files(Request::new(ListSessionWorkflowFilesRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("ListSessionWorkflowFiles should succeed");

    // Then
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
    // Given
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
    let metadata = a_session_metadata()
        .with_session_id(session_id)
        .with_pid(pid)
        .build();
    write_session_yaml(&session_dir, &metadata);
    std::fs::write(session_dir.join("changeset.yaml"), "safe: true\n").unwrap();
    let service = test_service(sessions_base);

    // When / Then
    for malicious in [
        "../.env",
        "..\\changeset.yaml",
        "foo/../changeset.yaml",
        "/etc/passwd",
    ] {
        let err = service
            .read_session_workflow_file(Request::new(ReadSessionWorkflowFileRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                basename: malicious.to_string(),
            }))
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
    // Given
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
    let metadata = a_session_metadata()
        .with_session_id(session_id)
        .with_pid(pid)
        .build();
    write_session_yaml(&session_dir, &metadata);
    std::fs::write(session_dir.join("changeset.yaml"), golden).unwrap();
    let service = test_service(sessions_base);

    // When
    let response = service
        .read_session_workflow_file(Request::new(ReadSessionWorkflowFileRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            basename: "changeset.yaml".to_string(),
        }))
        .await
        .expect("ReadSessionWorkflowFile should succeed for allowlisted yaml");

    // Then
    assert_eq!(response.into_inner().content_utf8, golden);
}
