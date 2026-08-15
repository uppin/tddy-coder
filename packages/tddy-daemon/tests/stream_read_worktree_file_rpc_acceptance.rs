//! `StreamReadWorktreeFile` at the RPC — AC15-AC20 of `docs/ft/daemon/session-worktree-sync.md`.
//!
//! `stream_read_worktree_file_acceptance.rs` pins the reader: what bytes come back and which paths
//! are refused. What is pinned *here* is the half the reader cannot answer — that the handler
//! reaches it through the same `resolve_listed_worktree` gate the unary read uses, and that what it
//! puts on the wire is a sequence of frames a client can rebuild the file from.
//!
//! The frame budget is the point of the last of those. A payload above
//! `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` is chunk-framed by the transport, and one lost
//! chunk frame wedges the call with no error at all — so a handler that returned a whole file in
//! one oversized frame would round-trip perfectly in a test and fail silently in production.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{ConnectionServiceImpl, HOST_DOCUMENT_FRAME_BYTES};
use tddy_daemon::project_storage::{self, ProjectData};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_daemon::user_sessions_path::projects_path_for_user;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ReadWorktreeFileRequest,
};

/// Well above anything this suite writes, so a size refusal is always the one a test asked for.
const A_ROOMY_CAP: u64 = 64 * 1024 * 1024;

/// Small enough that an over-cap file costs a kilobyte to write rather than sixty-four megabytes.
const A_TIGHT_CAP: u64 = 1024;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A registered project, its worktree, and the service serving reads of it.
struct ServedWorktree {
    service: ConnectionServiceImpl,
    project_id: String,
    worktree_path: String,
    worktree: PathBuf,
    _data_dir: tempfile::TempDir,
    _tmp: tempfile::TempDir,
}

/// A project whose worktree this daemon serves, under a `max_attachment_bytes` of `cap`.
fn a_served_worktree_capped_at(cap: u64) -> ServedWorktree {
    let os_user = std::env::var("USER").expect("USER must be set");
    let data_dir = tempfile::tempdir().expect("data tempdir");
    let service = a_service(data_dir.path().to_path_buf(), &os_user, cap);

    let tmp = tempfile::tempdir().expect("repo tempdir");
    let repo = tmp.path().join("main");
    std::fs::create_dir_all(&repo).expect("create repo dir");
    git(&repo, &["init", "-q", "--initial-branch=main"]);
    git(&repo, &["config", "user.email", "agent@example.com"]);
    git(&repo, &["config", "user.name", "Agent"]);
    std::fs::write(repo.join("README.md"), "# served\n").expect("write README");
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let worktree = tmp.path().join("wt-feature");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            worktree.to_str().expect("worktree path is utf-8"),
            "-b",
            "feature-x",
        ],
    );

    let projects_dir = projects_path_for_user(&os_user, Some(data_dir.path())).expect("projects");
    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "stream-read-worktree-file".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: repo
                .canonicalize()
                .expect("canonical repo")
                .display()
                .to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: HashMap::new(),
        },
    )
    .expect("register project");

    let canonical = worktree.canonicalize().expect("canonical worktree");
    ServedWorktree {
        service,
        project_id,
        worktree_path: canonical.display().to_string(),
        worktree: canonical,
        _data_dir: data_dir,
        _tmp: tmp,
    }
}

fn a_service(data_dir: PathBuf, os_user: &str, max_attachment_bytes: u64) -> ConnectionServiceImpl {
    let yaml = format!(
        "users:\n  - github_user: \"testuser\"\n    os_user: \"{os_user}\"\nmax_attachment_bytes: {max_attachment_bytes}\n"
    );
    let config_dir = tempfile::tempdir().expect("config tempdir");
    let config_path = config_dir.path().join("config.yaml");
    std::fs::write(&config_path, yaml).expect("write config");
    let config = DaemonConfig::load(&config_path).expect("load config");

    let sessions_base = data_dir.clone();
    ConnectionServiceImpl::new(
        config,
        Arc::new(move |_| Some(sessions_base.clone())),
        data_dir,
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string())),
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn a_read_request(served: &ServedWorktree, rel_path: &str) -> ReadWorktreeFileRequest {
    ReadWorktreeFileRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: served.project_id.clone(),
        worktree_path: served.worktree_path.clone(),
        rel_path: rel_path.to_string(),
    }
}

/// Every frame of one successful stream, kept as frames rather than as one buffer: reassembling
/// correctly is only half the contract, and a single oversized frame reassembles perfectly.
struct StreamedFile {
    frames: Vec<Vec<u8>>,
    total_byte_sizes: Vec<u64>,
}

impl StreamedFile {
    fn bytes(&self) -> Vec<u8> {
        self.frames.concat()
    }

    fn frame_sizes(&self) -> Vec<usize> {
        self.frames.iter().map(Vec::len).collect()
    }
}

async fn a_streamed_read(served: &ServedWorktree, rel_path: &str) -> StreamedFile {
    let mut stream = served
        .service
        .stream_read_worktree_file(Request::new(a_read_request(served, rel_path)))
        .await
        .unwrap_or_else(|e| panic!("StreamReadWorktreeFile must serve {rel_path}: {e:?}"))
        .into_inner();

    let mut streamed = StreamedFile {
        frames: Vec::new(),
        total_byte_sizes: Vec::new(),
    };
    while let Some(item) = stream.next().await {
        let chunk = item.expect("no frame of a successful read carries a status");
        streamed.frames.push(chunk.data);
        streamed.total_byte_sizes.push(chunk.total_byte_size);
    }
    streamed
}

