//! What `StreamStartSession` says while a session starts: each slow step — the worktree, the
//! semantic index when one was asked for, the agent — is announced as a `StartPhase` when it begins
//! and when it ends, all before the terminal result, so the create pane can show which step the
//! host is in rather than a disabled button.
//!
//! Feature docs: docs/ft/web/session-drawer.md (Start progress) and docs/ft/web/session-code-pane.md (Indexing indicator)
//!
//! A `claude-cli` session is the harness: it is the session type the create pane starts most, it
//! has all three steps, and it starts in-process here — a real project repository with an `origin`
//! pointing at itself, so the worktree's fetch succeeds, and `/bin/cat` as the `claude` binary, so
//! the agent's PTY spawn succeeds without the real CLI (the `claude_cli_session_acceptance.rs`
//! fixture).
//!
//! The semantic index is the one step this harness cannot finish: the crate is built without the
//! `local-model` feature, so asking for an index fails at its first act ("no embedder"). That is
//! still the step beginning, which is what the phase is for — and a failed step sends no end, the
//! stream terminating with the failure instead.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session::start_phase::{Boundary, Step};
use tddy_service::proto::session::start_session_event::Event as StartEvent;
use tddy_service::proto::session::{
    SessionService as SessionServiceTrait, StartSessionEvent, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_PROJECT_ID: &str = "start-phase-project";

/// Per event, not per start: the slowest step here is a local `git fetch` plus `git worktree add`.
const AN_EVENT_IS_DUE_WITHIN: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------------------------
// A daemon that can start a claude-cli session

struct ADaemonWithAProject {
    service: DaemonSessionHost,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
    _config: tempfile::TempDir,
}

fn a_daemon_with_a_project() -> ADaemonWithAProject {
    let repo = tempfile::tempdir().expect("a repository directory");
    a_repository_whose_origin_is_itself(repo.path());
    let sessions = tempfile::tempdir().expect("a sessions directory");
    register_project(&sessions.path().join("projects"), repo.path());
    let (config_dir, config) = a_config_launching_claude_as("/bin/cat");
    ADaemonWithAProject {
        service: a_session_host(config, sessions.path().to_path_buf()),
        _repo: repo,
        _sessions: sessions,
        _config: config_dir,
    }
}

/// The OS user the test runs as, so the claude-cli spawn needs no privilege drop.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn a_config_launching_claude_as(binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().expect("a config directory");
    let user = current_os_user();
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nclaude_cli:\n  binary_path: {binary}\n"
    );
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).expect("write the config");
    let config = DaemonConfig::load(&path).expect("the config parses");
    (dir, config)
}

fn a_session_host(config: DaemonConfig, sessions_base: PathBuf) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user = current_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| user.clone()));
    DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

fn a_repository_whose_origin_is_itself(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@e.st"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    git(
        dir,
        &["remote", "add", "origin", dir.to_str().expect("UTF-8")],
    );
    git(dir, &["push", "-q", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo: &Path) {
    std::fs::create_dir_all(projects_dir).expect("the projects directory");
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: start-phase\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo.display()
        ),
    )
    .expect("register the project");
}

fn git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|err| panic!("git {args:?} in {cwd:?}: {err}"));
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

fn a_claude_cli_start() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-4-8".to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------
// What the stream said

/// One event of a start, as the create pane reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    Began(Step),
    Ended(Step),
    Result,
    Failed(Code),
}

/// Everything a start's stream said, in order, up to and including how it ended.
async fn everything_the_start_said(
    service: &DaemonSessionHost,
    request: StartSessionRequest,
) -> (Vec<Said>, Option<Status>) {
    let mut stream = service
        .stream_start_session(Request::direct(request))
        .await
        .expect("StreamStartSession accepts the request")
        .into_inner();
    let mut said = Vec::new();
    let mut failure = None;
    while let Some(item) = next_item(&mut stream).await {
        match item {
            Ok(event) => said.extend(what_it_says(event)),
            Err(status) => {
                said.push(Said::Failed(status.code));
                failure = Some(status);
            }
        }
    }
    (said, failure)
}

fn what_it_says(event: StartSessionEvent) -> Option<Said> {
    match event.event.expect("every event carries a variant") {
        StartEvent::Phase(phase) => {
            let step = Step::try_from(phase.step).expect("a known step");
            match Boundary::try_from(phase.boundary).expect("a known boundary") {
                Boundary::Begin => Some(Said::Began(step)),
                Boundary::End => Some(Said::Ended(step)),
                Boundary::Unspecified => panic!("a phase without a boundary: {phase:?}"),
            }
        }
        StartEvent::Result(_) => Some(Said::Result),
        // This suite asks for no attachments; their progress is pinned in
        // `session_attach_staging_scope_acceptance.rs`.
        StartEvent::AttachmentProgress(_) => None,
    }
}

async fn next_item<T>(
    stream: &mut (impl Stream<Item = Result<T, Status>> + Unpin),
) -> Option<Result<T, Status>> {
    tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, stream.next())
        .await
        .expect("no start-session event arrived within the timeout")
}

// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn starting_a_session_streams_worktree_then_agent_phases_before_the_result() {
    // Given a daemon that can start a claude-cli session on a registered project
    let daemon = a_daemon_with_a_project();

    // When a session is started over the streaming RPC
    let (said, failure) = everything_the_start_said(&daemon.service, a_claude_cli_start()).await;

    // Then the worktree step, then the agent step, each began and ended before the one result
    assert_eq!(
        failure.map(|status| status.message),
        None,
        "the start succeeds"
    );
    assert_eq!(
        said,
        vec![
            Said::Began(Step::Worktree),
            Said::Ended(Step::Worktree),
            Said::Began(Step::Agent),
            Said::Ended(Step::Agent),
            Said::Result,
        ]
    );
}

#[tokio::test]
async fn semantic_index_phase_is_streamed_when_enabled() {
    // Given a daemon that can start a claude-cli session, built without a local embedding model
    let daemon = a_daemon_with_a_project();

    // When a session asking for a semantic index is started over the streaming RPC
    let (said, failure) = everything_the_start_said(
        &daemon.service,
        StartSessionRequest {
            semantic_index: true,
            ..a_claude_cli_start()
        },
    )
    .await;

    // Then the semantic index step began once the worktree had ended, and its failure — the one
    // thing this build can make of it — ended the stream with no end for that step and no result
    assert_eq!(
        said,
        vec![
            Said::Began(Step::Worktree),
            Said::Ended(Step::Worktree),
            Said::Began(Step::SemanticIndex),
            Said::Failed(Code::FailedPrecondition),
        ]
    );
    let failure = failure.expect("the start fails at the index");
    assert!(
        failure
            .message
            .starts_with("semantic index requested but no embedder is available"),
        "the failure names the missing embedder, was {:?}",
        failure.message
    );
}
