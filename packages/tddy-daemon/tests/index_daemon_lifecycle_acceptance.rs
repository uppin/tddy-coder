//! What `tddy-daemon` owes the index daemon: start it when something first needs it, hand the same
//! process to the next caller, replace it once it has exited, stop it when it has gone idle, and
//! stop it — rather than orphan it — when the daemon itself shuts down.
//!
//! The process each test points the spawn at is a shell script the test writes, not a fixture
//! `[[bin]]`. Two reasons: `CARGO_BIN_EXE_tddy-index-daemon` names another package's binary and so
//! never reaches this suite, and
//! `docs/dev/todo/2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md`
//! records what a test-only `[[bin]]` costs the manifest of the crate that declares it. The script
//! honours the same `--grpc-uds` contract the real binary does: refuse a path something may
//! already be serving on, bind it, and stay up until it is stopped.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use tddy_daemon::config::{DaemonConfig, IndexDaemonConfig};
use tddy_daemon::index_daemon::{IndexDaemonError, IndexDaemonRegistry, IndexDaemonSpawn};
use tddy_task::{TaskId, TaskRegistry};
use tempfile::TempDir;

/// Long enough that nothing but the child-death check can end a readiness wait, so a test that
/// returns quickly proves the check fired rather than that a budget was small.
const A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT: Duration = Duration::from_secs(300);

/// How long a test is willing to wait for a bounded operation before calling it hung.
const THIS_TEST_S_PATIENCE: Duration = Duration::from_secs(10);

/// A scratch directory holding the stand-in index daemon, the socket it is told to bind, and the
/// argv it recorded.
struct AnIndexDaemonHost {
    dir: TempDir,
}

/// A stand-in that binds the socket it was given, announces itself, and stays up until stopped.
fn an_index_daemon_that_binds_and_stays_up() -> AnIndexDaemonHost {
    let host = AnIndexDaemonHost {
        dir: TempDir::new().expect("a scratch directory for the index daemon"),
    };
    host.write_program(&format!(
        "#!/bin/sh\n\
         printf '%s' \"$*\" > {argv}\n\
         if [ -e \"$2\" ]; then exit 3; fi\n\
         : > \"$2\"\n\
         echo \"listening on $2\" >&2\n\
         exec sleep 86400\n",
        argv = host.recorded_argv_path().display(),
    ));
    host
}

/// A stand-in that starts and stays up without ever binding the socket it was given.
fn an_index_daemon_that_never_binds() -> AnIndexDaemonHost {
    let host = AnIndexDaemonHost {
        dir: TempDir::new().expect("a scratch directory for the index daemon"),
    };
    host.write_program("#!/bin/sh\nexec sleep 86400\n");
    host
}

/// A stand-in that says why it cannot serve and exits, binding nothing.
fn an_index_daemon_that_exits_before_binding() -> AnIndexDaemonHost {
    let host = AnIndexDaemonHost {
        dir: TempDir::new().expect("a scratch directory for the index daemon"),
    };
    host.write_program(
        "#!/bin/sh\n\
         echo 'no rust-analyzer on PATH' >&2\n\
         exit 17\n",
    );
    host
}

impl AnIndexDaemonHost {
    fn write_program(&self, script: &str) {
        let path = self.program_path();
        std::fs::write(&path, script).expect("write the stand-in index daemon");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make the stand-in index daemon executable");
    }

    fn program_path(&self) -> PathBuf {
        self.dir.path().join("tddy-index-daemon")
    }

    fn socket_path(&self) -> PathBuf {
        self.dir.path().join("index.sock")
    }

    fn recorded_argv_path(&self) -> PathBuf {
        self.dir.path().join("argv")
    }

    /// The command line the stand-in was actually invoked with.
    fn argv_it_was_given(&self) -> String {
        std::fs::read_to_string(self.recorded_argv_path())
            .expect("the stand-in index daemon recorded no argv")
    }

    /// A spawn plan with budgets loose enough that no test waits on them by accident.
    fn spawn_plan(&self) -> IndexDaemonSpawn {
        IndexDaemonSpawn {
            program: self.program_path(),
            socket_path: self.socket_path(),
            ready_timeout: A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT,
            idle_timeout: Duration::from_secs(600),
        }
    }
}

/// The kinds of every task the daemon is running, so a test can say exactly what is there.
async fn running_task_kinds(tasks: &TaskRegistry) -> Vec<String> {
    let mut kinds: Vec<String> = tasks
        .list()
        .await
        .into_iter()
        .map(|handle| handle.kind.clone())
        .collect();
    kinds.sort();
    kinds
}

/// The pid of the child the index daemon's task registered for the cancellation escalation net.
async fn the_child_process_of(tasks: &TaskRegistry, task_id: &TaskId) -> u32 {
    let handle = tasks.get(task_id).await.expect("the index daemon's task");
    let pids = handle.pid_slot.lock().expect("the task's pid slot").clone();
    assert_eq!(
        pids.len(),
        1,
        "expected exactly one child pid, got {pids:?}"
    );
    pids[0]
}

