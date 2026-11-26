//! [`BlockingLane`]: `QueryBranch` is polled hundreds of times a minute, so identical in-flight legs
//! share one blocking job, and a full lane refuses at once instead of queueing work the pool cannot
//! drain (the 2026-11-26 incident: ~7,900 timeouts and a starved `ListSessions`).

use super::blocking_lane::BlockingLane;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use tddy_rpc::{Code, Status};
use tokio::sync::Semaphore;

const RPC_TIMEOUT: Duration = Duration::from_secs(300);

fn a_lane_with_permits(permits: usize) -> Arc<BlockingLane<String>> {
    Arc::new(BlockingLane::new(Arc::new(Semaphore::new(permits))))
}

/// A job that counts its runs and returns `answer` after a short git-like pause.
fn a_counted_job(
    runs: &Arc<AtomicUsize>,
    answer: &str,
) -> impl FnOnce() -> anyhow::Result<String> + Send + 'static {
    let runs = Arc::clone(runs);
    let answer = answer.to_string();
    move || {
        runs.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        Ok(answer)
    }
}

/// Start a job on `lane` under `key` that holds its permit until the returned sender is used.
async fn a_parked_job(lane: &Arc<BlockingLane<String>>, key: &str) -> mpsc::Sender<()> {
    let (release, parked) = mpsc::channel::<()>();
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let lane = Arc::clone(lane);
    let key = key.to_string();
    tokio::spawn(async move {
        let _ = lane
            .run(key, RPC_TIMEOUT, "parked", move || {
                let _ = started_tx.send(());
                let _ = parked.recv();
                Ok("parked".to_string())
            })
            .await;
    });
    tokio::task::spawn_blocking(move || started_rx.recv())
        .await
        .unwrap()
        .expect("the parked job starts");
    release
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_for_one_key_share_a_single_blocking_job() {
    // Given
    let lane = a_lane_with_permits(4);
    let runs = Arc::new(AtomicUsize::new(0));

    // When five polls of the same branch leg arrive together
    let calls: Vec<_> = (0..5)
        .map(|_| {
            let lane = Arc::clone(&lane);
            let job = a_counted_job(&runs, "abc123");
            tokio::spawn(async move {
                lane.run(
                    "project/feature/remote".to_string(),
                    RPC_TIMEOUT,
                    "remote",
                    job,
                )
                .await
            })
        })
        .collect();
    let mut answers = Vec::new();
    for call in calls {
        answers.push(call.await.unwrap().expect("every waiter gets the answer"));
    }

    // Then
    assert_eq!(runs.load(Ordering::SeqCst), 1, "the git leg ran once");
    assert_eq!(answers, vec!["abc123".to_string(); 5]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_saturated_lane_refuses_a_new_key_at_once() {
    // Given a lane whose only permit is held by a job that has not finished
    let lane = a_lane_with_permits(1);
    let release = a_parked_job(&lane, "project/other/worktree").await;
    let runs = Arc::new(AtomicUsize::new(0));

    // When
    let started = Instant::now();
    let refused: Result<String, Status> = lane
        .run(
            "project/feature/remote".to_string(),
            RPC_TIMEOUT,
            "remote",
            a_counted_job(&runs, "abc123"),
        )
        .await;

    // Then
    let _ = release.send(());
    let status = refused.expect_err("a full lane refuses");
    assert_eq!(status.code(), Code::Unavailable);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "refused at once"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 0, "the refused job never ran");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_permit_comes_back_when_its_job_finishes() {
    // Given a lane with one permit that served one request
    let lane = a_lane_with_permits(1);
    let runs = Arc::new(AtomicUsize::new(0));
    lane.run(
        "project/feature/remote".to_string(),
        RPC_TIMEOUT,
        "remote",
        a_counted_job(&runs, "first"),
    )
    .await
    .expect("the first request is served");

    // When
    let second = lane
        .run(
            "project/feature/remote".to_string(),
            RPC_TIMEOUT,
            "remote",
            a_counted_job(&runs, "second"),
        )
        .await;

    // Then
    assert_eq!(second.expect("the lane has room again"), "second");
    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "a finished job is not reused"
    );
}
