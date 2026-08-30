//! Acceptance tests: which allow-list the context RPCs actually serve, and the batched read.
//!
//! The three context RPCs carry an `agent` field, and it is **advisory**. Authorization on this
//! path is per OS user rather than per session
//! (`ConnectionServiceImpl::authorize_exec_tool_caller`), so a caller holding a valid token for one
//! of its sessions can name any other session of the same user. If the field also chose the table
//! row, that caller could hand itself `.claude/**`, `.cursor/**` and `.mcp.json` out of a checkout
//! it was never granted them on — the files that routinely carry API tokens in MCP `env` blocks,
//! and exactly the gitignored ones the git-listing gate this reader replaces used to refuse. The
//! enforced bound would be the union of every table row instead of the session's own.
//!
//! So the serving daemon derives the row from the session it has already looked up. These tests
//! drive the real handlers, because that derivation is a property of the handler rather than of the
//! reader beneath it.
//!
//! The batched read is exercised here for the same reason: its framing contract — which file each
//! frame belongs to, and when a file has ended — only exists on the wire.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ContextManifestRequest,
    ReadContextFileBatchRequest, ReadContextFileRequest, SplitAgentPlacement, StartSessionRequest,
};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a40";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn run_git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

fn a_git_repo_with_origin() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let path = repo.path();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@t.com"]);
    run_git(path, &["config", "user.name", "Test"]);
    run_git(path, &["commit", "-q", "--allow-empty", "-m", "init"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn register_project(sessions_base: &Path, repo_path: &Path) {
    tddy_daemon::project_storage::write_projects(
        &sessions_base.join("projects"),
        &[tddy_daemon::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "context-rpc-scope".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

/// A session on a real checkout, plus the service that serves it.
struct ASession {
    service: tddy_daemon::connection_service::ConnectionServiceImpl,
    session_id: String,
    session_dir: PathBuf,
    worktree: PathBuf,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

/// A `workspace` session — the type a split placement's codebase half is persisted as, and the only
/// type that starts without an agent process to spawn.
async fn a_session() -> ASession {
    a_session_paired_with(None).await
}

/// The same session, recorded as holding the worktree a split session's agent works in — the
/// back-pointer a split start writes with the session on the codebase host.
async fn a_session_holding_a_split_agents_worktree() -> ASession {
    a_session_paired_with(Some(SplitAgentPlacement {
        session_id: "019d105b-ac0f-78d3-9a89-409731145a41".to_string(),
        agent_daemon_instance_id: "agent-host".to_string(),
    }))
    .await
}

async fn a_session_paired_with(split_agent: Option<SplitAgentPlacement>) -> ASession {
    let repo = a_git_repo_with_origin();
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    register_project(sessions.path(), repo.path());
    let service = test_service(sessions.path().to_path_buf());

    let started = service
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "workspace".to_string(),
            split_agent,
            ..Default::default()
        }))
        .await
        .expect("workspace session must start")
        .into_inner();

    let session_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        sessions.path(),
        &started.session_id,
    );
    let worktree = PathBuf::from(
        tddy_core::read_session_metadata(&session_dir)
            .expect("session metadata")
            .repo_path
            .expect("workspace worktree"),
    );

    ASession {
        service,
        session_id: started.session_id,
        session_dir,
        worktree,
        _repo: repo,
        _sessions: sessions,
    }
}

impl ASession {
    /// Rewrites what the daemon persisted about this session's type. A `cursor-cli` session cannot
    /// be *started* in a test without a Cursor process, and the type is exactly the persisted fact
    /// the handler is supposed to read, so it is set the way the daemon sets it: in `.session.yaml`.
    fn persisted_as(self, session_type: &str) -> Self {
        let mut meta =
            tddy_core::read_session_metadata(&self.session_dir).expect("session metadata");
        meta.session_type = Some(session_type.to_string());
        tddy_core::write_session_metadata(&self.session_dir, &meta).expect("rewrite metadata");
        self
    }

    fn with_file(self, rel_path: &str, contents: &[u8]) -> Self {
        let path = self.worktree.join(rel_path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        self
    }

    /// Every path `StreamContextManifest` advertises for this session, asked as `agent`.
    async fn manifest_paths_asked_as(&self, agent: &str) -> Vec<String> {
        let mut stream = self
            .service
            .stream_context_manifest(Request::new(ContextManifestRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent: agent.to_string(),
            }))
            .await
            .expect("the manifest must be served")
            .into_inner();

        let mut paths = Vec::new();
        while let Some(entry) = stream.next().await {
            paths.push(entry.expect("manifest entry").rel_path);
        }
        paths
    }

    /// One file over `StreamReadContextFile`, asked as `agent`.
    async fn read_asked_as(
        &self,
        agent: &str,
        rel_path: &str,
    ) -> Result<Vec<u8>, tddy_rpc::Status> {
        let mut stream = self
            .service
            .stream_read_context_file(Request::new(ReadContextFileRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent: agent.to_string(),
                rel_path: rel_path.to_string(),
            }))
            .await?
            .into_inner();

        let mut bytes = Vec::new();
        while let Some(frame) = stream.next().await {
            bytes.extend_from_slice(&frame?.data);
        }
        Ok(bytes)
    }

    /// A whole batch over `StreamReadContextFileBatch`, reassembled the way the setup sync does:
    /// keyed by the path each frame names, and only counted complete once a frame says so.
    async fn read_batch(
        &self,
        rel_paths: &[&str],
    ) -> Result<BTreeMap<String, Vec<u8>>, tddy_rpc::Status> {
        let mut stream = self
            .service
            .stream_read_context_file_batch(Request::new(ReadContextFileBatchRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent: "claude".to_string(),
                rel_paths: rel_paths.iter().map(|p| (*p).to_string()).collect(),
            }))
            .await?
            .into_inner();

        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut finished: Vec<String> = Vec::new();
        while let Some(frame) = stream.next().await {
            let frame = frame?;
            files
                .entry(frame.rel_path.clone())
                .or_default()
                .extend_from_slice(&frame.data);
            if frame.end_of_file {
                finished.push(frame.rel_path);
            }
        }
        assert_eq!(
            finished.len(),
            files.len(),
            "every file in a batch must be declared finished exactly once"
        );
        Ok(files)
    }
}