/// Bounded wait for a pid to leave the process table, so "stopped" means the child is gone rather
/// than that its task said so.
async fn wait_until_the_process_is_gone(pid: u32) {
    let alive = || unsafe { libc::kill(pid as i32, 0) } == 0;
    tokio::time::timeout(THIS_TEST_S_PATIENCE, async {
        while alive() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("pid {pid} was still running"));
}

/// Bounded wait for a task to report a terminal status.
async fn wait_until_terminal(tasks: &TaskRegistry, task_id: &TaskId) {
    tokio::time::timeout(THIS_TEST_S_PATIENCE, async {
        loop {
            match tasks.get(task_id).await {
                Some(handle) if handle.status().is_terminal() => return,
                None => return,
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .expect("the index daemon's task never reached a terminal state");
}

/// The refusal a caller gets when the process died before it was ready to serve.
struct DiedBeforeReady(String);

fn assert_died_before_ready(error: IndexDaemonError) -> DiedBeforeReady {
    match error {
        IndexDaemonError::DiedBeforeReady { reason } => DiedBeforeReady(reason),
        other => panic!("expected a died-before-ready refusal but got {other:?}"),
    }
}

impl DiedBeforeReady {
    fn mentioning(self, fragment: &str) -> Self {
        assert!(
            self.0.contains(fragment),
            "expected the refusal to mention '{fragment}', was '{}'",
            self.0
        );
        self
    }
}

/// The refusal a caller gets when the process is running but could not be dialled.
struct NotDialable(String);

fn assert_not_dialable(error: IndexDaemonError) -> NotDialable {
    match error {
        IndexDaemonError::NotDialable { detail } => NotDialable(detail),
        other => panic!("expected an undialable-socket refusal but got {other:?}"),
    }
}

impl NotDialable {
    fn mentioning(self, fragment: &str) -> Self {
        assert!(
            self.0.contains(fragment),
            "expected the refusal to mention '{fragment}', was '{}'",
            self.0
        );
        self
    }
}

fn assert_not_ready_in_time(error: IndexDaemonError, expected_seconds: u64) {
    match error {
        IndexDaemonError::NotReadyInTime { seconds } => assert_eq!(seconds, expected_seconds),
        other => panic!("expected a readiness-budget refusal but got {other:?}"),
    }
}

fn assert_stopped(error: IndexDaemonError) {
    match error {
        IndexDaemonError::Stopped => {}
        other => panic!("expected a shutting-down refusal but got {other:?}"),
    }
}

#[tokio::test]
async fn starts_the_index_daemon_on_the_first_request_that_needs_it() {
    // Given a daemon managing an index daemon that nothing has asked for yet
    let host = an_index_daemon_that_binds_and_stays_up();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(host.spawn_plan(), tasks.clone());
    assert_eq!(running_task_kinds(&tasks).await, Vec::<String>::new());

    // When something asks for it
    let running = registry
        .get_or_spawn()
        .await
        .expect("the first request's index daemon");

    // Then it was started, and told to serve on the socket the daemon will dial
    assert_eq!(running_task_kinds(&tasks).await, vec!["index-daemon"]);
    assert_eq!(running.socket_path, host.socket_path());
    assert_eq!(
        host.argv_it_was_given(),
        format!("--grpc-uds {}", host.socket_path().display())
    );
}

#[tokio::test]
async fn reuses_the_running_index_daemon_for_a_second_request() {
    // Given an index daemon already started for one request
    let host = an_index_daemon_that_binds_and_stays_up();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(host.spawn_plan(), tasks.clone());
    let first = registry
        .get_or_spawn()
        .await
        .expect("the first request's index daemon");

    // When a second request asks for one
    let second = registry
        .get_or_spawn()
        .await
        .expect("the second request's index daemon");

    // Then both requests were handed the one process
    assert_eq!(first.task_id, second.task_id);
    assert_eq!(running_task_kinds(&tasks).await, vec!["index-daemon"]);
}

#[tokio::test]
async fn restarts_the_index_daemon_after_it_exits() {
    // Given an index daemon that has exited, leaving behind the socket it bound — which the next
    // process would otherwise refuse to bind
    let host = an_index_daemon_that_binds_and_stays_up();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(host.spawn_plan(), tasks.clone());
    let first = registry
        .get_or_spawn()
        .await
        .expect("the first request's index daemon");
    tasks.cancel_task(&first.task_id).await;
    wait_until_terminal(&tasks, &first.task_id).await;
    assert!(host.socket_path().exists());

    // When the next request asks for one
    let second = registry
        .get_or_spawn()
        .await
        .expect("a replacement index daemon");

    // Then a fresh process was started rather than the dead one handed back
    assert_ne!(first.task_id, second.task_id);
}

#[tokio::test]
async fn stops_the_index_daemon_when_the_daemon_shuts_down() {
    // Given a running index daemon
    let host = an_index_daemon_that_binds_and_stays_up();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(host.spawn_plan(), tasks.clone());
    let running = registry.get_or_spawn().await.expect("the index daemon");
    let child = the_child_process_of(&tasks, &running.task_id).await;

    // When the daemon shuts down
    registry.shutdown().await;

    // Then the child process is gone rather than outliving the daemon that started it
    wait_until_the_process_is_gone(child).await;
    assert!(registry.running().await.is_none());
}

#[tokio::test]
async fn stops_the_index_daemon_once_it_has_gone_idle() {
    // Given a running index daemon left unused for longer than its idle budget
    let host = an_index_daemon_that_binds_and_stays_up();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(
        IndexDaemonSpawn {
            idle_timeout: Duration::from_millis(50),
            ..host.spawn_plan()
        },
        tasks.clone(),
    );
    let running = registry.get_or_spawn().await.expect("the index daemon");
    let child = the_child_process_of(&tasks, &running.task_id).await;
    tokio::time::sleep(Duration::from_millis(120)).await;

    // When the daemon's reaper runs
    let reaped = registry.reap_idle().await;

    // Then the process it was holding was stopped
    assert_eq!(reaped, Some(running.task_id.clone()));
    wait_until_the_process_is_gone(child).await;
}

#[tokio::test]
async fn fails_fast_when_the_index_daemon_dies_before_it_is_ready() {
    // Given an index daemon that exits before binding anything, under a readiness budget far
    // longer than this test will wait
    let host = an_index_daemon_that_exits_before_binding();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(host.spawn_plan(), tasks.clone());

    // When something asks for it
    let refusal = tokio::time::timeout(THIS_TEST_S_PATIENCE, registry.get_or_spawn())
        .await
        .expect("the request returned rather than waiting out the readiness budget")
        .expect_err("a dead index daemon is not a usable one");

    // Then the refusal says how it died instead of reporting a timeout
    assert_died_before_ready(refusal)
        .mentioning("17")
        .mentioning("no rust-analyzer on PATH");
}

#[tokio::test]
async fn refuses_a_connection_to_an_index_daemon_it_cannot_dial() {
    // Given a running index daemon whose socket cannot be dialled — the stand-in leaves a plain
    // file where the real binary leaves a listening socket
    let host = an_index_daemon_that_binds_and_stays_up();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(host.spawn_plan(), tasks.clone());
    registry.get_or_spawn().await.expect("the index daemon");

    // When something asks for a connection to it
    let refusal = registry
        .connect()
        .await
        .expect_err("an undialable index daemon is not a usable one");

    // Then the refusal names the socket it tried, rather than a connection being skipped
    assert_not_dialable(refusal).mentioning(&host.socket_path().display().to_string());
}

#[tokio::test]
async fn refuses_an_index_daemon_that_never_binds_its_socket() {
    // Given an index daemon that starts and stays up without binding anything, under a one-second
    // readiness budget
    let host = an_index_daemon_that_never_binds();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(
        IndexDaemonSpawn {
            ready_timeout: Duration::from_secs(1),
            ..host.spawn_plan()
        },
        tasks.clone(),
    );

    // When something asks for it
    let refusal = registry
        .get_or_spawn()
        .await
        .expect_err("an index daemon that bound nothing is not reachable");

    // Then it is refused on the readiness budget, because starting is not the same as serving
    assert_not_ready_in_time(refusal, 1);
}

#[tokio::test]
async fn starts_no_index_daemon_once_the_daemon_has_shut_down() {
    // Given a daemon that has shut its index daemon down
    let host = an_index_daemon_that_binds_and_stays_up();
    let tasks = TaskRegistry::new();
    let registry = IndexDaemonRegistry::new(host.spawn_plan(), tasks.clone());
    registry.shutdown().await;

    // When a late request asks for one
    let refusal = registry
        .get_or_spawn()
        .await
        .expect_err("a daemon on its way out starts nothing");

    // Then it is refused, rather than a process being started that nothing will stop
    assert_stopped(refusal);
    assert_eq!(running_task_kinds(&tasks).await, Vec::<String>::new());
}

#[test]
fn manages_no_index_daemon_unless_the_configuration_asks_for_one() {
    // Given the configuration a deployment that has not asked for an index daemon runs
    let config = DaemonConfig::default();

    // Then no index daemon is named, so none is managed
    assert_eq!(config.index_daemon, None);
}

#[test]
fn takes_the_program_the_socket_and_the_budgets_the_configuration_names() {
    // Given an index daemon section naming all four
    let configured = IndexDaemonConfig {
        binary_path: Some(PathBuf::from("/opt/tddy/bin/tddy-index-daemon")),
        socket_path: Some(PathBuf::from("/run/user/1000/code-index.sock")),
        ready_timeout_secs: 90,
        idle_timeout_secs: 900,
    };

    // When the spawn plan is derived from it
    let plan = IndexDaemonSpawn::from_config(&configured);

    // Then the plan is what the section asked for
    assert_eq!(
        plan,
        IndexDaemonSpawn {
            program: PathBuf::from("/opt/tddy/bin/tddy-index-daemon"),
            socket_path: PathBuf::from("/run/user/1000/code-index.sock"),
            ready_timeout: Duration::from_secs(90),
            idle_timeout: Duration::from_secs(900),
        }
    );
}
