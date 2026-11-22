//! `GetSessionStart` keeps the branch a start reported when its session's host lists none.
//!
//! On 2026-11-22 a claude-cli start on the Coder target `remote-dev` reported
//! `branch: "test/orchestrator-coder-hooks-probe"` while running (its `worktree_ready` event) and
//! `branch: ""` once `succeeded`: the host listed the target session with an empty branch — a
//! target start records none in its changeset — and the empty listing replaced the reported one.
//!
//! Findings: docs/dev/changesets/2026-11-22-coder-target-back-channel.md, problem 3.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream;
use pretty_assertions::assert_eq;
use tddy_service::proto::session::{
    start_session_event::Event, StartSessionEvent, StartSessionResponse, StartTaskStarted,
    StartWorktreeReady,
};
use tddy_tool_engine::start_ledger::{
    begin_recorded_start_with, StartLedger, StartLedgerRecord, StartState, StartedSessionDetails,
    StartedSessionListing,
};

const BRANCH: &str = "test/orchestrator-coder-hooks-probe";
const REMOTE_WORKTREE: &str =
    "/home/coder/.tddy/repos/tddy-coder/.worktrees/01a201cd-1cd1-7c53-8481-77991a117fac";
const SESSION: &str = "01a201cd-1cd1-7c53-8481-77991a117fac";

/// A host that lists a target session as it does today: its worktree, but no branch.
struct ListsATargetSessionWithoutItsBranch;

#[async_trait]
impl StartedSessionDetails for ListsATargetSessionWithoutItsBranch {
    async fn listing(&self, _: &str, _: &str) -> Option<StartedSessionListing> {
        Some(StartedSessionListing {
            branch: String::new(),
            worktree: REMOTE_WORKTREE.to_string(),
            display_name: "Hook probe on Coder remote-dev".to_string(),
        })
    }
}

/// A start that cut its worktree on `BRANCH` and then succeeded.
fn a_target_start_that_cut_its_branch(
) -> impl futures_util::Stream<Item = Result<StartSessionEvent, tddy_rpc::Status>> + Send + Unpin + 'static
{
    stream::iter(vec![
        Ok(StartSessionEvent {
            event: Some(Event::Started(StartTaskStarted {
                task_id: "task-1".to_string(),
                daemon_instance_id: "host-a".to_string(),
            })),
        }),
        Ok(StartSessionEvent {
            event: Some(Event::WorktreeReady(StartWorktreeReady {
                branch: BRANCH.to_string(),
                worktree: REMOTE_WORKTREE.to_string(),
                base_commit: "0a76c98fb".to_string(),
                base_behind: 0,
            })),
        }),
        Ok(StartSessionEvent {
            event: Some(Event::Result(StartSessionResponse {
                session_id: SESSION.to_string(),
                ..Default::default()
            })),
        }),
    ])
}

async fn finished(ledger: &StartLedger, start_id: &str) -> StartLedgerRecord {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(record) = ledger.read(start_id) {
            if record.state != StartState::Running {
                return record;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "start {start_id} never finished"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_succeeded_target_start_keeps_the_branch_it_cut_when_the_host_lists_none() {
    // Given a start on a target that reported its branch, whose session the host lists without one
    let dir = tempfile::tempdir().unwrap();
    let ledger = StartLedger::in_session_dir(dir.path());

    // When it succeeds
    let handle = begin_recorded_start_with(
        ledger.clone(),
        a_target_start_that_cut_its_branch(),
        Some(Arc::new(ListsATargetSessionWithoutItsBranch)),
    )
    .await
    .unwrap();

    // Then its record still names the branch it cut
    let record = finished(&ledger, &handle.start_id).await;
    assert_eq!(
        (record.state, record.branch.as_deref()),
        (StartState::Succeeded, Some(BRANCH))
    );
}
