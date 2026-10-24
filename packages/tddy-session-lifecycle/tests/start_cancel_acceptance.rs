//! Acceptance: a streamed session start runs as a task, names it first, and cancelling that task ends
//! the start CANCELLED — its clone killed, nothing left behind.
//!
//! A slow clone needs no fake: the project's URL is an SSH remote at an address nothing answers on
//! (TEST-NET-1, RFC 5737), so `ssh` waits out its connect timeout — long enough to cancel inside.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-10-24-start-progress-start-cancel.md

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
    let task_id = match next_event(&mut stream)
        .await
        .expect("a first event")
        .expect("not an error")
        .event
    {
        Some(StartEvent::Started(started)) => started.task_id,
        other => panic!("the first event names the task: {other:?}"),
    };
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
    let mut ended = None;
    while let Some(item) = next_event(&mut stream).await {
        if let Err(status) = item {
            ended = Some(status);
        }
    }

    // Then — cancelled promptly, and nothing of the clone or the session is left
    let status = ended.expect("a cancelled start ends with a status, not a result");
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
