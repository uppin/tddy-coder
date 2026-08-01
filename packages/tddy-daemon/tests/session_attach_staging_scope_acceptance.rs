//! Acceptance: the restart-cleared staging root, the `STAGED_ATTACHMENT` document scope, and the
//! streaming twins of `ReadHostDocument` / `StartSession`.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md`
//! Changeset: `docs/dev/1-WIP/2026-08-01-session-attach-ui.md`
//!
//! These pin the single-host half of the contract:
//! - staged files live under a restart-cleared temp root, not `{tddy_data_dir}/staging/`;
//! - a staged file is readable through the `STAGED_ATTACHMENT` scope by `<staging_id>/<file_name>`;
//! - a staged file whose upload never completed is refused, and nothing is written;
//! - a `relative_path` that is not exactly two segments is refused;
//! - `StreamReadHostDocument` carries a document past the unary cap whole, and refuses one past the
//!   host's configured cap;
//! - `StreamStartSession` reports progress per attachment and then exactly one terminal result;
//! - a materialization failure terminates the stream and leaves no partial attachments.
//!
//! The cross-host half (peer forwarding) lives in `tests/session_attach_cross_host_acceptance.rs`.
//!
//! The materialization helper is shared across all session types, so the workspace path (real
//! `session_dir`, no agent spawn) is the cleanest end-to-end harness — the same choice
//! `tests/staging_rpc_acceptance.rs` makes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::connection_service::HOST_DOCUMENT_FRAME_BYTES;
use tddy_daemon::host_documents::MAX_HOST_DOCUMENT_BYTES;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::connection::{
    session_attachment::Source as AttachmentSource, start_session_event::Event as StartEvent,
    ConnectionService as ConnectionServiceTrait, HostDocumentChunk, HostDocumentScope,
    ReadHostDocumentRequest, SessionAttachment, StagedAttachmentRef, StartSessionEvent,
    StartSessionRequest, UploadStagedAttachmentChunkRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "testuser-token";
const TEST_PROJECT_ID: &str = "attach-scope-proj";
const STAGING_ID_A: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";
const STAGING_ID_B: &str = "bbbbbbbb-bbbb-7bbb-8bbb-bbbbbbbbbbbb";

/// Generous cap for the tests that are not about the cap itself — comfortably above the 5 MiB
/// document used to prove the streaming read clears the 4 MiB unary ceiling.
const GENEROUS_CAP_BYTES: u64 = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn write_config(max_attachment_bytes: u64) -> (tempfile::TempDir, DaemonConfig) {
    let os_user = std::env::var("USER").expect("USER must be set");
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
max_attachment_bytes: {max_attachment_bytes}
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

fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: attach-scope-proj\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// A service whose staging base is an explicit temp root, so a test can assert *where* staged bytes
/// land. Owns every `TempDir` the service depends on for the test's lifetime.
struct Fixture {
    service: ConnectionServiceImpl,
    /// The staging base the service was told to use (stands in for `std::env::temp_dir()`).
    staging_base: PathBuf,
    /// `tddy_data_dir` — staged files must **not** appear under here.
    data_dir: PathBuf,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
    _staging: tempfile::TempDir,
    _config: tempfile::TempDir,
}

fn a_workspace_service_with_cap(max_attachment_bytes: u64) -> Fixture {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let staging_tmp = tempfile::tempdir().unwrap();
    let (config_dir, config) = write_config(max_attachment_bytes);

    let sessions_base = sessions_tmp.path().to_path_buf();
    let resolver_base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(resolver_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));

    let service = ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        sessions_base.clone(),
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
    .with_staging_base_dir(staging_tmp.path().to_path_buf());

    Fixture {
        service,
        staging_base: staging_tmp.path().to_path_buf(),
        data_dir: sessions_base,
        _repo: repo_dir,
        _sessions: sessions_tmp,
        _staging: staging_tmp,
        _config: config_dir,
    }
}

fn a_workspace_service() -> Fixture {
    a_workspace_service_with_cap(GENEROUS_CAP_BYTES)
}

/// Uploads `data` as one chunk. `last` controls whether the batch is marked complete — an
/// unfinished upload is exactly what the completeness gate must refuse.
async fn stage_chunk(
    service: &ConnectionServiceImpl,
    staging_id: &str,
    file_name: &str,
    data: &[u8],
    last: bool,
) -> Result<(), Status> {
    service
        .upload_staged_attachment_chunk(Request::new(UploadStagedAttachmentChunkRequest {
            session_token: VALID_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            staging_id: staging_id.to_string(),
            file_name: file_name.to_string(),
            data: data.to_vec(),
            last,
        }))
        .await
        .map(|_| ())
}

async fn stage_complete_file(
    service: &ConnectionServiceImpl,
    staging_id: &str,
    file_name: &str,
    data: &[u8],
) {
    stage_chunk(service, staging_id, file_name, data, true)
        .await
        .expect("staging the attachment must succeed");
}

fn staged_ref(staging_id: &str, file_name: &str, basename: &str) -> SessionAttachment {
    SessionAttachment {
        basename: basename.to_string(),
        source: Some(AttachmentSource::Staged(StagedAttachmentRef {
            daemon_instance_id: String::new(),
            staging_id: staging_id.to_string(),
            file_name: file_name.to_string(),
        })),
    }
}

fn staged_document_request(relative_path: &str) -> ReadHostDocumentRequest {
    ReadHostDocumentRequest {
        session_token: VALID_TOKEN.to_string(),
        daemon_instance_id: String::new(),
        scope: HostDocumentScope::StagedAttachment.into(),
        session_id: String::new(),
        project_id: String::new(),
        relative_path: relative_path.to_string(),
    }
}

fn a_start_session_request(attachments: Vec<SessionAttachment>) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        session_type: "workspace".to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        attachments,
        ..Default::default()
    }
}

