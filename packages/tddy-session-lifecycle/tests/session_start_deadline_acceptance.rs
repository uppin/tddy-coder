//! A session start that overruns `session_start_timeout_secs` fails `DEADLINE_EXCEEDED` naming the
//! step it was on, through each way a start reaches this daemon, and leaves nothing behind.
//!
//! The two ways in are the unary `StartSession` a client makes and the operator start a satellite
//! makes (`HostSessionStarter`). Both reach the one generic start, so both run under the deadline.
//!
//! A clone that never finishes needs no fake: the project's URL is an SSH remote at an address
//! nothing answers on (TEST-NET-1, RFC 5737), so `ssh` waits until the deadline cancels the clone.
//! The undo needs a start that answers inside its grace: a worktree cut whose `post-checkout` hook
//! outlasts the deadline and then returns, so the session it made is removed by the operator's undo.
//!
//! These live in this crate rather than in `tddy-agent-launch` because the host that builds the
//! `LaunchSessions` a start runs on is `DaemonSessionHost`, and this crate owns it.
//!
//! Feature docs: docs/ft/daemon/session-agent-roster.md (A start has a deadline)

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_extensions::{
    OperatorBranchChoice, OperatorSessionStart, OperatorStartOutcome, SessionStarter,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::operator_session_starter::HostSessionStarter;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "session-start-deadline-token";
const PROJECT: &str = "session-start-deadline-project";
const CUT_BRANCH: &str = "session-start-deadline-cut";

/// Past this the start is not bounded at all. The deadline under test is one second.
const A_START_THAT_HANGS: Duration = Duration::from_secs(60);

fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// The daemon config: the current user mapped, and a one-second session start deadline.
fn a_config_with_a_one_second_start_deadline() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().expect("a config directory");
    let user = current_os_user();
    let yaml = format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\nsession_start_timeout_secs: 1\n"
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
        Arc::new(CliSessionManager::new()),
        tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[(
            "claude",
            Path::new("/bin/cat"),
        )]),
    )
}

/// A daemon with one registered project, and the repository the project is served from.
struct ADaemon {
    service: Arc<DaemonSessionHost>,
    /// Where the project's worktrees are cut: the clone's destination, or the existing repository.
    repo: PathBuf,
    sessions: tempfile::TempDir,
    _config: tempfile::TempDir,
    _repository: Option<tempfile::TempDir>,
}

fn write_projects(sessions: &tempfile::TempDir, git_url: &str, main_repo_path: &Path) {
    let projects = sessions.path().join("projects");
    std::fs::create_dir_all(&projects).expect("a projects directory");
    std::fs::write(
        projects.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {PROJECT}\n    name: session-start-deadline\n    git_url: \"{git_url}\"\n    main_repo_path: {}\n",
            main_repo_path.display()
        ),
    )
    .expect("write the projects");
}

/// A project whose clone never finishes: it is cloned from an SSH remote nothing answers on.
fn a_daemon_whose_clone_never_finishes() -> ADaemon {
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let repo = sessions.path().join("repos").join(PROJECT);
    write_projects(&sessions, "ssh://git@192.0.2.1/nobody/nothing.git", &repo);
    let (config_dir, config) = a_config_with_a_one_second_start_deadline();
    ADaemon {
        service: Arc::new(a_session_host(config, sessions.path().to_path_buf())),
        repo,
        sessions,
        _config: config_dir,
        _repository: None,
    }
}

/// Written by the worktree cut's `post-checkout` hook as the cut reaches it.
fn cut_began_marker(repo: &Path) -> PathBuf {
    repo.join(".git").join("cut-began")
}