// ---------------------------------------------------------------------------
// The row served is the session's, not the request's
// ---------------------------------------------------------------------------

/// A request naming a different agent than the session's own type is served the **session's** row.
/// Here the session is a Cursor one and the request says `codex`: `.cursor/rules.md` (Cursor's row)
/// comes back and `.codex/config.toml` (the requested row) does not, which is the derivation
/// happening in the one direction that matters — the caller cannot pick.
#[tokio::test]
async fn a_manifest_request_naming_another_agent_is_served_the_sessions_own_row() {
    // Given
    let session = a_session()
        .await
        .persisted_as("cursor-cli")
        .with_file(".cursor/rules.md", b"# cursor\n")
        .with_file(".codex/config.toml", b"model = 'gpt'\n");

    // When
    let paths = session.manifest_paths_asked_as("codex").await;

    // Then
    assert!(
        paths.contains(&".cursor/rules.md".to_string()),
        "the cursor session's own row must be served; got {paths:?}"
    );
    assert!(
        !paths.contains(&".codex/config.toml".to_string()),
        "the row the request named must not widen what the session serves; got {paths:?}"
    );
}

/// And the read agrees with the manifest: a path only the *requested* row names is refused, so the
/// field cannot be used to reach a file the session's row withholds.
#[tokio::test]
async fn a_read_request_cannot_reach_a_path_only_the_agent_it_named_would_allow() {
    // Given
    let session = a_session()
        .await
        .persisted_as("cursor-cli")
        .with_file(".codex/config.toml", b"model = 'gpt'\n");

    // When
    let refusal = session.read_asked_as("codex", ".codex/config.toml").await;

    // Then
    assert_eq!(
        refusal
            .expect_err("a path outside the session's row must be refused")
            .code(),
        Code::PermissionDenied
    );
}