/// Await the next stream item with a bounded wait so a stream that never produces fails loudly
/// instead of hanging the suite.
async fn next_item<T>(
    stream: &mut (impl Stream<Item = Result<T, Status>> + Unpin),
) -> Option<Result<T, Status>> {
    tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .expect("no stream item arrived within the timeout")
}

/// Drains a `HostDocumentChunk` stream into the concatenated bytes plus every frame's
/// `total_byte_size`, so a test can assert the size is stamped on **every** frame.
async fn drain_document(
    stream: &mut (impl Stream<Item = Result<HostDocumentChunk, Status>> + Unpin),
) -> (Vec<u8>, Vec<u64>) {
    let mut bytes = Vec::new();
    let mut sizes = Vec::new();
    while let Some(item) = next_item(stream).await {
        let chunk = item.expect("host-document stream yielded an error");
        bytes.extend_from_slice(&chunk.data);
        sizes.push(chunk.total_byte_size);
    }
    (bytes, sizes)
}

/// Every attachment basename that exists under the session's `artifacts/attachments/`, sorted.
fn materialized_attachments(sessions_base: &Path, session_id: &str) -> Vec<String> {
    let dir = unified_session_dir_path(sessions_base, session_id)
        .join("artifacts")
        .join("attachments");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// True when any `artifacts/attachments/` directory under the sessions root holds a file.
fn any_attachment_written(sessions_base: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(sessions_base.join("sessions")) else {
        return false;
    };
    entries.flatten().any(|session| {
        std::fs::read_dir(session.path().join("artifacts").join("attachments"))
            .map(|mut files| files.any(|f| f.is_ok()))
            .unwrap_or(false)
    })
}

// ---------------------------------------------------------------------------
// The staging root is restart-cleared, and is not the data dir
// ---------------------------------------------------------------------------

/// An uploaded staged file lands under the configured staging base — the temp root that a host
/// restart clears — and leaves nothing under `{tddy_data_dir}/staging/`, which is where it used to
/// go and where it would survive a restart forever.
#[tokio::test]
async fn staged_files_land_under_the_temp_staging_root_and_never_under_the_data_dir() {
    // Given — a service whose staging base is a temp root distinct from its data dir
    let fixture = a_workspace_service();
    let os_user = std::env::var("USER").unwrap();

    // When — a file is staged
    stage_complete_file(&fixture.service, STAGING_ID_A, "spec.md", b"# spec").await;

    // Then — it is under the staging base, byte-for-byte
    let staged =
        tddy_daemon::session_attachment_staging::staging_root_for(&os_user, &fixture.staging_base)
            .join(STAGING_ID_A)
            .join("spec.md");
    assert!(staged.exists(), "staged file must be at {staged:?}");
    assert_eq!(std::fs::read(&staged).unwrap(), b"# spec");

    // And — nothing was written under the data dir's old staging location
    let legacy_root = fixture.data_dir.join("staging");
    assert!(
        !legacy_root.exists(),
        "no staged bytes may be written under {legacy_root:?}"
    );
}

/// The default staging base is a fixed, greppable root under the directory the host clears, so an
/// operator who configures nothing still gets a root that a restart empties.
///
/// The complementary property — that staged bytes never land under `{tddy_data_dir}/staging/`, where
/// they used to survive every restart — is asserted against a real configured data dir by
/// `staged_files_land_under_the_temp_staging_root_and_never_under_the_data_dir` above; there is no
/// data dir to compare against here, because this default is derived without one.
#[test]
fn the_default_staging_base_is_a_named_root_under_the_directory_the_host_clears() {
    // Given / When
    let base = tddy_daemon::session_attachment_staging::default_staging_base_dir();

    // Then — a fixed segment, which the docs, the fixtures and the operator all name it by
    assert_eq!(
        base.file_name().and_then(|s| s.to_str()),
        Some("tddy-staging"),
        "the staging root must be named, not an anonymous temp directory: {base:?}"
    );

    // And — inside the directory the OS designates as temporary, which is what clears it
    let temp_dir = std::env::temp_dir();
    assert!(
        base.starts_with(&temp_dir),
        "the staging root must sit under {temp_dir:?}, was {base:?}"
    );
}

// ---------------------------------------------------------------------------
// The STAGED_ATTACHMENT scope
// ---------------------------------------------------------------------------

/// A completed staged file is readable through the new scope, addressed the same two-segment way
/// `SESSION_UPLOAD` is addressed. This is what lets a session host on another machine fetch bytes
/// the browser uploaded to the daemon it happened to be connected to.
#[tokio::test]
async fn a_completed_staged_file_is_readable_through_the_staged_attachment_scope() {
    // Given — a completed staged upload
    let fixture = a_workspace_service();
    stage_complete_file(&fixture.service, STAGING_ID_A, "notes.md", b"staged body").await;

    // When — it is read through the STAGED_ATTACHMENT scope by "<staging_id>/<file_name>"
    let response = fixture
        .service
        .read_host_document(Request::new(staged_document_request(&format!(
            "{STAGING_ID_A}/notes.md"
        ))))
        .await
        .expect("reading a completed staged file must succeed")
        .into_inner();

    // Then — the exact bytes come back, with the size reported
    assert_eq!(response.data, b"staged body");
    assert_eq!(response.byte_size, b"staged body".len() as u64);
}

/// A staged file whose final chunk never arrived has no completeness marker. Reading it must be
/// refused rather than returning the partial bytes — a cross-host fetch that truncated silently
/// would hand the agent a half-written attachment it cannot tell from a whole one.
#[tokio::test]
async fn the_staged_attachment_scope_refuses_a_file_whose_upload_never_completed() {
    // Given — a staged upload that was never finalized (`last: false`)
    let fixture = a_workspace_service();
    stage_chunk(
        &fixture.service,
        STAGING_ID_A,
        "partial.md",
        b"first half",
        false,
    )
    .await
    .expect("the first chunk must be accepted");

    // When — the incomplete file is read through the scope
    let err = fixture
        .service
        .read_host_document(Request::new(staged_document_request(&format!(
            "{STAGING_ID_A}/partial.md"
        ))))
        .await
        .expect_err("an incomplete staged upload must be refused");

    // Then — FAILED_PRECONDITION, naming the reason, and no bytes were handed back
    assert_eq!(err.code, Code::FailedPrecondition, "got {err:?}");
    assert!(
        err.message.contains("not complete"),
        "message must name the incomplete upload, was {:?}",
        err.message
    );
}

/// The scope's `relative_path` is exactly `<staging_id>/<file_name>`. A bare basename names no
/// batch, so it is a request error rather than a read of some default batch.
#[tokio::test]
async fn the_staged_attachment_scope_refuses_a_relative_path_that_is_not_two_segments() {
    // Given — a completed staged file that a correct path would reach
    let fixture = a_workspace_service();
    stage_complete_file(&fixture.service, STAGING_ID_A, "notes.md", b"body").await;

    // When — it is addressed with a single-segment path
    let err = fixture
        .service
        .read_host_document(Request::new(staged_document_request("notes.md")))
        .await
        .expect_err("a one-segment relative_path must be refused");

    // Then — INVALID_ARGUMENT naming the required shape
    assert_eq!(err.code, Code::InvalidArgument, "got {err:?}");
    assert!(
        err.message.contains("<staging_id>/<file_name>"),
        "message must name the required shape, was {:?}",
        err.message
    );
}

/// A path that climbs out of the batch directory is refused, so a staged upload cannot be used as a
/// foothold to read arbitrary files under the staging root.
#[tokio::test]
async fn the_staged_attachment_scope_refuses_a_relative_path_that_escapes_its_batch() {
    // Given — a service with one staged batch
    let fixture = a_workspace_service();
    stage_complete_file(&fixture.service, STAGING_ID_A, "notes.md", b"body").await;

    // When — a path traverses out of the batch
    let err = fixture
        .service
        .read_host_document(Request::new(staged_document_request(&format!(
            "{STAGING_ID_A}/../{STAGING_ID_A}/notes.md"
        ))))
        .await
        .expect_err("a traversing relative_path must be refused");

    // Then — INVALID_ARGUMENT
    assert_eq!(err.code, Code::InvalidArgument, "got {err:?}");
}

// ---------------------------------------------------------------------------
// StreamReadHostDocument
// ---------------------------------------------------------------------------

/// The unary read refuses anything over `MAX_HOST_DOCUMENT_BYTES`. The streaming twin exists so a
/// large attachment is still reachable — it must deliver the document **whole**, and stamp the
/// total size on every frame so a consumer needs no preamble to size a progress bar.
#[tokio::test]
async fn stream_read_host_document_delivers_a_document_larger_than_the_unary_cap_whole() {
    // Given — a staged document one MiB past the unary ceiling
    let fixture = a_workspace_service();
    let size = MAX_HOST_DOCUMENT_BYTES + 1024 * 1024;
    let document: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    stage_complete_file(&fixture.service, STAGING_ID_A, "big.bin", &document).await;

    // When — it is read over the streaming RPC
    let mut stream = fixture
        .service
        .stream_read_host_document(Request::new(staged_document_request(&format!(
            "{STAGING_ID_A}/big.bin"
        ))))
        .await
        .expect("StreamReadHostDocument must accept an over-cap document")
        .into_inner();
    let (bytes, sizes) = drain_document(&mut stream).await;

    // Then — every byte arrives, in order
    assert_eq!(bytes.len(), document.len(), "document was truncated");
    assert_eq!(bytes, document, "document bytes differ from the source");

    // And — the document arrived as the frames its size implies, each stamped with the total. The
    // count is derived from the document and the frame size, never from what the stream produced:
    // `vec![_; sizes.len()]` would hold for one frame just as well as for all of them.
    let expected_frames = size.div_ceil(HOST_DOCUMENT_FRAME_BYTES);
    assert_eq!(
        sizes,
        vec![size as u64; expected_frames],
        "every frame must carry the total size, and there must be one per {HOST_DOCUMENT_FRAME_BYTES}-byte slice"
    );
}

/// The host's configured cap bounds the streaming read. Over it, the document is refused up front
/// rather than streamed and truncated — the UI shows this limit and refuses at pick time, so a
/// request that gets here at all is a client that ignored it.
#[tokio::test]
async fn stream_read_host_document_refuses_a_document_over_the_hosts_configured_cap() {
    // Given — a host configured with a 1 MiB cap and a 2 MiB staged document
    let fixture = a_workspace_service_with_cap(1024 * 1024);
    let document = vec![7u8; 2 * 1024 * 1024];
    stage_complete_file(&fixture.service, STAGING_ID_A, "over.bin", &document).await;

    // When — it is read over the streaming RPC
    let err = fixture
        .service
        .stream_read_host_document(Request::new(staged_document_request(&format!(
            "{STAGING_ID_A}/over.bin"
        ))))
        .await
        .expect_err("a document over the configured cap must be refused");

    // Then — INVALID_ARGUMENT naming the limit, before any byte is streamed
    assert_eq!(err.code, Code::InvalidArgument, "got {err:?}");
    assert!(
        err.message.contains("1048576"),
        "message must name the configured limit, was {:?}",
        err.message
    );
}

// ---------------------------------------------------------------------------
// StreamStartSession
// ---------------------------------------------------------------------------

/// The streaming twin reports what the host is doing while it materializes attachments, and ends
/// with exactly one result carrying the session id. Progress before result, one result, and it is
/// last — a consumer that renders rows depends on all three.
#[tokio::test]
async fn stream_start_session_reports_progress_for_each_attachment_then_one_terminal_result() {
    // Given — two completed staged files
    let fixture = a_workspace_service();
    stage_complete_file(&fixture.service, STAGING_ID_A, "spec.md", b"# spec").await;
    stage_complete_file(&fixture.service, STAGING_ID_B, "log.txt", b"log body").await;

    // When — the session is started over the streaming RPC
    let mut stream = fixture
        .service
        .stream_start_session(Request::new(a_start_session_request(vec![
            staged_ref(STAGING_ID_A, "spec.md", "spec.md"),
            staged_ref(STAGING_ID_B, "log.txt", "log.txt"),
        ])))
        .await
        .expect("StreamStartSession must accept the request")
        .into_inner();

    let mut progressed: Vec<String> = Vec::new();
    let mut results: Vec<String> = Vec::new();
    let mut events_after_result = 0;
    while let Some(item) = next_item(&mut stream).await {
        let event: StartSessionEvent = item.expect("StreamStartSession yielded an error");
        if !results.is_empty() {
            events_after_result += 1;
        }
        match event.event.expect("every event must carry a variant") {
            StartEvent::AttachmentProgress(progress) => {
                assert_eq!(
                    progress.attachment_count, 2,
                    "every progress event reports the request's attachment count"
                );
                progressed.push(progress.basename);
            }
            StartEvent::Result(result) => results.push(result.session_id),
        }
    }

    // Then — both attachments were reported on
    progressed.sort();
    progressed.dedup();
    assert_eq!(
        progressed,
        vec!["log.txt".to_string(), "spec.md".to_string()]
    );

    // And — exactly one result, and it was the last event
    assert_eq!(results.len(), 1, "expected exactly one terminal result");
    assert_eq!(events_after_result, 0, "the result must be the last event");

    // And — both attachments are on disk under the new session
    assert_eq!(
        materialized_attachments(&fixture.data_dir, &results[0]),
        vec!["log.txt".to_string(), "spec.md".to_string()]
    );
}

/// A failure part-way through materialization ends the stream as an error and rolls back what it
/// already wrote. A half-attached session is worse than none: the agent would read a directory that
/// silently lacks the document the prompt refers to.
#[tokio::test]
async fn a_materialization_failure_terminates_the_stream_and_leaves_no_partial_attachments() {
    // Given — one good staged file, and a second ref pointing at a file that was never staged
    let fixture = a_workspace_service();
    stage_complete_file(&fixture.service, STAGING_ID_A, "good.md", b"present").await;

    // When — the session is started over the streaming RPC
    let mut stream = fixture
        .service
        .stream_start_session(Request::new(a_start_session_request(vec![
            staged_ref(STAGING_ID_A, "good.md", "good.md"),
            staged_ref(STAGING_ID_B, "missing.md", "missing.md"),
        ])))
        .await
        .expect("StreamStartSession must accept a request it later fails")
        .into_inner();

    let mut error: Option<Status> = None;
    let mut results = 0;
    while let Some(item) = next_item(&mut stream).await {
        match item {
            Ok(event) => {
                if matches!(event.event, Some(StartEvent::Result(_))) {
                    results += 1;
                }
            }
            Err(status) => {
                error = Some(status);
                break;
            }
        }
    }

    // Then — the stream ended with the error naming the missing staged file, never with a result. The
    // code alone would also hold for a duplicate basename or an unset source, neither of which is
    // what this test arranges.
    let status = error.expect("the stream must terminate with an error");
    assert_eq!(status.code, Code::InvalidArgument, "got {status:?}");
    assert_eq!(status.message, "staged attachment file not found");
    assert_eq!(results, 0, "a failed start must not emit a result event");

    // And — the attachment written before the failure was rolled back
    assert!(
        !any_attachment_written(&fixture.data_dir),
        "a failed materialization must leave no attachment behind"
    );
}

/// An unauthenticated caller is refused before the handler decides where the session runs. Deciding
/// first means an anonymous request can drive an outbound forward and hold a pending-call slot on
/// two hosts for the forward's whole deadline.
///
/// Naming a daemon this host cannot route to is what makes the order observable: classifying the
/// route first answers `FAILED_PRECONDITION` for the unknown host and never looks at the token.
#[tokio::test]
async fn stream_start_session_refuses_an_invalid_token_before_it_classifies_the_route() {
    // Given — a service, and a request with a bad token naming a host that was never discovered
    let fixture = a_workspace_service();
    let request = StartSessionRequest {
        session_token: "not-a-session-token".to_string(),
        daemon_instance_id: "some-other-host".to_string(),
        ..a_start_session_request(vec![])
    };

    // When — the session is started over the streaming RPC
    let err = fixture
        .service
        .stream_start_session(Request::new(request))
        .await
        .expect_err("an invalid token must be refused at call time, not mid-stream");

    // Then — UNAUTHENTICATED, at call time
    assert_eq!(err.code, Code::Unauthenticated, "got {err:?}");
    assert_eq!(err.message, "invalid or expired session");
}
