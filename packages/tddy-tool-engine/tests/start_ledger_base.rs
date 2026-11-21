//! A start records the base its worktree was cut from — the commit and how far it was behind its
//! remote tip — from the start's `worktree_ready` event, so `GetSessionStart` reports it
//! (`#orchestra` 4/9).

use std::time::Duration;

use futures_util::stream;
use pretty_assertions::assert_eq;
use tddy_rpc::Status;
use tddy_service::proto::session::{
    start_session_event::Event, StartSessionEvent, StartTaskStarted, StartWorktreeReady,
};
use tddy_tool_engine::start_ledger::{begin_recorded_start, StartLedger};
use tokio::sync::mpsc;

#[tokio::test]
async fn a_started_session_records_the_base_commit_it_was_cut_from() {
    // Given a start that cut its worktree from a base two commits behind its remote tip
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());
    let (tx, rx) = mpsc::unbounded_channel::<Result<StartSessionEvent, Status>>();
    let events = Box::pin(stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|event| (event, rx))
    }));
    let send = |event: Event| {
        tx.send(Ok(StartSessionEvent { event: Some(event) }))
            .unwrap()
    };
    send(Event::Started(StartTaskStarted {
        task_id: "task-1".to_string(),
        daemon_instance_id: "host-a".to_string(),
    }));
    let handle = begin_recorded_start(ledger.clone(), events).await.unwrap();

    // When its worktree-ready event arrives
    send(Event::WorktreeReady(StartWorktreeReady {
        branch: "feature/outbox".to_string(),
        worktree: "/w/outbox".to_string(),
        base_commit: "4ba506396".to_string(),
        base_behind: 2,
    }));

    // Then the record carries the base and how stale it was
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let recorded = loop {
        let record = ledger.read(&handle.start_id).expect("a record");
        if record.worktree.is_some() || tokio::time::Instant::now() >= deadline {
            break record;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        (recorded.base_commit.as_deref(), recorded.base_behind),
        (Some("4ba506396"), Some(2))
    );
}