/// A `workspace` session runs no agent of its own, so it is served the shared base however the
/// request asks. This is the case the security argument turns on: a token for a session that syncs
/// nothing but `AGENTS.md` must not become a way to read another checkout's `.claude/`.
#[tokio::test]
async fn a_session_that_runs_no_agent_is_served_the_shared_base_however_the_request_asks() {
    // Given
    let session = a_session()
        .await
        .with_file(".claude/settings.json", b"{}\n")
        .with_file("AGENTS.md", b"# shared\n");

    // When
    let paths = session.manifest_paths_asked_as("claude").await;

    // Then
    assert_eq!(paths, vec!["AGENTS.md".to_string()]);
}

/// The one session type that is served a row *wider* than its own type names, and the reason the
/// derivation reads the pairing rather than the type alone: the codebase half of a split placement
/// is persisted as `workspace` — it runs no agent — while the agent that reads its guidance lives
/// on another daemon, whose `.session.yaml` this host cannot see. What it can see is the
/// back-pointer written with the session, and split placement is `claude-cli` only, so a paired
/// workspace session serves Claude's row.
///
/// Without this the split path would sync `AGENTS.md` and nothing else, which is the whole feature
/// silently switched off.
#[tokio::test]
async fn a_workspace_session_holding_a_split_agents_worktree_serves_claudes_row() {
    // Given
    let session = a_session_holding_a_split_agents_worktree()
        .await
        .with_file("CLAUDE.md", b"# rules\n")
        .with_file(".claude/settings.json", b"{}\n");

    // When
    let paths = session.manifest_paths_asked_as("claude").await;

    // Then
    assert!(
        paths.contains(&"CLAUDE.md".to_string())
            && paths.contains(&".claude/settings.json".to_string()),
        "the codebase half of a split placement must serve the agent's guidance; got {paths:?}"
    );
}

// ---------------------------------------------------------------------------
// The batched read
// ---------------------------------------------------------------------------

/// The whole setup prefetch in one call. Before this, populating a split session's context dir cost
/// 1 + N sequential peer round trips — a 120-file `.claude/skills/` tree was 121 calls before the
/// agent process existed. Every file still arrives byte-exact, tagged with the path it belongs to.
#[tokio::test]
async fn a_batch_carries_several_files_byte_for_byte_in_one_call() {
    // Given
    let session = a_session()
        .await
        .persisted_as("claude-cli")
        .with_file("CLAUDE.md", b"# rules\n")
        .with_file(".claude/settings.json", b"{\"a\":1}\n")
        .with_file(".mcp.json", b"");

    // When
    let files = session
        .read_batch(&["CLAUDE.md", ".claude/settings.json", ".mcp.json"])
        .await
        .expect("the batch must be served");

    // Then
    assert_eq!(
        files,
        BTreeMap::from([
            ("CLAUDE.md".to_string(), b"# rules\n".to_vec()),
            (".claude/settings.json".to_string(), b"{\"a\":1}\n".to_vec()),
            (".mcp.json".to_string(), Vec::new()),
        ])
    );
}

/// One unlisted path fails the whole batch, before a byte of any file flows. A partially served
/// batch would leave the caller unable to tell "the project does not ship that file" from "this
/// host would not serve it", and setup sync must fail loudly rather than start an agent against
/// guidance with a hole in it.
#[tokio::test]
async fn a_batch_naming_a_path_outside_the_allow_list_serves_none_of_it() {
    // Given
    let session = a_session()
        .await
        .persisted_as("claude-cli")
        .with_file("CLAUDE.md", b"# rules\n")
        .with_file(".env", b"SECRET=hunter2\n");

    // When
    let refusal = session.read_batch(&["CLAUDE.md", ".env"]).await;

    // Then
    assert_eq!(
        refusal
            .expect_err("a batch naming .env must be refused whole")
            .code(),
        Code::PermissionDenied
    );
}
