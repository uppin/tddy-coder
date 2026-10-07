//! A host that wants to know which processes this crate starts hands its registry an observer.
//!
//! The one process this crate starts is the language server, and the observer is told two things
//! about it: that it exists (with its program, arguments, working directory and pid) and how it
//! ended. Driven against the deterministic `fake_lsp`, never a real language server.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_lsp::{
    Language, LaunchSpec, LspAllowList, LspError, LspKey, LspRegistry, ProcessOutcome,
    ProcessStart, ProcessToken, SpawnObserver,
};
use tddy_task::TaskRegistry;

/// How long a test waits for an end it expects to be reported.
const LONG_ENOUGH_FOR_AN_END: Duration = Duration::from_secs(5);

/// A process that was started, with how it ended once it has.
type Kept = (ProcessStart, Option<ProcessOutcome>);

/// An observer that keeps what it is told. A token is an index into what was kept.
#[derive(Clone, Default)]
struct Collected {
    kept: Arc<Mutex<Vec<Kept>>>,
}

impl Collected {
    fn processes(&self) -> Vec<Kept> {
        self.kept.lock().expect("collected").clone()
    }

    /// The first process's start and outcome once it has ended, or `None` if it never does.
    async fn the_first_process_once_it_has_ended(&self) -> Option<(ProcessStart, ProcessOutcome)> {
        tokio::time::timeout(LONG_ENOUGH_FOR_AN_END, async {
            loop {
                if let Some((start, Some(outcome))) = self.processes().into_iter().next() {
                    return (start, outcome);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .ok()
    }
}

impl SpawnObserver for Collected {
    fn started(&self, process: &ProcessStart) -> ProcessToken {
        let mut kept = self.kept.lock().expect("collected");
        kept.push((process.clone(), None));
        ProcessToken(kept.len() as u64 - 1)
    }

    fn ended(&self, token: ProcessToken, outcome: &ProcessOutcome) {
        self.kept.lock().expect("collected")[token.0 as usize].1 = Some(outcome.clone());
    }
}

fn a_registry_over_the_fake(args: &[&str], observer: &Collected) -> LspRegistry {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"));
    spec.args = args.iter().map(|arg| arg.to_string()).collect();
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
        .with_spawn_observer(Arc::new(observer.clone()))
}

/// A workspace root that exists, so the server is started in it.
fn an_existing_root() -> PathBuf {
    std::env::temp_dir()
}

fn the_key_of(root: &Path) -> LspKey {
    LspKey {
        root: root.to_path_buf(),
        language: Language::Rust,
    }
}

#[tokio::test]
async fn a_language_server_launch_is_reported_with_its_program_args_cwd_and_pid() {
    // Given a registry that reports to an observer, over a workspace root that exists
    let observer = Collected::default();
    let tasks = TaskRegistry::new();
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    let registry = LspRegistry::new(allow, tasks.clone(), Duration::from_secs(60))
        .with_spawn_observer(Arc::new(observer.clone()));
    let root = an_existing_root();

    // When a server is spawned for it
    let service = registry
        .get_or_spawn(the_key_of(&root))
        .await
        .expect("the fake server starts");

    // Then one start was reported, naming the program, the root as its cwd, and the server's pid
    let registered_pids = tasks
        .get(&service.task_id)
        .await
        .expect("the server's task")
        .pid_slot
        .lock()
        .expect("the pid slot")
        .clone();
    let started: Vec<ProcessStart> = observer
        .processes()
        .into_iter()
        .map(|(start, _)| start)
        .collect();
    assert_eq!(started.len(), 1, "expected one start, found {started:?}");
    assert_eq!(started[0].program, env!("CARGO_BIN_EXE_fake_lsp"));
    assert_eq!(started[0].cwd, Some(root));
    assert_eq!(started[0].pid.map(|pid| vec![pid]), Some(registered_pids));
}

#[tokio::test]
async fn a_language_server_that_is_shut_down_is_reported_ended_with_how_it_died() {
    // Given a registry with a live server and an observer
    let observer = Collected::default();
    let registry = a_registry_over_the_fake(&[], &observer);
    let root = an_existing_root();
    registry
        .get_or_spawn(the_key_of(&root))
        .await
        .expect("the fake server starts");

    // When every server is shut down
    registry.shutdown_all().await;

    // Then the server's end is reported: the graceful exit, or the kill that follows it
    let (_, outcome) = observer
        .the_first_process_once_it_has_ended()
        .await
        .expect("an end was reported");
    assert!(
        matches!(
            outcome,
            ProcessOutcome::Signalled { signal: 9 } | ProcessOutcome::Exited { code: 0 }
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_server_that_exits_before_the_handshake_is_reported_ended_with_its_exit_code() {
    // Given a registry over a server that exits as soon as it starts
    let observer = Collected::default();
    let registry = a_registry_over_the_fake(&["--exit-immediately"], &observer);
    let root = an_existing_root();

    // When a server is requested
    let refusal = registry.get_or_spawn(the_key_of(&root)).await.err();

    // Then the request failed because the server went away
    assert!(
        matches!(refusal, Some(LspError::ServerExited)),
        "{refusal:?}"
    );
    // And the end is reported with the exit code, although initialization never finished
    let (_, outcome) = observer
        .the_first_process_once_it_has_ended()
        .await
        .expect("an end was reported");
    assert_eq!(outcome, ProcessOutcome::Exited { code: 0 });
}