/// The status of a refused read, for a test that is about the refusal.
async fn a_refused_read(served: &ServedWorktree, rel_path: &str) -> Code {
    match served
        .service
        .stream_read_worktree_file(Request::new(a_read_request(served, rel_path)))
        .await
    {
        Err(status) => status.code(),
        Ok(_) => panic!("expected {rel_path} to be refused, got a stream"),
    }
}

/// Bytes no UTF-8 decoder accepts, in a pattern that would survive a lossy re-encoding only by
/// accident.
fn a_binary_file_of(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 256) as u8).collect()
}

// ---------------------------------------------------------------------------
// Byte fidelity on the wire — AC16
// ---------------------------------------------------------------------------

#[tokio::test]
async fn streams_a_binary_file_byte_for_byte() {
    // Given a file holding every byte value, including ones no UTF-8 decoder accepts
    let served = a_served_worktree_capped_at(A_ROOMY_CAP);
    let contents = a_binary_file_of(2048);
    std::fs::write(served.worktree.join("logo.png"), &contents).expect("write file");

    // When it is streamed
    let streamed = a_streamed_read(&served, "logo.png").await;

    // Then every byte comes back as it was written
    assert_eq!(streamed.bytes(), contents);
}

// ---------------------------------------------------------------------------
// Framing — AC18, AC19
// ---------------------------------------------------------------------------

#[tokio::test]
async fn streams_a_file_spanning_several_frames_within_the_frame_budget() {
    // Given a file two frames and a bit long
    let served = a_served_worktree_capped_at(A_ROOMY_CAP);
    let contents = a_binary_file_of(HOST_DOCUMENT_FRAME_BYTES * 2 + 1_024);
    std::fs::write(served.worktree.join("big.bin"), &contents).expect("write file");

    // When it is streamed
    let streamed = a_streamed_read(&served, "big.bin").await;

    // Then it arrives as whole frames of the budget plus the remainder, and rebuilds exactly
    assert_eq!(
        streamed.frame_sizes(),
        vec![HOST_DOCUMENT_FRAME_BYTES, HOST_DOCUMENT_FRAME_BYTES, 1_024]
    );
    assert_eq!(streamed.bytes(), contents);
}

#[tokio::test]
async fn stamps_the_files_full_size_on_every_frame() {
    // Given a file spanning more than one frame
    let served = a_served_worktree_capped_at(A_ROOMY_CAP);
    let contents = a_binary_file_of(HOST_DOCUMENT_FRAME_BYTES + 7);
    std::fs::write(served.worktree.join("big.bin"), &contents).expect("write file");

    // When it is streamed
    let streamed = a_streamed_read(&served, "big.bin").await;

    // Then a reader knows the total from the first frame, with no header frame to special-case
    let full = contents.len() as u64;
    assert_eq!(streamed.total_byte_sizes, vec![full, full]);
}

#[tokio::test]
async fn streams_an_empty_file_as_exactly_one_empty_frame() {
    // Given a zero-byte file
    let served = a_served_worktree_capped_at(A_ROOMY_CAP);
    std::fs::write(served.worktree.join("empty.txt"), b"").expect("write file");

    // When it is streamed
    let streamed = a_streamed_read(&served, "empty.txt").await;

    // Then "the file is empty" stays distinguishable from "the stream produced nothing"
    assert_eq!(streamed.frames, vec![Vec::<u8>::new()]);
    assert_eq!(streamed.total_byte_sizes, vec![0]);
}

// ---------------------------------------------------------------------------
// Refusals — AC17, AC20
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_a_file_over_the_hosts_attachment_cap_before_the_first_frame() {
    // Given a file above this host's max_attachment_bytes
    let served = a_served_worktree_capped_at(A_TIGHT_CAP);
    let contents = a_binary_file_of(A_TIGHT_CAP as usize + 1);
    std::fs::write(served.worktree.join("big.bin"), &contents).expect("write file");

    // When it is asked for
    let code = a_refused_read(&served, "big.bin").await;

    // Then the call fails outright: once frames have started a client cannot tell a truncated file
    // from a whole one, so the refusal has to come before any of them.
    assert_eq!(code, Code::InvalidArgument);
}

#[tokio::test]
async fn refuses_a_path_that_climbs_out_of_the_worktree() {
    // Given a served worktree
    let served = a_served_worktree_capped_at(A_ROOMY_CAP);

    // When a traversal is attempted
    let code = a_refused_read(&served, "../../etc/passwd").await;

    // Then the handler is behind the same gate the unary read is, rather than reading the file and
    // framing it.
    assert_eq!(code, Code::InvalidArgument);
}

#[tokio::test]
async fn refuses_an_unknown_session_token() {
    // Given a served worktree and a request carrying a token the daemon does not know
    let served = a_served_worktree_capped_at(A_ROOMY_CAP);
    std::fs::write(served.worktree.join("README.md"), b"# served\n").expect("write file");
    let request = ReadWorktreeFileRequest {
        session_token: "not-a-token".to_string(),
        ..a_read_request(&served, "README.md")
    };

    // When it is asked for
    let status = served
        .service
        .stream_read_worktree_file(Request::new(request))
        .await
        .expect_err("an unknown token must be refused");

    // Then
    assert_eq!(status.code(), Code::Unauthenticated);
}
