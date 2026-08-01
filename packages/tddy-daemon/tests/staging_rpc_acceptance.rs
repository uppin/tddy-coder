//! Acceptance: start-session attachment materialization — both sources.
//!
//! PRD: `docs/ft/coder/session-attachments.md` § Start-session materialization
//! (amends `docs/ft/coder/session-attachments.md`).
//!
//! These pin the host-side contract end-to-end through `ConnectionServiceImpl`:
//! - a staged attachment referenced by `StartSession` lands under
//!   `{session_dir}/artifacts/attachments/<basename>` before the agent runs;
//! - a `StagedAttachmentRef` naming a `daemon_instance_id` this host cannot reach is a request
//!   error;
//! - duplicate `basename` values within one request are rejected before any attachment is written;
//! - a `HostDocumentRef` to a session artifact copies the bytes into attachments;
//! - a `HostDocumentRef` whose `relative_path` escapes the scope root is refused;
//! - a `HostDocumentRef` to a file over `MAX_HOST_DOCUMENT_BYTES` is refused.
//!
//! Multi-host forwarding of the staging + `ReadHostDocument` RPCs lives in
//! `tests/staging_forwarding_acceptance.rs` (needs the LiveKit testkit container).
//!
//! The materialization helper is shared across all session types, so the workspace path (real
//! `session_dir`, no agent spawn) is the cleanest end-to-end harness.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::host_documents::MAX_HOST_DOCUMENT_BYTES;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    session_attachment::Source as AttachmentSource, ConnectionService as ConnectionServiceTrait,
    HostDocumentRef, HostDocumentScope, SessionAttachment, StagedAttachmentRef,
    StartSessionRequest, UploadStagedAttachmentChunkRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "testuser-token";
const TEST_PROJECT_ID: &str = "attach-start-proj";
const STAGING_ID_A: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";
const STAGING_ID_B: &str = "bbbbbbbb-bbbb-7bbb-8bbb-bbbbbbbbbbbb";

fn write_config() -> (tempfile::TempDir, DaemonConfig) {
    let os_user = std::env::var("USER").expect("USER must be set");
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service(
    config: DaemonConfig,
    sessions_base: PathBuf,
    staging_base: PathBuf,
) -> ConnectionServiceImpl {
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
    .with_staging_base_dir(staging_base)
}

fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com");
        cmd.output().expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: attach-start-proj\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// A service + a registered workspace project backed by a real bare-origin git repo. The staging
/// base is its own `TempDir` so a run never sees a batch a previous run left in the process temp
/// dir (which is where the daemon stages by default).
fn a_workspace_service() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    ConnectionServiceImpl,
) {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    let staging_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config();
    let service = minimal_service(
        config,
        sessions_tmp.path().to_path_buf(),
        staging_tmp.path().to_path_buf(),
    );
    (repo_dir, sessions_tmp, staging_tmp, service)
}

async fn start_workspace(
    service: &ConnectionServiceImpl,
    attachments: Vec<SessionAttachment>,
) -> Result<String, (Code, String)> {
    let resp = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            attachments,
            ..Default::default()
        }))
        .await;
    match resp {
        Ok(r) => Ok(r.into_inner().session_id),
        Err(e) => Err((e.code, e.message)),
    }
}

/// Uploads one file's bytes as a single final chunk to `UploadStagedAttachmentChunk`.
async fn stage_one_file(
    service: &ConnectionServiceImpl,
    daemon_instance_id: &str,
    staging_id: &str,
    file_name: &str,
    data: &[u8],
) -> Result<(), (Code, String)> {
    let resp = service
        .upload_staged_attachment_chunk(Request::new(UploadStagedAttachmentChunkRequest {
            session_token: VALID_TOKEN.to_string(),
            daemon_instance_id: daemon_instance_id.to_string(),
            staging_id: staging_id.to_string(),
            file_name: file_name.to_string(),
            data: data.to_vec(),
            last: true,
        }))
        .await;
    match resp {
        Ok(_) => Ok(()),
        Err(e) => Err((e.code, e.message)),
    }
}

