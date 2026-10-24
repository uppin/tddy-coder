//! Acceptance: a streamed session start runs as a task, names it first, and cancelling that task ends
//! the start CANCELLED — its clone killed, nothing left behind.
//!
//! A slow clone needs no fake: the project's URL is an SSH remote at an address nothing answers on
//! (TEST-NET-1, RFC 5737), so `ssh` waits out its connect timeout — long enough to cancel inside.
//! A slow worktree cut needs none either: the project's repository carries a `post-checkout` hook,
//! which `git worktree add` runs, that says it began and then sleeps.
//!
//! Feature docs: docs/ft/daemon/background-tasks.md (§ Session starts run as tasks)

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{Stream, StreamExt};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session::start_session_event::Event as StartEvent;
use tddy_service::proto::session::{
    SessionService as SessionServiceTrait, StartSessionEvent, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "start-cancel-token";
const PROJECT: &str = "start-cancel-project";

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
    (dir, DaemonConfig::load(&path).expect("the config parses"))
}

fn a_session_host(config: DaemonConfig, sessions_base: PathBuf) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user = current_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| user.clone()));
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

/// A daemon whose project must be cloned from `git_url` into `<sessions>/repos/<project>`.
struct ADaemon {
    service: DaemonSessionHost,
    working_copy: PathBuf,
    sessions: tempfile::TempDir,
    _config: tempfile::TempDir,
}

fn a_daemon_cloning_from(git_url: &str) -> ADaemon {
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let working_copy = sessions.path().join("repos").join(PROJECT);
    let projects = sessions.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(
        projects.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT}\n    name: start-cancel\n    git_url: \"{git_url}\"\n    main_repo_path: {}\n",
            working_copy.display()
        ),
    )
    .unwrap();
    let (config_dir, config) = a_config_launching_claude_as("/bin/cat");
    ADaemon {
        service: a_session_host(config, sessions.path().to_path_buf()),
        working_copy,
        sessions,
        _config: config_dir,
    }
}

/// A daemon whose project is already on disk, at a repository whose worktree cut is slow: its
/// `post-checkout` hook leaves an untracked file in the new worktree (as an agent would, and which
/// makes `git worktree remove` refuse it), touches `cut_began`, and then sleeps.
struct ADaemonWithASlowCut {
    service: DaemonSessionHost,
    repo: tempfile::TempDir,
    cut_began: PathBuf,
    sessions: tempfile::TempDir,
    _config: tempfile::TempDir,
}

fn a_daemon_whose_worktree_cut_is_slow() -> ADaemonWithASlowCut {
    let repo = tempfile::tempdir().expect("a repository directory");
    a_repository_whose_origin_is_itself(repo.path());
    let cut_began = repo.path().join(".git").join("cut-began");
    let hook = repo.path().join(".git").join("hooks").join("post-checkout");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\ntouch scratch-the-agent-left\ntouch '{}'\nsleep 3\n",
            cut_began.display()
        ),
    )
    .expect("write the hook");
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("make the hook executable");
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let projects = sessions.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(
        projects.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT}\n    name: start-cancel\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo.path().display()
        ),
    )
    .unwrap();
    let (config_dir, config) = a_config_launching_claude_as("/bin/cat");
    ADaemonWithASlowCut {
        service: a_session_host(config, sessions.path().to_path_buf()),
        repo,
        cut_began,
        sessions,
        _config: config_dir,
    }
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

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("git {args:?} in {cwd:?}: {err}"));
    assert!(out.status.success(), "git {args:?} failed in {cwd:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn worktrees_of(repo: &Path) -> Vec<String> {
    git(repo, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|line| line.strip_prefix("worktree ").map(str::to_string))
        .collect()
}

async fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < deadline, "{path:?} within 30 s");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn the_task_it_named(
    stream: &mut (impl Stream<Item = Result<StartSessionEvent, Status>> + Unpin),
) -> String {
    match next_event(stream)
        .await
        .expect("a first event")
        .expect("not an error")
        .event
    {
        Some(StartEvent::Started(started)) => started.task_id,
        other => panic!("the first event names the task: {other:?}"),
    }
}

