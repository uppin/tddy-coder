//! Acceptance tests for the lazy, semaphore-bounded per-worktree disk-size calculator.
//!
//! Feature: docs/ft/web/worktree-disk-usage-streaming.md
//! Changeset: docs/dev/1-WIP/worktree-disk-usage-streaming.md
//!
//! These exercise the daemon library `WorktreeSizeCalculator` directly (no ConnectionService RPC),
//! mirroring `worktrees_acceptance.rs`. The directory-size walk is injected so a test can observe
//! concurrency and gate completion deterministically — production uses the real
//! `directory_size_bytes_best_effort`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use tddy_daemon::worktrees::{WorktreeSizeCalculator, WorktreeSizeStatus, WorktreeSizeUpdate};

const PROJECT: &str = "proj-disk-usage";

fn wt(name: &str) -> PathBuf {
    PathBuf::from(format!("/repos/demo/.worktrees/{name}"))
}

/// A `u64`-returning sizer that records each call's path and byte count, so a test can assert how
/// many times a given worktree was walked and vary its reported size between recomputations.
#[derive(Clone)]
struct InstrumentedSizer {
    sizes: Arc<Mutex<HashMap<PathBuf, u64>>>,
    calls: Arc<Mutex<HashMap<PathBuf, usize>>>,
}

