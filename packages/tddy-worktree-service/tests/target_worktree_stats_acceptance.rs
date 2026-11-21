//! Acceptance: the Worktree tab of a session whose agent runs on a **target** shows that
//! session's worktree there — branch, diff summary and size measured on the target — without
//! holding up the local rows (`#target-parity` 7/7).
//!
//! On the Coder probe of 2026-11-21 the tab of the target session `01a1fcba-…` found no row for
//! its `/home/coder/…` worktree and showed it as missing.
//!
//! The target setup is [`target_session_support`]'s.
//!
//! PRD: docs/ft/web/session-worktree-inspector.md.

mod target_session_support;

use std::path::Path;

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use tddy_rpc::Request;
use tddy_service::proto::worktree::*;
use tddy_testing_commons::remote_target::StandInSshTarget;
use tddy_worktree_service::test_util::TEST_TOKEN;

use target_session_support::{
    a_daemon_with_a_target_session, reachable, ADaemon, Reach, BRANCH, REMOTE_WORKTREE,
};

/// A tracked file the agent changed in its worktree.
const CHANGED: &str = "README.md";

/// The target answers each command only after `delay`, as a slow transport does.
fn slowly(delay: std::time::Duration) -> impl FnOnce(&StandInSshTarget) -> Reach {
    move |target| {
        let slow = target.root.join("bin/slow-ssh");
        tddy_testing_commons::remote_target::write_executable(
            &slow,
            &format!(
                "#!/bin/sh\n/bin/sleep {}\nexec '{}' \"$@\"\n",
                delay.as_secs_f64(),
                target.root.join("bin/ssh").display()
            ),
        );
        Reach::Through(slow)
    }
}

impl ADaemon {
    /// The agent changed a tracked file in its worktree on the target: two lines added.
    fn agent_changed_a_file(&self) {
        std::fs::write(
            self.remote_worktree().join(CHANGED),
            "# probe\nfirst line added\nsecond line added\n",
        )
        .unwrap();
    }

    /// The Worktree tab's stream of the project's worktrees.
    async fn streams(&self) -> tddy_worktree_service::MpscWorktreeStatsStream {
        self.service
            .stream_worktree_stats(Request::direct(StreamWorktreeStatsRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: self.project_id.clone(),
                recalculate_all: false,
            }))
            .await
            .expect("the stream opens")
            .into_inner()
    }
}

/// Every row the stream carries for `path`, in order — the snapshot's and each update's — until
/// one satisfies `done` or [`OBSERVE`] passes. The last row seen, if any.
async fn the_row_for(
    stream: &mut tddy_worktree_service::MpscWorktreeStatsStream,
    path: &str,
    done: impl Fn(&WorktreeRow) -> bool,
) -> Option<WorktreeRow> {
    let mut last = None;
    let _ = tokio::time::timeout(OBSERVE, async {
        while let Some(Ok(event)) = stream.next().await {
            for row in event.snapshot.into_iter().chain(event.updated) {
                if row.path == path {
                    let finished = done(&row);
                    last = Some(row);
                    if finished {
                        return;
                    }
                }
            }
        }
    })
    .await;
    last
}

/// A safety net for the stand-in target to answer, not a prediction of how long it takes.
const OBSERVE: std::time::Duration = std::time::Duration::from_secs(20);

fn sized(row: &WorktreeRow) -> bool {
    row.size_status == WorktreeSizeStatus::Cached as i32
}

#[tokio::test]
async fn the_stream_holds_a_target_sessions_worktree_with_its_branch_diff_and_size() {
    // Given a target session whose agent changed a file in its worktree there
    let daemon = a_daemon_with_a_target_session(reachable);
    daemon.agent_changed_a_file();

    // When the Worktree tab streams the project's worktrees
    let mut stream = daemon.streams().await;
    let row = the_row_for(&mut stream, REMOTE_WORKTREE, sized).await;

    // Then the session's worktree is there, measured on the target
    assert_eq!(
        row.map(|r| (
            r.branch_label,
            r.changed_files,
            r.lines_added,
            r.stale,
            r.disk_bytes > 0
        )),
        Some((BRANCH.to_string(), 1, 2, false, true))
    );
}

#[tokio::test]
async fn the_local_snapshot_is_sent_before_the_target_answers() {
    // Given a target session on a target that takes seconds to answer each command
    let daemon = a_daemon_with_a_target_session(slowly(std::time::Duration::from_secs(5)));

    // When the stream opens
    let started = std::time::Instant::now();
    let mut stream = daemon.streams().await;
    let first = stream.next().await.and_then(Result::ok);

    // Then its first event — the snapshot holding the local clone — comes at once
    assert_eq!(
        (
            first.map(|event| event
                .snapshot
                .iter()
                .any(|r| Path::new(&r.path) == daemon.local_clone)),
            started.elapsed() < std::time::Duration::from_secs(3),
        ),
        (Some(true), true)
    );
}

#[tokio::test]
async fn an_unreachable_targets_row_is_stale_without_numbers() {
    // Given a target session whose target cannot be reached now
    let daemon = a_daemon_with_a_target_session(|_| {
        Reach::Refused("the Coder vault of ada is locked".to_string())
    });

    // When
    let mut stream = daemon.streams().await;
    let row = the_row_for(&mut stream, REMOTE_WORKTREE, |r| r.stale).await;

    // Then its row is there — not missing — marked stale, with no numbers
    assert_eq!(
        row.map(|r| (r.stale, r.disk_bytes, r.changed_files)),
        Some((true, 0, 0))
    );
}

#[tokio::test]
async fn calculating_a_target_worktrees_size_measures_it_on_the_target() {
    // Given an open stream whose own measurement of the target worktree has arrived, after which
    // the agent changed a file there
    let daemon = a_daemon_with_a_target_session(reachable);
    let mut stream = daemon.streams().await;
    let measured_by_the_stream = the_row_for(&mut stream, REMOTE_WORKTREE, sized).await;
    daemon.agent_changed_a_file();

    // When the tab asks for the target worktree's size
    let asked = daemon
        .service
        .calculate_worktree_size(Request::direct(CalculateWorktreeSizeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: daemon.project_id.clone(),
            worktree_path: REMOTE_WORKTREE.to_string(),
        }))
        .await
        .map(|r| r.into_inner().ok)
        .map_err(|s| s.message().to_string());
    let row = the_row_for(&mut stream, REMOTE_WORKTREE, |r| {
        sized(r) && r.changed_files > 0
    })
    .await;

    // Then it is accepted, and the stream carries a fresh measurement made there, holding the
    // change the stream's own measurement predates
    assert_eq!(
        (
            measured_by_the_stream.map(|r| r.changed_files),
            asked,
            row.map(|r| (r.changed_files, r.lines_added, r.disk_bytes > 0)),
        ),
        (Some(0), Ok(true), Some((1, 2, true)))
    );
}