async fn how_it_ended(
    stream: &mut (impl Stream<Item = Result<StartSessionEvent, Status>> + Unpin),
) -> Status {
    let mut ended = None;
    while let Some(item) = next_event(stream).await {
        match item {
            Err(status) => ended = Some(status),
            Ok(StartSessionEvent {
                event: Some(StartEvent::Result(result)),
            }) => panic!("a cancelled start answers no result: {result:?}"),
            Ok(_) => {}
        }
    }
    ended.expect("a cancelled start ends with a status")
}

fn a_claude_cli_start() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: PROJECT.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-4-8".to_string(),
        ..Default::default()
    }
}

async fn next_event(
    stream: &mut (impl Stream<Item = Result<StartSessionEvent, Status>> + Unpin),
) -> Option<Result<StartSessionEvent, Status>> {
    tokio::time::timeout(Duration::from_secs(30), stream.next())
        .await
        .expect("an event within 30 s")
}

fn entries_under(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_streamed_start_names_its_task_first() {
    // Given — a start that will hang in its clone
    let daemon = a_daemon_cloning_from("ssh://git@192.0.2.1/nobody/nothing.git");

    // When
    let mut stream = daemon
        .service
        .stream_start_session(Request::direct(a_claude_cli_start()))
        .await
        .expect("the start is accepted")
        .into_inner();
    let first = next_event(&mut stream)
        .await
        .expect("a first event")
        .expect("not an error");

    // Then
    assert!(
        matches!(&first.event, Some(StartEvent::Started(started)) if !started.task_id.is_empty()),
        "the first event names the task the start runs as: {first:?}"
    );

    // Cleanup: the clone is stopped here, so the runtime does not wait out ssh's connect timeout.
    if let Some(StartEvent::Started(started)) = first.event {
        daemon
            .service
            .task_registry()
            .cancel_task(&tddy_task::TaskId(started.task_id))
            .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_start_that_is_cloning_ends_it_cancelled_and_leaves_no_repository() {
    // Given — a start whose clone is waiting on an address nothing answers
    let daemon = a_daemon_cloning_from("ssh://git@192.0.2.1/nobody/nothing.git");
    let mut stream = daemon
        .service
        .stream_start_session(Request::direct(a_claude_cli_start()))
        .await
        .expect("the start is accepted")
        .into_inner();
    let task_id = the_task_it_named(&mut stream).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let cancelled_at = Instant::now();

    // When
    assert!(
        daemon
            .service
            .task_registry()
            .cancel_task(&tddy_task::TaskId(task_id))
            .await,
        "the start's task is cancellable"
    );
    let status = how_it_ended(&mut stream).await;

    // Then — cancelled promptly, and nothing of the clone or the session is left
    assert_eq!(status.code, Code::Cancelled, "ended CANCELLED: {status:?}");
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(10),
        "it ended when cancelled, not when ssh gave up: {:?}",
        cancelled_at.elapsed()
    );
    assert!(!daemon.working_copy.exists(), "no repository left behind");
    assert!(
        entries_under(daemon.working_copy.parent().unwrap())
            .iter()
            .all(|name| !name.contains("partial")),
        "no partial clone left behind"
    );
    assert!(
        entries_under(&daemon.sessions.path().join("sessions")).is_empty(),
        "no session directory left behind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_during_the_worktree_cut_leaves_no_session_directory_or_worktree() {
    // Given — a start whose worktree is being cut
    let daemon = a_daemon_whose_worktree_cut_is_slow();
    let mut stream = daemon
        .service
        .stream_start_session(Request::direct(a_claude_cli_start()))
        .await
        .expect("the start is accepted")
        .into_inner();
    let task_id = the_task_it_named(&mut stream).await;
    wait_for(&daemon.cut_began).await;

    // When
    assert!(
        daemon
            .service
            .task_registry()
            .cancel_task(&tddy_task::TaskId(task_id))
            .await,
        "the start's task is cancellable"
    );
    let status = how_it_ended(&mut stream).await;

    // Then — cancelled, and neither the session nor its worktree survived it
    assert_eq!(status.code, Code::Cancelled, "ended CANCELLED: {status:?}");
    assert!(
        entries_under(&daemon.sessions.path().join("sessions")).is_empty(),
        "no session directory left behind"
    );
    assert_eq!(
        worktrees_of(daemon.repo.path()).len(),
        1,
        "no worktree left behind beside the main checkout:\n{}",
        git(daemon.repo.path(), &["worktree", "list", "--porcelain"])
    );
}
