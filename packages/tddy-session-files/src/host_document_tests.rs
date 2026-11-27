//! Host-document gate tests over real scope roots on disk.
//
// Test-only: included via `mod` under `#[cfg(all(test, unix))]`, so the filesystem-port
// guard reads the whole file as test code and the real-disk fixtures below are the tests'
// fixtures, not production I/O.
#![cfg(test)]
use super::*;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::SessionMetadata;
use tddy_rpc::Code;

const SESSION_ID: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";
const PROJECT_ID: &str = "host-doc-proj";

fn caller() -> String {
    std::env::var("USER").expect("USER")
}

/// None of these scopes resolve against the staging base; naming a path that exists nowhere
/// keeps that explicit (and fails loudly if one of them ever starts reaching for it).
fn unused_staging_base() -> &'static Path {
    Path::new("/nonexistent-staging-base")
}

/// A data root + the owning session's directory, with `artifacts/` and `uploads/` ready.
fn a_session_with_artifacts(
    artifact: Option<(&str, &[u8])>,
    upload: Option<(&str, &str, &[u8])>,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let data = tempfile::tempdir().unwrap();
    let session_dir = unified_session_dir_path(data.path(), SESSION_ID);
    let artifacts = session_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    if let Some((name, bytes)) = artifact {
        std::fs::write(artifacts.join(name), bytes).unwrap();
    }
    if let Some((upload_id, file_name, bytes)) = upload {
        let dir = session_dir.join("uploads").join(upload_id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file_name), bytes).unwrap();
    }
    (data, session_dir)
}

fn write_session_metadata(session_dir: &std::path::Path, repo_path: Option<&str>) {
    let meta = SessionMetadata {
        owner_subject: None,
        display_name: None,
        effort: None,
        session_id: SESSION_ID.to_string(),
        project_id: PROJECT_ID.to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        status: "running".to_string(),
        repo_path: repo_path.map(str::to_string),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: None,
        model: None,
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
        cursor_chat_id: None,
        harness_resume_token: None,
        orchestration_mark: None,
        agent_conversations: Vec::new(),
    };
    tddy_core::write_session_metadata(session_dir, &meta).unwrap();
}

/// AC(hd-1) — SESSION_ARTIFACT resolves under the caller's os_user data root.
#[test]
fn read_host_document_resolves_session_artifact_scope_under_the_callers_os_user() {
    // Given — a session holding artifacts/PRD.md
    let (data, _session_dir) = a_session_with_artifacts(Some(("PRD.md", b"plan")), None);

    // When
    let doc = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionArtifact,
        SESSION_ID,
        "",
        "PRD.md",
    )
    .unwrap();

    // Then
    assert_eq!(doc.data, b"plan");
    assert_eq!(doc.byte_size, 4);
}

/// AC(hd-2) — SESSION_UPLOAD resolves with "<upload_id>/<file_name>".
#[test]
fn read_host_document_resolves_session_upload_scope_with_upload_id_slash_file_name() {
    // Given — a session holding uploads/u1/notes.txt
    let (data, _session_dir) = a_session_with_artifacts(None, Some(("u1", "notes.txt", b"notes")));

    // When
    let doc = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionUpload,
        SESSION_ID,
        "",
        "u1/notes.txt",
    )
    .unwrap();

    // Then
    assert_eq!(doc.data, b"notes");
}

/// AC(hd-3) — PROJECT_REPO resolves under the project's main_repo_path.
#[test]
fn read_host_document_resolves_project_repo_scope_under_the_projects_main_repo_path() {
    // Given — a project whose main_repo_path holds README.md
    let data = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), b"repo doc").unwrap();
    project_storage::write_projects(
        &data.path().join("projects"),
        &[project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "host-doc-proj".to_string(),
            git_url: String::new(),
            main_repo_path: repo.path().to_string_lossy().into_owned(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        }],
    )
    .unwrap();

    // When
    let doc = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::ProjectRepo,
        "",
        PROJECT_ID,
        "README.md",
    )
    .unwrap();

    // Then
    assert_eq!(doc.data, b"repo doc");
}

/// AC(hd-4) — a `relative_path` with `..` segments is refused.
#[test]
fn read_host_document_refuses_a_relative_path_with_dotdot_segments() {
    // Given — a session with an artifact
    let (data, _session_dir) = a_session_with_artifacts(Some(("PRD.md", b"x")), None);

    // When / Then
    let err = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionArtifact,
        SESSION_ID,
        "",
        "../outside.txt",
    )
    .unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

