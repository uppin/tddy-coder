//! [`CliSessionManager::kill_all`] runs at daemon shutdown — on Tddy Desktop, on the UI thread — so
//! it must finish within its own deadline even when the blocking pool is saturated by work it does
//! not own (the 2026-11-26 incident: a quit queued behind ~2,000 leaked git jobs for 7 minutes).

use super::{CliSessionManager, RecordedTerminal};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The 5 s grace `kill_all` gives a cancelled process before SIGKILL, plus scheduling slack.
const KILL_ALL_DEADLINE: Duration = Duration::from_secs(7);

/// A runtime whose whole blocking pool is a single thread.
fn a_runtime_with_one_blocking_thread() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .expect("runtime builds")
}

/// Occupy the runtime's only blocking thread until the returned sender is sent to or dropped.
async fn saturate_the_blocking_pool() -> mpsc::Sender<()> {
    let (release, parked) = mpsc::channel::<()>();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let _ = started_tx.send(());
        let _ = parked.recv();
    });
    started_rx.await.expect("the parked job starts");
    release
}

#[test]
fn kill_all_finishes_within_its_deadline_while_the_blocking_pool_is_saturated() {
    let runtime = a_runtime_with_one_blocking_thread();
    runtime.block_on(async {
        // Given a live terminal, and a blocking pool held by someone else's job
        let manager = CliSessionManager::new();
        let worktree = tempfile::tempdir().unwrap();
        manager
            .start_terminal_under_id(
                "saturated-pool",
                "01a1f5aa-0000-7000-8000-0000000000aa",
                worktree.path().to_path_buf(),
                RecordedTerminal::Agent {
                    kind: "muse-cli".to_string(),
                    argv: vec!["/bin/cat".to_string()],
                },
            )
            .await
            .expect("the terminal starts");
        let release = saturate_the_blocking_pool().await;

        // When
        let started = Instant::now();
        let finished = tokio::time::timeout(KILL_ALL_DEADLINE, manager.kill_all()).await;

        // Then
        let _ = release.send(());
        assert!(
            finished.is_ok(),
            "kill_all was still waiting on the blocking pool after {:?}",
            started.elapsed()
        );
    });
}