fn staged_ref(daemon_instance_id: &str, staging_id: &str, file_name: &str) -> SessionAttachment {
    SessionAttachment {
        basename: file_name.to_string(),
        source: Some(AttachmentSource::Staged(StagedAttachmentRef {
            daemon_instance_id: daemon_instance_id.to_string(),
            staging_id: staging_id.to_string(),
            file_name: file_name.to_string(),
        })),
    }
}

fn host_doc_ref(
    scope: HostDocumentScope,
    session_id: &str,
    relative_path: &str,
) -> SessionAttachment {
    SessionAttachment {
        basename: std::path::Path::new(relative_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(relative_path)
            .to_string(),
        source: Some(AttachmentSource::HostDocument(HostDocumentRef {
            daemon_instance_id: String::new(),
            scope: scope.into(),
            session_id: session_id.to_string(),
            project_id: String::new(),
            relative_path: relative_path.to_string(),
        })),
    }
}

// ---------------------------------------------------------------------------
// AC1: a staged attachment referenced by StartSession lands under artifacts/attachments/.
// ---------------------------------------------------------------------------

/// AC1 — a file staged via `UploadStagedAttachmentChunk`, then referenced by `StartSession`,
/// is present at `{session_dir}/artifacts/attachments/<basename>` once `StartSession` returns.
#[tokio::test]
async fn a_staged_attachment_referenced_by_start_session_lands_under_artifacts_attachments_before_the_agent_runs(
) {
    // Given — a workspace project and a staged file "spec.md"
    let (_repo, sessions_tmp, _staging, service) = a_workspace_service();
    stage_one_file(&service, "", STAGING_ID_A, "spec.md", b"# spec\nplan body")
        .await
        .expect("staging the attachment must succeed");

    // When — StartSession references the staged file
    let session_id = start_workspace(&service, vec![staged_ref("", STAGING_ID_A, "spec.md")])
        .await
        .expect("StartSession with a staged attachment must succeed");

    // Then — the file is materialized under the new session's artifacts/attachments/
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &session_id);
    let materialized = session_dir
        .join("artifacts")
        .join("attachments")
        .join("spec.md");
    assert!(
        materialized.exists(),
        "staged attachment must be materialized at {materialized:?}"
    );
    assert_eq!(std::fs::read(&materialized).unwrap(), b"# spec\nplan body");
}

// ---------------------------------------------------------------------------
// AC2: a StagedAttachmentRef naming a host this daemon cannot reach is a request error.
// ---------------------------------------------------------------------------

/// AC2 — a staged ref naming a host that is **not** in this daemon's eligible list cannot be
/// fetched from anywhere, so `StartSession` fails with `FAILED_PRECONDITION` from
/// `classify_peer_route`.
///
/// A ref naming a *reachable* peer is materialized cross-host (see
/// `tests/session_attach_cross_host_acceptance.rs`); this is the single-host guard behind the
/// "never a silent empty attachment" rule — an unreachable host must be an error, never an
/// attachment that quietly resolves against the local filesystem or arrives empty.
#[tokio::test]
async fn start_session_refuses_a_staged_attachment_ref_naming_an_unreachable_daemon_instance_id() {
    // Given — a local daemon with no peer wired up
    let (_repo, sessions_tmp, _staging, service) = a_workspace_service();

    // When — StartSession carries a staged ref naming a host that was never discovered
    let err = start_workspace(
        &service,
        vec![staged_ref("peer-host", STAGING_ID_A, "x.md")],
    )
    .await
    .expect_err("a staged ref naming an unreachable host must be rejected");

    // Then — FAILED_PRECONDITION, not a silent success and not a local read
    assert_eq!(err.0, Code::FailedPrecondition, "got {err:?}");

    // And — nothing was written: the refusal and the absence of an empty attachment are the same
    // guarantee, and only asserting both rules out a session that starts with a placeholder file
    assert!(
        !any_attachments_dir_has_files(&sessions_tmp.path().join("sessions")),
        "no attachment may be written when the staging host is unreachable"
    );
}

// ---------------------------------------------------------------------------
// AC3: duplicate basenames within one request are rejected before any write.
// ---------------------------------------------------------------------------