/// The resolver applies the *per-scope* half of the gate too, not only the shape rules every
/// scope shares: `SESSION_UPLOAD` addresses a file as exactly `<upload_id>/<file_name>`, so a
/// bare basename names no drop to read it out of. Asserted here, through the served read,
/// because the rule holding in [`validate_relative_path`] says nothing about this path calling
/// it with the scope in hand.
#[test]
fn read_host_document_refuses_a_session_upload_path_that_names_no_drop() {
    // Given — a session holding uploads/u1/notes.txt
    let (data, _session_dir) = a_session_with_artifacts(None, Some(("u1", "notes.txt", b"notes")));

    // When — the file is addressed without its drop
    let err = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionUpload,
        SESSION_ID,
        "",
        "notes.txt",
    )
    .unwrap_err();

    // Then
    assert_eq!(
        (err.code, err.message.as_str()),
        (
            Code::InvalidArgument,
            "session upload relative_path must be <upload_id>/<file_name>"
        )
    );
}

/// AC(hd-5) — an absolute `relative_path` is refused.
#[test]
fn read_host_document_refuses_an_absolute_relative_path() {
    // Given
    let (data, _session_dir) = a_session_with_artifacts(Some(("PRD.md", b"x")), None);

    // When / Then
    let err = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionArtifact,
        SESSION_ID,
        "",
        "/etc/passwd",
    )
    .unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

/// AC(hd-6) — a file over the cap is refused (not truncated).
#[test]
fn read_host_document_refuses_a_file_over_the_cap_without_truncating() {
    // Given — an artifact larger than the cap
    let (data, _session_dir) = a_session_with_artifacts(
        Some(("big.bin", &vec![b'x'; MAX_HOST_DOCUMENT_BYTES + 1])),
        None,
    );

    // When / Then
    let err = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionArtifact,
        SESSION_ID,
        "",
        "big.bin",
    )
    .unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}

/// AC(hd-7) — non-UTF-8 bytes survive verbatim.
#[test]
fn read_host_document_returns_bytes_verbatim_for_a_binary_file() {
    // Given — a binary (non-UTF-8) artifact
    let bin: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x01, 0x80, 0x7f, 0xc3];
    let (data, _session_dir) = a_session_with_artifacts(Some(("img.bin", &bin)), None);

    // When
    let doc = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionArtifact,
        SESSION_ID,
        "",
        "img.bin",
    )
    .unwrap();

    // Then
    assert_eq!(doc.data, bin);
    assert_eq!(doc.byte_size, bin.len() as u64);
}

/// AC(hd-8) — a SESSION_WORKTREE path not surfaced by the git listing is refused.
#[test]
fn read_host_document_refuses_a_worktree_relative_path_not_surfaced_by_the_listing() {
    // Given — a git repo with a tracked file and a gitignored file
    let repo = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .env("GIT_AUTHOR_NAME", "T")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "T")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .status()
            .expect("git");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "T"]);
    std::fs::write(repo.path().join("listed.txt"), b"ok").unwrap();
    std::fs::write(repo.path().join(".gitignore"), "secret.txt\n").unwrap();
    std::fs::write(repo.path().join("secret.txt"), b"hidden").unwrap();
    run(&["add", "listed.txt", ".gitignore"]);
    run(&["commit", "-q", "-m", "init"]);

    // ... and a session whose repo_path is that worktree
    let (data, session_dir) = a_session_with_artifacts(None, None);
    write_session_metadata(&session_dir, Some(repo.path().to_str().unwrap()));

    // When / Then — the gitignored file is not surfaced by the listing, so it is refused
    let err = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionWorktree,
        SESSION_ID,
        "",
        "secret.txt",
    )
    .unwrap_err();
    assert_eq!(err.code, Code::PermissionDenied);
}

/// Regression — a symlinked file inside the artifacts scope root that points outside the
/// root is refused. `std::fs::read` follows symlinks, so the full file path is canonicalized
/// and re-checked against the canonical scope root (a lexical check on the parent is not
/// enough — the file name itself may be a link).
#[test]
fn read_host_document_refuses_a_symlinked_file_escaping_the_scope_root() {
    // Given — a session and an outside file the symlink will target
    let (data, session_dir) = a_session_with_artifacts(None, None);
    let outside = tempfile::tempdir().unwrap();
    let outside_file = outside.path().join("secret.txt");
    std::fs::write(&outside_file, b"outside bytes").unwrap();
    let artifacts = session_dir.join("artifacts");
    std::os::unix::fs::symlink(&outside_file, artifacts.join("escape.md")).unwrap();

    // When / Then — the symlink resolves outside the artifacts root, so it is refused
    let err = read_host_document_bytes(
        &caller(),
        data.path(),
        unused_staging_base(),
        HostDocumentScope::SessionArtifact,
        SESSION_ID,
        "",
        "escape.md",
    )
    .unwrap_err();
    assert_eq!(err.code, Code::InvalidArgument);
}