/// A project whose worktree cut outlasts the deadline: its `post-checkout` hook leaves a file in
/// the new worktree (as an agent would, which `git worktree remove` refuses), marks that the cut
/// began, then sleeps past the one-second deadline and returns.
fn a_daemon_whose_worktree_cut_outlasts_the_deadline() -> ADaemon {
    let repository = tempfile::tempdir().expect("a repository directory");
    a_repository_whose_origin_is_itself(repository.path());
    let cut_began = cut_began_marker(repository.path());
    let hook = repository
        .path()
        .join(".git")
        .join("hooks")
        .join("post-checkout");
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
    write_projects(&sessions, "", repository.path());
    let (config_dir, config) = a_config_with_a_one_second_start_deadline();
    ADaemon {
        service: Arc::new(a_session_host(config, sessions.path().to_path_buf())),
        repo: repository.path().to_path_buf(),
        sessions,
        _config: config_dir,
        _repository: Some(repository),
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

fn branch_exists(repo: &Path, branch: &str) -> bool {
    std::process::Command::new("git")
        .current_dir(repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .status()
        .expect("git runs")
        .success()
}

fn entries_under(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|e| {
                    e.expect("a directory entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .unwrap_or_default()
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

/// The operator start a satellite makes: keyed by the OS user, with no session token.
fn an_operator_claude_cli_start() -> OperatorSessionStart {
    OperatorSessionStart {
        os_user: current_os_user(),
        project_id: PROJECT.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-4-8".to_string(),
        initial_prompt: String::new(),
        branch: OperatorBranchChoice::NewBranchFromBase {
            new_branch_name: CUT_BRANCH.to_string(),
            integration_base_ref: String::new(),
        },
    }
}

/// An operator's workspace session on a branch it cuts, which answers once its worktree is cut.
fn an_operator_workspace_start() -> OperatorSessionStart {
    OperatorSessionStart {
        session_type: "workspace".to_string(),
        model: String::new(),
        ..an_operator_claude_cli_start()
    }
}

/// The message of a start that failed `DEADLINE_EXCEEDED`.
fn a_deadline_exceeded(result: Result<impl std::fmt::Debug, Status>) -> String {
    let status = result.expect_err("a start past its deadline does not succeed");
    assert_eq!(status.code, Code::DeadlineExceeded, "{status:?}");
    status.message
}

// -- acceptance ---------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_unary_start_whose_clone_never_finishes_fails_deadline_exceeded_naming_the_clone() {
    // Given — a project whose clone never finishes, under a one-second session start deadline
    let daemon = a_daemon_whose_clone_never_finishes();

    // When — the unary StartSession is asked for a claude-cli session on it
    let answered: Result<_, Status> = tokio::time::timeout(
        A_START_THAT_HANGS,
        SessionServiceTrait::start_session(&*daemon.service, Request::direct(a_claude_cli_start())),
    )
    .await
    .expect("the start ended with no deadline of its own");

    // Then — DEADLINE_EXCEEDED naming the clone, and no session directory is left behind
    let message = a_deadline_exceeded(answered.map(|response| response.into_inner()));
    assert!(message.contains("clone"), "{message}");
    assert!(
        entries_under(&daemon.sessions.path().join("sessions")).is_empty(),
        "no session directory left behind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_operator_start_whose_clone_never_finishes_fails_deadline_exceeded_naming_the_clone() {
    // Given — a project whose clone never finishes, started by an operator
    let daemon = a_daemon_whose_clone_never_finishes();
    let starter = HostSessionStarter::new(Arc::clone(&daemon.service));

    // When — the operator's start is asked for
    let answered: Result<OperatorStartOutcome, Status> = tokio::time::timeout(
        A_START_THAT_HANGS,
        starter.start_for_operator(an_operator_claude_cli_start()),
    )
    .await
    .expect("the operator start ended with no deadline of its own");

    // Then — DEADLINE_EXCEEDED naming the clone, and no session directory is left behind
    let message = a_deadline_exceeded(answered);
    assert!(message.contains("clone"), "{message}");
    assert!(
        entries_under(&daemon.sessions.path().join("sessions")).is_empty(),
        "no session directory left behind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_operator_start_that_answers_inside_its_grace_is_undone_by_its_operator_key() {
    // Given — a project whose worktree cut outlasts the deadline and then answers with a session
    let daemon = a_daemon_whose_worktree_cut_outlasts_the_deadline();
    let starter = HostSessionStarter::new(Arc::clone(&daemon.service));

    // When — an operator's workspace session is started on it
    let answered: Result<OperatorStartOutcome, Status> = tokio::time::timeout(
        A_START_THAT_HANGS,
        starter.start_for_operator(an_operator_workspace_start()),
    )
    .await
    .expect("the operator start ended with no deadline of its own");

    // Then — DEADLINE_EXCEEDED naming the worktree, and the session and the branch it cut are gone.
    // The cut began (the marker), so the branch and the worktree existed before they were removed.
    let message = a_deadline_exceeded(answered);
    assert!(message.contains("worktree"), "{message}");
    assert!(
        cut_began_marker(&daemon.repo).exists(),
        "the start reached its worktree cut"
    );
    assert!(
        entries_under(&daemon.sessions.path().join("sessions")).is_empty(),
        "no session directory left behind"
    );
    assert_eq!(
        worktrees_of(&daemon.repo).len(),
        1,
        "the session's worktree was removed"
    );
    assert!(
        !branch_exists(&daemon.repo, CUT_BRANCH),
        "the branch the start cut was removed"
    );
}