/// AC3 — two `SessionAttachment`s with the same `basename` (different staging ids) fail
/// `StartSession` with `INVALID_ARGUMENT` before any attachment is written.
#[tokio::test]
async fn start_session_rejects_duplicate_basenames_within_one_request_before_writing_any_attachment(
) {
    // Given — a workspace project and two staged files sharing a basename
    let (_repo, sessions_tmp, _staging, service) = a_workspace_service();
    stage_one_file(&service, "", STAGING_ID_A, "dup.md", b"a")
        .await
        .expect("staging first file must succeed");
    stage_one_file(&service, "", STAGING_ID_B, "dup.md", b"b")
        .await
        .expect("staging second file must succeed");

    // When — StartSession references both under the same basename
    let err = start_workspace(
        &service,
        vec![
            staged_ref("", STAGING_ID_A, "dup.md"),
            staged_ref("", STAGING_ID_B, "dup.md"),
        ],
    )
    .await
    .expect_err("duplicate basenames must be rejected");

    // Then — INVALID_ARGUMENT and no session was created
    assert_eq!(err.0, Code::InvalidArgument, "got {err:?}");
    let attachments_root = sessions_tmp.path().join("sessions");
    assert!(
        !any_attachments_dir_has_files(&attachments_root),
        "no attachment may be written when the request is rejected"
    );
}