impl InstrumentedSizer {
    fn new() -> Self {
        Self {
            sizes: Arc::new(Mutex::new(HashMap::new())),
            calls: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn set_size(&self, path: &Path, bytes: u64) {
        self.sizes.lock().unwrap().insert(path.to_path_buf(), bytes);
    }

    fn calls_for(&self, path: &Path) -> usize {
        *self.calls.lock().unwrap().get(path).unwrap_or(&0)
    }

    fn into_sizer(self) -> Arc<dyn Fn(&Path) -> u64 + Send + Sync> {
        Arc::new(move |path: &Path| {
            *self
                .calls
                .lock()
                .unwrap()
                .entry(path.to_path_buf())
                .or_insert(0) += 1;
            *self.sizes.lock().unwrap().get(path).unwrap_or(&0)
        })
    }
}

/// A counting gate the test opens N walks at a time through, so it can pin how many size walks are
/// allowed to run concurrently. Each walk waits for one token before completing.
#[derive(Clone)]
struct Gate {
    inner: Arc<(Mutex<usize>, Condvar)>,
}

impl Gate {
    fn new() -> Self {
        Self {
            inner: Arc::new((Mutex::new(0), Condvar::new())),
        }
    }

    /// Called from inside a walk (blocking thread): block until a token is available, take one.
    fn wait_one(&self) {
        let (lock, cvar) = &*self.inner;
        let mut n = lock.lock().unwrap();
        while *n == 0 {
            n = cvar.wait(n).unwrap();
        }
        *n -= 1;
    }

    /// Called from the test: let `count` waiting walks proceed.
    fn release(&self, count: usize) {
        let (lock, cvar) = &*self.inner;
        *lock.lock().unwrap() += count;
        cvar.notify_all();
    }
}

async fn recv_update(sub: &mut broadcast::Receiver<WorktreeSizeUpdate>) -> WorktreeSizeUpdate {
    tokio::time::timeout(Duration::from_secs(2), sub.recv())
        .await
        .expect("a worktree-size update should be published within the timeout")
        .expect("the update channel closed unexpectedly")
}

async fn recv_started(rx: &mut UnboundedReceiver<PathBuf>) -> PathBuf {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("a size walk should have started within the timeout")
        .expect("the started channel closed unexpectedly")
}

async fn no_further_walk_starts(rx: &mut UnboundedReceiver<PathBuf>) {
    let result = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        result.is_err(),
        "no further size walk may start while the semaphore is saturated, but one did: {:?}",
        result.ok().flatten()
    );
}

async fn await_all_cached(calc: &WorktreeSizeCalculator, paths: &[PathBuf]) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let all = paths
                .iter()
                .all(|p| calc.state(PROJECT, p).status == WorktreeSizeStatus::Cached);
            if all {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("every enqueued worktree should reach the Cached status");
}

async fn await_cached_size(calc: &WorktreeSizeCalculator, path: &Path, bytes: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = calc.state(PROJECT, path);
            if state.status == WorktreeSizeStatus::Cached && state.disk_bytes == Some(bytes) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("worktree {path:?} should reach Cached with {bytes} bytes"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_never_calculated_worktree_reports_none_status() {
    // Given a calculator with nothing computed yet
    let tmp = tempfile::tempdir().unwrap();
    let calc = WorktreeSizeCalculator::with_sizer(
        tmp.path().to_path_buf(),
        2,
        InstrumentedSizer::new().into_sizer(),
    );

    // When the status of an untouched worktree is read
    let state = calc.state(PROJECT, &wt("feat-a"));

    // Then it is None with no size and no calculation time
    assert_eq!(state.status, WorktreeSizeStatus::None);
    assert_eq!(state.disk_bytes, None);
    assert_eq!(state.calculated_at_unix_ms, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn calculating_a_worktree_transitions_none_to_calculating_to_cached() {
    // Given a subscriber watching the project and an untouched worktree
    let tmp = tempfile::tempdir().unwrap();
    let sizer = InstrumentedSizer::new();
    let path = wt("feat-a");
    sizer.set_size(&path, 4096);
    let calc = WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, sizer.into_sizer());
    let mut updates = calc.subscribe(PROJECT);
    assert_eq!(calc.state(PROJECT, &path).status, WorktreeSizeStatus::None);

    // When the worktree's size is enqueued
    calc.enqueue(PROJECT, &path).await;

    // Then the status is published as Calculating, then Cached with the walked size and a timestamp
    let calculating = recv_update(&mut updates).await;
    assert_eq!(calculating.path, path);
    assert_eq!(calculating.status, WorktreeSizeStatus::Calculating);
    assert_eq!(calculating.disk_bytes, None);

    let cached = recv_update(&mut updates).await;
    assert_eq!(cached.path, path);
    assert_eq!(cached.status, WorktreeSizeStatus::Cached);
    assert_eq!(cached.disk_bytes, Some(4096));
    assert!(
        cached.calculated_at_unix_ms.is_some(),
        "the Cached update must record when the size was calculated"
    );

    let final_state = calc.state(PROJECT, &path);
    assert_eq!(final_state.status, WorktreeSizeStatus::Cached);
    assert_eq!(final_state.disk_bytes, Some(4096));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_semaphore_limits_concurrent_size_calculations_to_two() {
    // Given a calculator with a two-permit semaphore and a gated sizer that reports each walk it
    // starts and blocks until the test releases it
    let tmp = tempfile::tempdir().unwrap();
    let (started_tx, mut started_rx) = unbounded_channel::<PathBuf>();
    let gate = Gate::new();
    let sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync> = {
        let gate = gate.clone();
        Arc::new(move |p: &Path| {
            started_tx.send(p.to_path_buf()).ok();
            gate.wait_one();
            4096
        })
    };
    let calc = WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, sizer);

    let paths = [wt("feat-a"), wt("feat-b"), wt("feat-c"), wt("feat-d")];

    // When four worktrees are enqueued at once
    for p in &paths {
        calc.enqueue(PROJECT, p).await;
    }

    // Then exactly two walks start and no third begins while the two permits are held
    let first = recv_started(&mut started_rx).await;
    let second = recv_started(&mut started_rx).await;
    no_further_walk_starts(&mut started_rx).await;

    // And releasing the two in flight lets exactly the remaining two start
    gate.release(2);
    let third = recv_started(&mut started_rx).await;
    let fourth = recv_started(&mut started_rx).await;
    no_further_walk_starts(&mut started_rx).await;

    // And once all are released every worktree reaches Cached
    gate.release(2);
    await_all_cached(&calc, &paths).await;

    let mut started: Vec<PathBuf> = vec![first, second, third, fourth];
    started.sort();
    let mut expected: Vec<PathBuf> = paths.to_vec();
    expected.sort();
    assert_eq!(
        started, expected,
        "every worktree must be walked exactly once"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cached_size_is_served_after_reload_without_recomputing() {
    // Given a worktree whose size has been calculated and persisted
    let tmp = tempfile::tempdir().unwrap();
    let sizer = InstrumentedSizer::new();
    let path = wt("feat-a");
    sizer.set_size(&path, 4096);
    let calc =
        WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, sizer.clone().into_sizer());
    calc.enqueue(PROJECT, &path).await;
    await_cached_size(&calc, &path, 4096).await;
    assert_eq!(sizer.calls_for(&path), 1);

    // When a fresh calculator is opened over the same persistence root
    let reloaded =
        WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, sizer.clone().into_sizer());

    // Then the worktree reads back as Cached with its size, and the sizer is not invoked again
    let state = reloaded.state(PROJECT, &path);
    assert_eq!(state.status, WorktreeSizeStatus::Cached);
    assert_eq!(state.disk_bytes, Some(4096));
    assert_eq!(
        sizer.calls_for(&path),
        1,
        "a reload must not re-walk a cached worktree"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recalculating_one_worktree_leaves_the_others_untouched() {
    // Given two worktrees both cached at 4096 bytes
    let tmp = tempfile::tempdir().unwrap();
    let sizer = InstrumentedSizer::new();
    let a = wt("feat-a");
    let b = wt("feat-b");
    sizer.set_size(&a, 4096);
    sizer.set_size(&b, 4096);
    let calc =
        WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, sizer.clone().into_sizer());
    calc.enqueue(PROJECT, &a).await;
    calc.enqueue(PROJECT, &b).await;
    await_all_cached(&calc, &[a.clone(), b.clone()]).await;
    assert_eq!(sizer.calls_for(&a), 1);
    assert_eq!(sizer.calls_for(&b), 1);

    // When only the first worktree is recalculated (its on-disk size has grown)
    sizer.set_size(&a, 9000);
    calc.enqueue(PROJECT, &a).await;
    await_cached_size(&calc, &a, 9000).await;

    // Then only that worktree was re-walked; the other keeps its cached value untouched
    assert_eq!(sizer.calls_for(&a), 2);
    assert_eq!(
        sizer.calls_for(&b),
        1,
        "recalculating one worktree must not re-walk the others"
    );
    assert_eq!(calc.state(PROJECT, &b).disk_bytes, Some(4096));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn enqueuing_an_already_calculating_worktree_does_not_start_a_second_walk() {
    // Given a worktree whose first walk is held mid-flight by a gate
    let tmp = tempfile::tempdir().unwrap();
    let (started_tx, mut started_rx) = unbounded_channel::<PathBuf>();
    let gate = Gate::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync> = {
        let gate = gate.clone();
        let calls = Arc::clone(&calls);
        Arc::new(move |p: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            started_tx.send(p.to_path_buf()).ok();
            gate.wait_one();
            4096
        })
    };
    let calc = WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, sizer);
    let path = wt("feat-a");

    // When the same worktree is enqueued a second time while its first walk is still Calculating
    calc.enqueue(PROJECT, &path).await;
    recv_started(&mut started_rx).await;
    assert_eq!(
        calc.state(PROJECT, &path).status,
        WorktreeSizeStatus::Calculating
    );
    calc.enqueue(PROJECT, &path).await;

    // Then no second walk begins (the in-flight calculation is reused)
    no_further_walk_starts(&mut started_rx).await;

    // And after the first walk completes, the sizer was invoked exactly once
    gate.release(1);
    await_cached_size(&calc, &path, 4096).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_project_snapshot_reports_every_known_worktree_state() {
    // Given two worktrees calculated to different sizes
    let tmp = tempfile::tempdir().unwrap();
    let sizer = InstrumentedSizer::new();
    let a = wt("feat-a");
    let b = wt("feat-b");
    sizer.set_size(&a, 4096);
    sizer.set_size(&b, 8192);
    let calc = WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, sizer.into_sizer());
    calc.enqueue(PROJECT, &a).await;
    calc.enqueue(PROJECT, &b).await;
    await_all_cached(&calc, &[a.clone(), b.clone()]).await;

    // When the project snapshot is read (the stream's first frame)
    let snapshot = calc.snapshot(PROJECT);

    // Then it reports both worktrees, each Cached with its own size
    let mut got: Vec<(PathBuf, WorktreeSizeStatus, Option<u64>)> = snapshot
        .into_iter()
        .map(|u| (u.path, u.status, u.disk_bytes))
        .collect();
    got.sort_by(|x, y| x.0.cmp(&y.0));
    let mut want = vec![
        (a, WorktreeSizeStatus::Cached, Some(4096u64)),
        (b, WorktreeSizeStatus::Cached, Some(8192u64)),
    ];
    want.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(got, want);
}