/// True when any `artifacts/attachments/` directory under `root` holds a file.
fn any_attachments_dir_has_files(root: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for session in entries.flatten() {
        let dir = session.path().join("artifacts").join("attachments");
        if dir.is_dir() {
            if let Ok(files) = std::fs::read_dir(&dir) {
                if files.flatten().next().is_some() {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// AC4: a HostDocumentRef to a session artifact copies the bytes into attachments.
// ---------------------------------------------------------------------------

/// AC4 — a `HostDocumentRef` with `scope = SESSION_ARTIFACT` resolves an existing recipe
/// artifact on the owning session and copies its bytes into the new session's
/// `artifacts/attachments/<basename>`.
#[tokio::test]
async fn a_host_document_ref_to_a_session_artifact_copies_the_bytes_into_attachments() {
    // Given — a workspace project
    let (_repo, sessions_tmp, _staging, service) = a_workspace_service();

    // ... and a prior session A that already holds an artifacts/PRD.md planning doc
    let session_a = start_workspace(&service, vec![])
        .await
        .expect("session A must start");
    let dir_a = unified_session_dir_path(sessions_tmp.path(), &session_a);
    let artifacts_a = dir_a.join("artifacts");
    std::fs::create_dir_all(&artifacts_a).unwrap();
    std::fs::write(artifacts_a.join("PRD.md"), b"# primary doc\nplan body").unwrap();

    // When — StartSession session B references A's PRD.md as a host document
    let session_b = start_workspace(
        &service,
        vec![host_doc_ref(
            HostDocumentScope::SessionArtifact,
            &session_a,
            "PRD.md",
        )],
    )
    .await
    .expect("StartSession with a host-document attachment must succeed");

    // Then — B's attachments/PRD.md holds A's bytes (copied, not moved; A's doc remains)
    let dir_b = unified_session_dir_path(sessions_tmp.path(), &session_b);
    let copied = dir_b.join("artifacts").join("attachments").join("PRD.md");
    assert!(
        copied.exists(),
        "host document must be copied into attachments at {copied:?}"
    );
    assert_eq!(std::fs::read(&copied).unwrap(), b"# primary doc\nplan body");
    assert!(
        artifacts_a.join("PRD.md").exists(),
        "the source artifact must remain untouched"
    );
}

// ---------------------------------------------------------------------------
// AC5: a HostDocumentRef whose relative_path escapes the scope root is refused.
// ---------------------------------------------------------------------------

/// AC5 — a `HostDocumentRef` with `relative_path = "../outside"` is refused with
/// `INVALID_ARGUMENT` and writes no attachment.
#[tokio::test]
async fn a_host_document_ref_with_a_relative_path_escaping_the_scope_root_is_refused() {
    // Given — a workspace project and a prior session A with an artifact
    let (_repo, sessions_tmp, _staging, service) = a_workspace_service();
    let session_a = start_workspace(&service, vec![])
        .await
        .expect("session A must start");
    let dir_a = unified_session_dir_path(sessions_tmp.path(), &session_a);
    std::fs::create_dir_all(dir_a.join("artifacts")).unwrap();
    std::fs::write(dir_a.join("artifacts").join("PRD.md"), b"doc").unwrap();

    // When — StartSession B references a path that escapes the artifacts root
    let err = start_workspace(
        &service,
        vec![host_doc_ref(
            HostDocumentScope::SessionArtifact,
            &session_a,
            "../outside.txt",
        )],
    )
    .await
    .expect_err("an escaping relative_path must be rejected");

    // Then — INVALID_ARGUMENT and nothing materialized
    assert_eq!(err.0, Code::InvalidArgument, "got {err:?}");
    assert!(
        !any_attachments_dir_has_files(&sessions_tmp.path().join("sessions")),
        "no attachment may be written for a refused host document"
    );
}

// ---------------------------------------------------------------------------
// AC7: a HostDocumentRef to a file over the cap is refused.
// ---------------------------------------------------------------------------

/// AC7 — a `HostDocumentRef` pointing at a file larger than `MAX_HOST_DOCUMENT_BYTES` is
/// refused with `INVALID_ARGUMENT` (not truncated).
#[tokio::test]
async fn a_host_document_ref_to_a_file_over_the_cap_is_refused() {
    // Given — a prior session A with an artifact larger than the cap
    let (_repo, sessions_tmp, _staging, service) = a_workspace_service();
    let session_a = start_workspace(&service, vec![])
        .await
        .expect("session A must start");
    let dir_a = unified_session_dir_path(sessions_tmp.path(), &session_a);
    let artifacts_a = dir_a.join("artifacts");
    std::fs::create_dir_all(&artifacts_a).unwrap();
    let big = vec![b'x'; MAX_HOST_DOCUMENT_BYTES + 1];
    std::fs::write(artifacts_a.join("big.bin"), &big).unwrap();

    // When — StartSession B references the over-cap artifact
    let err = start_workspace(
        &service,
        vec![host_doc_ref(
            HostDocumentScope::SessionArtifact,
            &session_a,
            "big.bin",
        )],
    )
    .await
    .expect_err("an over-cap host document must be refused");

    // Then — INVALID_ARGUMENT and nothing materialized
    assert_eq!(err.0, Code::InvalidArgument, "got {err:?}");
    assert!(
        !any_attachments_dir_has_files(&sessions_tmp.path().join("sessions")),
        "no attachment may be written for an over-cap host document"
    );
}

// ---------------------------------------------------------------------------
// Regression: an incomplete staged upload is not materialized.
// ---------------------------------------------------------------------------

/// A `StagedAttachmentRef` whose upload is still in progress (no final chunk, so no completion
/// marker) is refused with `FAILED_PRECONDITION`; no truncated bytes reach the agent.
#[tokio::test]
async fn start_session_refuses_a_staged_attachment_whose_upload_is_not_complete() {
    // Given — a workspace project and a staged file with only a non-final chunk written
    let (_repo, sessions_tmp, _staging, service) = a_workspace_service();
    let resp = service
        .upload_staged_attachment_chunk(Request::new(UploadStagedAttachmentChunkRequest {
            session_token: VALID_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            staging_id: STAGING_ID_A.to_string(),
            file_name: "partial.md".to_string(),
            data: b"partial".to_vec(),
            last: false,
        }))
        .await;
    assert!(
        resp.is_ok(),
        "uploading a non-final chunk must succeed: {:?}",
        resp.err()
    );

    // When — StartSession references the incomplete staged file
    let err = start_workspace(&service, vec![staged_ref("", STAGING_ID_A, "partial.md")])
        .await
        .expect_err("an incomplete staged upload must be rejected");

    // Then — FAILED_PRECONDITION and nothing materialized
    assert_eq!(err.0, Code::FailedPrecondition, "got {err:?}");
    assert!(
        !any_attachments_dir_has_files(&sessions_tmp.path().join("sessions")),
        "no attachment may be written for an incomplete staged upload"
    );
}
