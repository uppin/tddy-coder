//! Closing a session — `PreviewCloseSession` and `CloseSession` through the real
//! `DaemonSessionHost` (docs/ft/daemon/session-liveness.md § Closing a session): a close ends every
//! agent tab of the session and removes every worktree it owns, keeping its record and branch, and
//! asks first whenever removing a worktree would lose work.
//!
//! The session is a tmux-run claude-cli session started through `StartSession`, its `claude` a stub
//! in a private tmux (`tmux_agent_support`), its worktree cut from a real project repository whose
//! `main` is pushed to an `origin`. Serial: the tmux socket directory is process-wide environment.

mod support;
mod tmux_agent_support;

use pretty_assertions::assert_eq;
use tddy_rpc::Code;
use tddy_service::proto::session::{
    close_session_event, CloseSessionDirtyFile, CloseSessionEvent, CloseSessionOutcome,
    CloseSessionPreview, CloseSessionRequest, CloseSessionStarted, CloseSessionStepKind,
    CloseSessionStepState, CloseSessionWorktree, CloseSessionWorktreeOwner, DeleteSessionRequest,
    DeleteSessionResponse, PreviewCloseSessionRequest,
};
use tddy_service::proto::session_agents_svc::{
    CancelAgentConversationRequest, CancelAgentConversationResponse,
};
use tddy_testing_commons::wait::eventually;

use support::OBSERVE;
use tddy_subagent_worktree::{ConversationId, ConversationWorktree, ConversationWorktrees};
use tmux_agent_support::close_events::{failed_steps_of, CloseEvents, UnreadableUntilDropped};
use tmux_agent_support::close_session::{a_close_request, WORKTREE_REMOVED_MARKER};
use tmux_agent_support::scripted_peer::{ScriptedPeer, PEER};
use tmux_agent_support::{
    a_daemon_in_a_room_with, a_daemon_whose_agent, a_daemon_with_a_private_tmux,
    a_process_is_alive, AgentTemperament, KilledOnDrop, TmuxAgentDaemon,
};

fn a_dirty(path: &str, change: &str) -> CloseSessionDirtyFile {
    CloseSessionDirtyFile {
        path: path.to_string(),
        change: change.to_string(),
    }
}

/// The paths and changes of a preview's only worktree, sorted by path.
fn dirty_files_of_the_only_worktree(preview: &CloseSessionPreview) -> Vec<CloseSessionDirtyFile> {
    assert_eq!(preview.worktrees.len(), 1, "{preview:?}");
    let mut files = preview.worktrees[0].dirty_files.clone();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

// ---------------------------------------------------------------------------
// The preview
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn the_preview_of_a_clean_session_needs_no_confirmation() {
    // Given a running session that changed nothing in its worktree
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    assert_eq!(
        (
            preview.needs_confirmation,
            preview.worktrees.len(),
            preview.worktrees.first().map(|w| w.dirty_files.len()),
            preview.worktrees.first().map(|w| w.unpushed_commit_count),
        ),
        (false, 1, Some(0), Some(0))
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_names_the_sessions_own_worktree_and_branch() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let branch = daemon.branch_of(&session_id);

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then its one worktree is the session's own, on this host, on the session's branch
    let worktree = &preview.worktrees[0];
    assert_eq!(
        (
            worktree.owner(),
            worktree.owner_id.as_str(),
            worktree.host.as_str(),
            worktree.path.as_str(),
            worktree.branch.as_str(),
            worktree.unavailable_reason.as_str(),
        ),
        (
            CloseSessionWorktreeOwner::Session,
            "",
            "",
            daemon.recorded_worktree_of(&session_id).as_str(),
            branch.as_str(),
            "",
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_lists_the_main_tab() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    let tabs: Vec<&str> = preview.tabs.iter().map(|tab| tab.tab_id.as_str()).collect();
    assert_eq!(tabs, vec!["main"]);
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_lists_a_tmux_wrapped_tab_beside_main() {
    // Given a running session with a tmux-wrapped agent tab open
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let (tab, _tab_tmux) = daemon.opens_a_tmux_wrapped_agent_tab(&session_id).await;

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    let mut tabs: Vec<String> = preview.tabs.iter().map(|t| t.tab_id.clone()).collect();
    tabs.sort();
    let mut expected = vec!["main".to_string(), tab];
    expected.sort();
    assert_eq!(tabs, expected);
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_lists_uncommitted_files_by_their_change_and_needs_confirmation() {
    // Given a session whose agent left a file, and changed and deleted nothing else
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    assert_eq!(
        (
            preview.needs_confirmation,
            dirty_files_of_the_only_worktree(&preview)
        ),
        (true, vec![a_dirty("notes.md", "untracked")])
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_does_not_list_ignored_files() {
    // Given a session whose worktree ignores `*.log`, holding one ignored and one untracked file
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.worktree_ignores(&session_id, "*.log");
    daemon.agent_leaves_a_file(&session_id, "build.log");
    daemon.agent_leaves_a_file(&session_id, "notes.md");

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then the ignored file is not among them
    assert_eq!(
        dirty_files_of_the_only_worktree(&preview),
        vec![
            a_dirty(".gitignore", "untracked"),
            a_dirty("notes.md", "untracked"),
        ]
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_lists_a_commit_no_remote_holds_and_needs_confirmation() {
    // Given a session whose agent committed work it pushed nowhere
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let sha = daemon.agent_commits(&session_id, "feature.rs", "add the feature");

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    let worktree = &preview.worktrees[0];
    let listed: Vec<(&str, &str)> = worktree
        .unpushed_commits
        .iter()
        .map(|commit| (commit.sha.as_str(), commit.subject.as_str()))
        .collect();
    assert_eq!(
        (
            preview.needs_confirmation,
            listed,
            worktree.unpushed_commit_count,
            worktree.dirty_files.len(),
        ),
        (true, vec![(sha.as_str(), "add the feature")], 1, 0)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_fingerprint_changes_when_the_worktree_does() {
    // Given a dirty session's preview
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");
    let before = daemon.previews_closing(&session_id).await;

    // When the agent writes another file
    daemon.agent_leaves_a_file(&session_id, "more-notes.md");
    let after = daemon.previews_closing(&session_id).await;

    // Then
    assert_ne!(before.fingerprint, after.fingerprint);
    assert!(!before.fingerprint.is_empty());
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_fingerprint_is_stable_while_nothing_changes() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");

    // When
    let first = daemon.previews_closing(&session_id).await;
    let second = daemon.previews_closing(&session_id).await;

    // Then
    assert_eq!(first.fingerprint, second.fingerprint);
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_of_a_session_this_daemon_does_not_hold_is_refused() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let refused = daemon.preview_of_closing("no-such-session").await;

    // Then it is refused as DeleteSession refuses a session it does not hold: the wrong daemon
    assert_eq!(
        refused.err().map(|status| status.code()),
        Some(Code::FailedPrecondition)
    );
}

// ---------------------------------------------------------------------------
// Closing a clean session
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_close_starts_by_naming_its_task_and_ends_ok() {
    // Given a clean running session
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    assert!(!events.started().task_id.is_empty(), "{events:?}");
    assert_eq!(
        (events.outcome().ok, events.outcome().failed_steps.len()),
        (true, 0)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_ends_with_exactly_one_terminal_event() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    assert_eq!(events.terminal_event_count(), 1, "{events:?}");
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_streams_a_done_step_for_the_tab_the_worktree_and_the_record() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let worktree = daemon.recorded_worktree_of(&session_id);

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    let done = events.done_steps();
    let has = |kind: CloseSessionStepKind, subject: &str| {
        done.iter().any(|(k, s)| *k == kind && s == subject)
    };
    assert_eq!(
        (
            has(CloseSessionStepKind::Terminate, "main"),
            has(CloseSessionStepKind::Remove, &worktree),
            done.iter()
                .any(|(kind, _)| *kind == CloseSessionStepKind::Record),
        ),
        (true, true, true),
        "{done:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_ends_the_sessions_tmux_session_and_its_agent() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    let agent = daemon.agent_pid_of(&session_id);

    // When
    daemon.closes(&session_id).await.outcome();

    // Then
    eventually(
        "the agent and its tmux session to be gone",
        OBSERVE,
        || match (a_process_is_alive(agent), daemon.tmux.has_session(&name)) {
            (false, false) => Ok(()),
            (agent, tmux) => Err(format!("agent alive: {agent}, tmux session alive: {tmux}")),
        },
    )
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_removes_the_worktree_and_unregisters_it_from_the_project() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let worktree = daemon.worktree_of(&session_id);

    // When
    daemon.closes(&session_id).await.outcome();

    // Then
    assert_eq!(
        (worktree.exists(), daemon.project_lists_worktree(&worktree)),
        (false, false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_keeps_the_session_record() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let session_dir = daemon.session_dir(&session_id);

    // When
    daemon.closes(&session_id).await.outcome();

    // Then
    assert_eq!(
        (
            session_dir.is_dir(),
            session_dir.join(".session.yaml").is_file()
        ),
        (true, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_keeps_the_sessions_branch() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let branch = daemon.branch_of(&session_id);

    // When
    daemon.closes(&session_id).await.outcome();

    // Then
    assert!(
        daemon.project_has_branch(&branch),
        "branch {branch} survives the close"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_records_the_removal_in_the_session_directory() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // When
    daemon.closes(&session_id).await.outcome();

    // Then
    let marker = daemon
        .session_dir(&session_id)
        .join(WORKTREE_REMOVED_MARKER);
    let recorded = std::fs::read_to_string(&marker).expect("the marker is written");
    assert!(!recorded.trim().is_empty(), "the marker records when");
}

#[tokio::test]
#[serial_test::serial]
async fn a_closed_session_is_listed_inactive_and_holding_no_worktree() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // When
    daemon.closes(&session_id).await.outcome();

    // Then
    let listed = daemon.listed(&session_id).await;
    assert_eq!((listed.is_active, listed.holds_worktree), (false, false));
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_ends_a_tmux_wrapped_tab_of_the_session() {
    // Given a running session with a tmux-wrapped agent tab open
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let (tab, tab_tmux) = daemon.opens_a_tmux_wrapped_agent_tab(&session_id).await;

    // When
    let events = daemon.closes(&session_id).await;

    // Then the tab's tmux session is gone, and its end was a step of the close
    assert_eq!(
        (
            events.outcome().ok,
            daemon.tmux.has_session(&tab_tmux),
            events
                .done_steps()
                .contains(&(CloseSessionStepKind::Terminate, tab.clone())),
        ),
        (true, false, true),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn closing_a_closed_session_again_succeeds() {
    // Given a session already closed
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.closes(&session_id).await.outcome();

    // When it is closed again
    let again = daemon.closes(&session_id).await;

    // Then a worktree already gone counts as removed
    assert_eq!(again.outcome().ok, true, "{again:?}");
}

// ---------------------------------------------------------------------------
// `holds_worktree`
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_live_session_with_a_worktree_is_listed_holding_it() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // Then
    assert_eq!(daemon.listed(&session_id).await.holds_worktree, true);
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_whose_worktree_is_gone_from_disk_holds_none() {
    // Given a session listed holding its worktree
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let before = daemon.listed(&session_id).await.holds_worktree;

    // When its worktree is removed outside the daemon
    daemon.worktree_is_removed_outside_the_daemon(&session_id);

    // Then it no longer holds one
    let after = daemon.listed(&session_id).await.holds_worktree;
    assert_eq!((before, after), (true, false));
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_whose_directory_records_the_removal_holds_no_worktree() {
    // Given a session listed holding its worktree
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let before = daemon.listed(&session_id).await.holds_worktree;

    // When its directory carries the worktree-removed marker, the worktree itself still there
    std::fs::write(
        daemon
            .session_dir(&session_id)
            .join(WORKTREE_REMOVED_MARKER),
        "2026-11-24T10:00:00Z\n",
    )
    .unwrap();

    // Then it no longer holds one
    let after = daemon.listed(&session_id).await.holds_worktree;
    assert_eq!((before, after), (true, false));
}

// ---------------------------------------------------------------------------
// Confirming a loss
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn an_unconfirmed_close_of_a_dirty_session_asks_with_the_dirty_files() {
    // Given a session whose agent left a file
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");

    // When it is closed without confirming
    let events = daemon.closes(&session_id).await;

    // Then the close asks, listing the file, and terminated nothing
    assert_eq!(
        (
            dirty_files_of_the_only_worktree(events.needs_confirmation()),
            events.streamed_a_step_of(CloseSessionStepKind::Terminate),
            events.streamed_a_step_of(CloseSessionStepKind::Remove),
        ),
        (vec![a_dirty("notes.md", "untracked")], false, false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn an_unconfirmed_close_of_a_dirty_session_leaves_the_session_running_and_its_files_intact() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");
    let worktree = daemon.worktree_of(&session_id);

    // When
    daemon.closes(&session_id).await.needs_confirmation();

    // Then
    assert_eq!(
        (
            daemon.tmux.has_session(&name),
            worktree.join("notes.md").is_file()
        ),
        (true, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn an_unconfirmed_close_of_a_session_with_an_unpushed_commit_asks() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    daemon.agent_commits(&session_id, "feature.rs", "add the feature");

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    assert_eq!(
        (
            events.needs_confirmation().worktrees[0].unpushed_commit_count,
            daemon.tmux.has_session(&name),
        ),
        (1, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_confirming_the_previews_fingerprint_removes_the_dirty_worktree() {
    // Given a dirty session, and its preview
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");
    let worktree = daemon.worktree_of(&session_id);
    let preview = daemon.previews_closing(&session_id).await;

    // When the close confirms it
    let events = daemon
        .closes_confirming(&session_id, &preview.fingerprint)
        .await;

    // Then
    assert_eq!(
        (events.outcome().ok, worktree.exists()),
        (true, false),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_confirming_a_stale_fingerprint_asks_again_and_removes_nothing() {
    // Given a dirty session's preview, after which the agent wrote another file
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");
    let preview = daemon.previews_closing(&session_id).await;
    daemon.agent_leaves_a_file(&session_id, "more-notes.md");
    let worktree = daemon.worktree_of(&session_id);

    // When the close confirms the preview it saw
    let events = daemon
        .closes_confirming(&session_id, &preview.fingerprint)
        .await;

    // Then it asks again with both files, and nothing was ended or removed
    assert_eq!(
        (
            dirty_files_of_the_only_worktree(events.needs_confirmation()),
            daemon.tmux.has_session(&name),
            worktree.join("more-notes.md").is_file(),
        ),
        (
            vec![
                a_dirty("more-notes.md", "untracked"),
                a_dirty("notes.md", "untracked"),
            ],
            true,
            true
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_confirmed_close_carrying_no_fingerprint_asks_again() {
    // Given a dirty session
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    daemon.agent_leaves_a_file(&session_id, "notes.md");

    // When a close says it confirmed, but names no preview
    let events = daemon.closes_confirming(&session_id, "").await;

    // Then
    assert_eq!(
        (
            events.needs_confirmation().needs_confirmation,
            daemon.tmux.has_session(&name)
        ),
        (true, true)
    );
}

// ---------------------------------------------------------------------------
// A tab that survives
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_tab_that_survives_termination_skips_every_removal_and_fails_the_close() {
    // Given a clean session whose tmux session nothing can end
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    let _agent = KilledOnDrop(daemon.agent_pid_of(&session_id));
    let worktree = daemon.worktree_of(&session_id);
    daemon.tmux_refuses_to_kill(&name);

    // When
    let events = daemon.closes(&session_id).await;

    // Then the terminate failed, the one removal was skipped, and the worktree is intact
    let outcome = events.outcome();
    assert_eq!(
        (
            outcome.ok,
            failed_steps_of(outcome, CloseSessionStepKind::Terminate)
                .contains(&CloseSessionStepState::Failed),
            failed_steps_of(outcome, CloseSessionStepKind::Remove),
            worktree.is_dir(),
        ),
        (false, true, vec![CloseSessionStepState::Skipped], true),
        "{events:?}"
    );
}

// ---------------------------------------------------------------------------
// Concurrent closes
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_second_close_of_a_session_whose_close_is_running_is_refused_naming_the_task() {
    // Given a close under way, its agent holding it in the terminate's grace
    let daemon =
        a_daemon_whose_agent(AgentTemperament::IgnoresHangupAndTerminationAndNeverReadsItsPane)
            .await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let _agent = KilledOnDrop(daemon.agent_pid_of(&session_id));
    let mut first = daemon
        .starts_closing(&session_id)
        .await
        .expect("the first close starts");
    let started = futures_util::StreamExt::next(&mut first)
        .await
        .expect("the first close streams its start")
        .expect("the start is not an error");
    let first_task = CloseEvents(vec![started]).started().task_id.clone();

    // When
    let second = daemon.starts_closing(&session_id).await;

    // Then
    let refusal = second.err().expect("the second close is refused");
    assert_eq!(
        (refusal.code(), refusal.message().contains(&first_task)),
        (Code::AlreadyExists, true),
        "{}",
        refusal.message()
    );
    CloseEvents::read_to_end(first).await.outcome();
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_of_another_session_starts_while_one_is_running() {
    // Given a close of one session under way, its agent holding it in the terminate's grace
    let daemon =
        a_daemon_whose_agent(AgentTemperament::IgnoresHangupAndTerminationAndNeverReadsItsPane)
            .await;
    let (busy, _busy_tmux) = daemon.a_running_tmux_session().await;
    let (other, _other_tmux) = daemon.a_running_tmux_session().await;
    let _agents = (
        KilledOnDrop(daemon.agent_pid_of(&busy)),
        KilledOnDrop(daemon.agent_pid_of(&other)),
    );
    let mut first = daemon
        .starts_closing(&busy)
        .await
        .expect("the first close starts");
    futures_util::StreamExt::next(&mut first)
        .await
        .expect("the first close streams its start")
        .expect("the start is not an error");

    // When
    let second = daemon.starts_closing(&other).await;

    // Then it is not refused, and runs to its own end
    let second = second.expect("a close of another session is not refused");
    let (first, second) = tokio::join!(
        CloseEvents::read_to_end(first),
        CloseEvents::read_to_end(second)
    );
    assert_eq!((first.outcome().ok, second.outcome().ok), (true, true));
}

#[tokio::test]
#[serial_test::serial]
async fn two_sessions_closed_at_once_both_close() {
    // Given two clean running sessions
    let daemon = a_daemon_with_a_private_tmux().await;
    let (one, _one_tmux) = daemon.a_running_tmux_session().await;
    let (two, _two_tmux) = daemon.a_running_tmux_session().await;
    let (one_worktree, two_worktree) = (daemon.worktree_of(&one), daemon.worktree_of(&two));

    // When both are closed at once
    let (first, second) = tokio::join!(daemon.closes(&one), daemon.closes(&two));

    // Then
    assert_eq!(
        (
            first.outcome().ok,
            second.outcome().ok,
            one_worktree.exists(),
            two_worktree.exists(),
        ),
        (true, true, false, false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_of_a_session_this_daemon_does_not_hold_is_refused() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let refused = daemon
        .starts_closing_with(a_close_request("no-such-session", None))
        .await;

    // Then it is refused as DeleteSession refuses a session it does not hold: the wrong daemon
    assert_eq!(
        refused.err().map(|status| status.code()),
        Some(Code::FailedPrecondition)
    );
}

// ---------------------------------------------------------------------------
// A clone-placed conversation's worktree
// ---------------------------------------------------------------------------

/// The conversation a session's subagent works in, in a worktree and branch of its own.
const CONVERSATION: &str = "explore";

/// A clone-placed conversation of `session_id`, its worktree cut inside the session's worktree as
/// the subagent route cuts it.
async fn a_conversation_worktree_of(
    daemon: &TmuxAgentDaemon,
    session_id: &str,
) -> ConversationWorktree {
    ConversationWorktrees::new(daemon.recorded_worktree_of(session_id), session_id)
        .ensure(&ConversationId::parse(CONVERSATION).unwrap())
        .await
        .expect("the conversation's worktree is cut")
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_lists_a_conversation_worktree_with_its_dirty_files() {
    // Given a running session whose subagent left a file in its conversation's worktree
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let conversation = a_conversation_worktree_of(&daemon, &session_id).await;
    std::fs::write(conversation.root().join("findings.md"), "found\n").unwrap();

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then the conversation's worktree is listed as its own, with the file, beside the session's
    let listed: Vec<(
        CloseSessionWorktreeOwner,
        String,
        String,
        Vec<CloseSessionDirtyFile>,
    )> = preview
        .worktrees
        .iter()
        .map(|worktree| {
            (
                worktree.owner(),
                worktree.owner_id.clone(),
                worktree.path.clone(),
                worktree.dirty_files.clone(),
            )
        })
        .collect();
    assert_eq!(
        (listed, preview.needs_confirmation),
        (
            vec![
                (
                    CloseSessionWorktreeOwner::Conversation,
                    CONVERSATION.to_string(),
                    conversation.root().to_string_lossy().into_owned(),
                    vec![a_dirty("findings.md", "untracked")],
                ),
                (
                    CloseSessionWorktreeOwner::Session,
                    String::new(),
                    daemon.recorded_worktree_of(&session_id),
                    Vec::new(),
                ),
            ],
            true
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_confirmed_close_removes_a_conversation_worktree_with_its_branch_and_base_ref() {
    // Given a session with a dirty conversation worktree, and its preview
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let conversation = a_conversation_worktree_of(&daemon, &session_id).await;
    std::fs::write(conversation.root().join("findings.md"), "found\n").unwrap();
    let base_ref = format!("refs/tddy/subagent-base/{session_id}/{CONVERSATION}");
    let preview = daemon.previews_closing(&session_id).await;

    // When the close confirms it
    let events = daemon
        .closes_confirming(&session_id, &preview.fingerprint)
        .await;

    // Then the conversation's worktree, its branch and its base ref are gone
    assert_eq!(
        (
            events.outcome().ok,
            events.done_steps().contains(&(
                CloseSessionStepKind::Remove,
                conversation.root().to_string_lossy().into_owned()
            )),
            conversation.root().exists(),
            daemon.project_has_branch(conversation.branch()),
            daemon.project_has_ref(&base_ref),
        ),
        (true, true, false, false, false),
        "{events:?}"
    );
}

// ---------------------------------------------------------------------------
// A split session's codebase half
// ---------------------------------------------------------------------------

/// A daemon holding two running sessions, the first recording the second as its codebase half on
/// this daemon (a jailed codebase): the split session, and the workspace session holding its
/// checkout.
async fn a_split_session_whose_codebase_half_is_on_this_daemon() -> (TmuxAgentDaemon, String, String)
{
    let daemon = a_daemon_with_a_private_tmux().await;
    let (split, _split_tmux) = daemon.a_running_tmux_session().await;
    let (workspace, _workspace_tmux) = daemon.a_running_tmux_session().await;
    daemon.records_its_codebase_half_as(&split, &daemon.instance_id(), &workspace);
    (daemon, split, workspace)
}

/// The owner, owner id and path of every worktree `preview` lists, in order.
fn owners_of(preview: &CloseSessionPreview) -> Vec<(CloseSessionWorktreeOwner, String, String)> {
    preview
        .worktrees
        .iter()
        .map(|worktree| {
            (
                worktree.owner(),
                worktree.owner_id.clone(),
                worktree.path.clone(),
            )
        })
        .collect()
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_lists_the_codebase_halfs_checkout_after_the_sessions_own_worktree() {
    // Given
    let (daemon, split, workspace) = a_split_session_whose_codebase_half_is_on_this_daemon().await;

    // When
    let preview = daemon.previews_closing(&split).await;

    // Then
    assert_eq!(
        owners_of(&preview),
        vec![
            (
                CloseSessionWorktreeOwner::Session,
                String::new(),
                daemon.recorded_worktree_of(&split)
            ),
            (
                CloseSessionWorktreeOwner::Codebase,
                workspace.clone(),
                daemon.recorded_worktree_of(&workspace)
            ),
        ]
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_closes_the_codebase_half_and_removes_its_checkout() {
    // Given
    let (daemon, split, workspace) = a_split_session_whose_codebase_half_is_on_this_daemon().await;
    let checkout = daemon.worktree_of(&workspace);

    // When
    let events = daemon.closes(&split).await;

    // Then the checkout is gone, and the workspace session records its own removal
    assert_eq!(
        (
            events.outcome().ok,
            checkout.exists(),
            daemon
                .session_dir(&workspace)
                .join(WORKTREE_REMOVED_MARKER)
                .is_file(),
        ),
        (true, false, true),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_codebase_half_its_daemon_no_longer_holds_has_no_checkout_left_to_remove() {
    // Given a split session whose workspace session is gone from its daemon
    let daemon = a_daemon_with_a_private_tmux().await;
    let (split, _tmux) = daemon.a_running_tmux_session().await;
    daemon.records_its_codebase_half_as(&split, &daemon.instance_id(), "a-deleted-workspace");

    // When
    let preview = daemon.previews_closing(&split).await;
    let events = daemon.closes(&split).await;

    // Then only the session's own worktree is listed, and the close ends ok
    assert_eq!(
        (
            owners_of(&preview)
                .into_iter()
                .map(|(owner, _, _)| owner)
                .collect::<Vec<_>>(),
            events.outcome().ok,
        ),
        (vec![CloseSessionWorktreeOwner::Session], true),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_codebase_half_whose_record_cannot_be_read_is_unreadable_not_gone() {
    // Given a split session whose workspace session's record was left half-written
    let (daemon, split, workspace) = a_split_session_whose_codebase_half_is_on_this_daemon().await;
    daemon.record_is_left_half_written(&workspace);

    // When
    let preview = daemon.previews_closing(&split).await;

    // Then the checkout is listed as one that could not be read, which needs confirming
    let codebase = preview
        .worktrees
        .iter()
        .find(|worktree| worktree.owner() == CloseSessionWorktreeOwner::Codebase);
    assert_eq!(
        (
            codebase.map(|worktree| worktree.unavailable_reason.contains("cannot be read")),
            preview.needs_confirmation,
        ),
        (Some(true), true),
        "{preview:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_of_a_split_session_whose_codebase_record_cannot_be_read_ends_nothing() {
    // Given
    let (daemon, split, workspace) = a_split_session_whose_codebase_half_is_on_this_daemon().await;
    let split_tmux = daemon.tmux_session_of(&split);
    daemon.record_is_left_half_written(&workspace);

    // When
    let events = daemon.closes(&split).await;

    // Then it asks, and the split session's agent still runs
    assert_eq!(
        (
            events.needs_confirmation().needs_confirmation,
            daemon.tmux.has_session(&split_tmux),
        ),
        (true, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_codebase_half_on_a_daemon_this_one_cannot_reach_is_unreadable() {
    // Given a split session whose codebase daemon is a peer, and no common room to reach it through
    let daemon = a_daemon_with_a_private_tmux().await;
    let (split, _tmux) = daemon.a_running_tmux_session().await;
    daemon.records_its_codebase_half_as(&split, "a-peer-daemon", "its-workspace");

    // When
    let preview = daemon.previews_closing(&split).await;

    // Then
    let codebase = preview
        .worktrees
        .iter()
        .find(|worktree| worktree.owner() == CloseSessionWorktreeOwner::Codebase)
        .map(|worktree| {
            (
                worktree.host.clone(),
                worktree.unavailable_reason.is_empty(),
            )
        });
    assert_eq!(
        (codebase, preview.needs_confirmation),
        (Some(("a-peer-daemon".to_string(), false)), true),
        "{preview:?}"
    );
}

// ---------------------------------------------------------------------------
// While a close runs
// ---------------------------------------------------------------------------

/// A refusal's code, and whether it says the session is being closed.
fn refused_as_closing<T>(result: Result<T, tddy_rpc::Status>) -> Option<(Code, bool)> {
    result
        .err()
        .map(|status| (status.code(), status.message().contains("being closed")))
}

#[tokio::test]
#[serial_test::serial]
async fn while_a_session_is_being_closed_its_resume_a_new_tab_and_its_delete_are_refused() {
    // Given a close under way, its agent holding it in the terminate's grace
    let daemon =
        a_daemon_whose_agent(AgentTemperament::IgnoresHangupAndTerminationAndNeverReadsItsPane)
            .await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let _agent = KilledOnDrop(daemon.agent_pid_of(&session_id));
    let mut close = daemon
        .starts_closing(&session_id)
        .await
        .expect("the close starts");
    futures_util::StreamExt::next(&mut close)
        .await
        .expect("the close streams its start")
        .expect("the start is not an error");

    // When
    let resumed = daemon.resume_of(&session_id).await;
    let tab = daemon.start_of_a_tmux_wrapped_agent_tab(&session_id).await;
    let deleted = daemon.delete_of(&session_id).await;

    // Then each is refused naming the close, which still runs to its end
    let refused = Some((Code::AlreadyExists, true));
    assert_eq!(
        (
            refused_as_closing(resumed),
            refused_as_closing(tab),
            refused_as_closing(deleted),
            CloseEvents::read_to_end(close).await.outcome().ok,
        ),
        (refused, refused, refused, true)
    );
}

// ---------------------------------------------------------------------------
// What cannot be read stops the close
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_close_of_a_session_whose_tab_roster_cannot_be_read_fails_its_check_and_ends_nothing() {
    // Given a running session whose tabs.yaml was left half-written
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    let worktree = daemon.worktree_of(&session_id);
    daemon.tab_roster_is_left_half_written(&session_id);

    // When
    let events = daemon.closes(&session_id).await;

    // Then the check failed, the removal was skipped, and the agent and its worktree are intact
    let outcome = events.outcome();
    assert_eq!(
        (
            outcome.ok,
            failed_steps_of(outcome, CloseSessionStepKind::Check),
            failed_steps_of(outcome, CloseSessionStepKind::Remove),
            daemon.tmux.has_session(&name),
            worktree.is_dir(),
        ),
        (
            false,
            vec![CloseSessionStepState::Failed],
            vec![CloseSessionStepState::Skipped],
            true,
            true
        ),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_whose_tab_roster_cannot_be_read_is_refused_and_keeps_its_record() {
    // Given a running session whose tabs.yaml was left half-written
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.tab_roster_is_left_half_written(&session_id);

    // When
    let deleted = daemon.delete_of(&session_id).await;

    // Then
    assert_eq!(
        (
            deleted
                .err()
                .map(|status| status.message().contains("tab roster")),
            daemon.session_dir(&session_id).is_dir(),
        ),
        (Some(true), true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_conversation_worktrees_directory_that_cannot_be_read_is_listed_as_unreadable() {
    // Given a running session whose conversation worktrees' directory cannot be read
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let conversations = daemon
        .worktree_of(&session_id)
        .join(tddy_subagent_worktree::SUBAGENT_WORKTREES_DIR);
    std::fs::create_dir_all(&conversations).unwrap();
    let _locked = UnreadableUntilDropped::made_of(conversations);

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    let unreadable_conversation = preview.worktrees.iter().any(|worktree| {
        worktree.owner() == CloseSessionWorktreeOwner::Conversation
            && !worktree.unavailable_reason.is_empty()
    });
    assert_eq!(
        (unreadable_conversation, preview.needs_confirmation),
        (true, true),
        "{preview:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_session_worktree_that_cannot_be_looked_at_is_listed_as_unreadable_not_gone() {
    // Given a running session whose worktree's parent directory cannot be searched
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let worktrees = daemon
        .worktree_of(&session_id)
        .parent()
        .expect("the worktree sits in its project's .worktrees")
        .to_path_buf();
    let _locked = UnreadableUntilDropped::made_of(worktrees);

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    let own = preview
        .worktrees
        .iter()
        .find(|worktree| worktree.owner() == CloseSessionWorktreeOwner::Session)
        .map(|worktree| worktree.unavailable_reason.is_empty());
    assert_eq!(
        (own, preview.needs_confirmation),
        (Some(false), true),
        "{preview:?}"
    );
}

// ---------------------------------------------------------------------------
// A codebase half on a peer
// ---------------------------------------------------------------------------

/// The workspace session holding a split session's checkout on [`PEER`].
const PEER_WORKSPACE: &str = "peer-workspace";

/// The words a daemon refuses a session it does not hold with.
const NOT_HELD_HERE: &str =
    "session is not present on this daemon; use the daemon that owns it (check routing / host)";

/// What the peer previews closing [`PEER_WORKSPACE`] would remove: its clean checkout.
fn the_peers_preview() -> CloseSessionPreview {
    CloseSessionPreview {
        worktrees: vec![CloseSessionWorktree {
            path: "/peer/project/.worktrees/peer-workspace".to_string(),
            owner: CloseSessionWorktreeOwner::Session as i32,
            branch: "peer-branch".to_string(),
            ..CloseSessionWorktree::default()
        }],
        fingerprint: "the-peers-fingerprint".to_string(),
        ..CloseSessionPreview::default()
    }
}

/// A peer whose close of [`PEER_WORKSPACE`] starts and ends ok.
fn a_peer_closing_its_checkout_ok() -> std::sync::Arc<ScriptedPeer> {
    let peer = ScriptedPeer::in_the_room();
    peer.answers("PreviewCloseSession", Ok(the_peers_preview()));
    peer.streams(
        "CloseSession",
        vec![
            an_event(close_session_event::Event::Started(CloseSessionStarted {
                task_id: "peer-task".to_string(),
                daemon_instance_id: PEER.to_string(),
            })),
            an_event(close_session_event::Event::Outcome(CloseSessionOutcome {
                ok: true,
                failed_steps: Vec::new(),
            })),
        ],
    );
    peer
}

fn an_event(event: close_session_event::Event) -> CloseSessionEvent {
    CloseSessionEvent { event: Some(event) }
}

/// A daemon in a room with `peer`, and a running session recording [`PEER_WORKSPACE`] there as its
/// codebase half.
async fn a_split_session_whose_codebase_half_is_on(
    peer: std::sync::Arc<ScriptedPeer>,
) -> (TmuxAgentDaemon, String) {
    let daemon = a_daemon_in_a_room_with(peer).await;
    let (split, _tmux) = daemon.a_running_tmux_session().await;
    daemon.records_its_codebase_half_as(&split, PEER, PEER_WORKSPACE);
    (daemon, split)
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_lists_the_checkout_the_peer_previews_as_the_codebase_half_on_that_peer() {
    // Given
    let peer = a_peer_closing_its_checkout_ok();
    let (daemon, split) = a_split_session_whose_codebase_half_is_on(peer.clone()).await;

    // When
    let preview = daemon.previews_closing(&split).await;

    // Then
    let codebase: Vec<(String, String, String)> = preview
        .worktrees
        .iter()
        .filter(|worktree| worktree.owner() == CloseSessionWorktreeOwner::Codebase)
        .map(|worktree| {
            (
                worktree.host.clone(),
                worktree.owner_id.clone(),
                worktree.path.clone(),
            )
        })
        .collect();
    assert_eq!(
        (
            codebase,
            peer.was_asked::<PreviewCloseSessionRequest>("PreviewCloseSession")
                .into_iter()
                .map(|asked| asked.session_id)
                .collect::<Vec<_>>(),
        ),
        (
            vec![(
                PEER.to_string(),
                PEER_WORKSPACE.to_string(),
                "/peer/project/.worktrees/peer-workspace".to_string()
            )],
            vec![PEER_WORKSPACE.to_string()]
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_closes_the_codebase_half_on_its_peer_confirming_the_peers_preview() {
    // Given
    let peer = a_peer_closing_its_checkout_ok();
    let (daemon, split) = a_split_session_whose_codebase_half_is_on(peer.clone()).await;

    // When
    let events = daemon.closes(&split).await;

    // Then the peer was asked to close its workspace session, confirming what it previewed
    let asked: Vec<(String, bool, String)> = peer
        .was_asked::<CloseSessionRequest>("CloseSession")
        .into_iter()
        .map(|asked| {
            (
                asked.session_id,
                asked.confirmed,
                asked.confirmed_fingerprint,
            )
        })
        .collect();
    assert_eq!(
        (events.outcome().ok, asked),
        (
            true,
            vec![(
                PEER_WORKSPACE.to_string(),
                true,
                "the-peers-fingerprint".to_string()
            )]
        ),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_peer_that_no_longer_holds_the_codebase_half_has_no_checkout_left_to_close() {
    // Given a peer that holds no such workspace session
    let peer = ScriptedPeer::in_the_room();
    peer.answers::<CloseSessionPreview>(
        "PreviewCloseSession",
        Err(tddy_rpc::Status::failed_precondition(NOT_HELD_HERE)),
    );
    let (daemon, split) = a_split_session_whose_codebase_half_is_on(peer.clone()).await;

    // When
    let events = daemon.closes(&split).await;

    // Then the close ends ok without asking the peer to close anything
    assert_eq!(
        (
            events.outcome().ok,
            peer.was_asked::<CloseSessionRequest>("CloseSession").len()
        ),
        (true, 0),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_peer_refusing_the_codebase_preview_for_another_reason_leaves_an_unreadable_checkout() {
    // Given a peer whose record of the workspace session cannot be read
    let peer = ScriptedPeer::in_the_room();
    peer.answers::<CloseSessionPreview>(
        "PreviewCloseSession",
        Err(tddy_rpc::Status::failed_precondition(
            "the record of the session in /peer/sessions/peer-workspace cannot be read",
        )),
    );
    let (daemon, split) = a_split_session_whose_codebase_half_is_on(peer).await;

    // When
    let preview = daemon.previews_closing(&split).await;

    // Then
    let codebase = preview
        .worktrees
        .iter()
        .find(|worktree| worktree.owner() == CloseSessionWorktreeOwner::Codebase)
        .map(|worktree| worktree.unavailable_reason.contains("cannot be read"));
    assert_eq!(
        (codebase, preview.needs_confirmation),
        (Some(true), true),
        "{preview:?}"
    );
}

// ---------------------------------------------------------------------------
// Agent clones
// ---------------------------------------------------------------------------

/// The workspace session holding a roster agent's clone on [`PEER`].
const CLONE_WORKSPACE: &str = "clone-workspace";

/// A daemon in a room with a peer that deletes whatever it is asked to, and a running session whose
/// record names an agent reading a clone [`PEER`] holds as [`CLONE_WORKSPACE`].
async fn a_session_with_a_recorded_clone() -> (TmuxAgentDaemon, String, std::sync::Arc<ScriptedPeer>)
{
    let peer = ScriptedPeer::in_the_room();
    peer.answers("DeleteSession", Ok(DeleteSessionResponse { ok: true }));
    let daemon = a_daemon_in_a_room_with(peer.clone()).await;
    let (session_id, _tmux) = daemon.a_running_tmux_session().await;
    daemon.records_an_agent_reading_a_clone(&session_id, PEER, CLONE_WORKSPACE);
    (daemon, session_id, peer)
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_tears_down_a_clone_the_record_names_on_the_daemon_holding_it() {
    // Given
    let (daemon, session_id, peer) = a_session_with_a_recorded_clone().await;

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    assert_eq!(
        (
            events.outcome().ok,
            events
                .done_steps()
                .contains(&(CloseSessionStepKind::Remove, CLONE_WORKSPACE.to_string())),
            peer.was_asked::<DeleteSessionRequest>("DeleteSession")
                .into_iter()
                .map(|asked| asked.session_id)
                .collect::<Vec<_>>(),
        ),
        (true, true, vec![CLONE_WORKSPACE.to_string()]),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_forgets_the_clone_it_tore_down_from_the_sessions_record() {
    // Given
    let (daemon, session_id, _peer) = a_session_with_a_recorded_clone().await;

    // When
    daemon.closes(&session_id).await.outcome();

    // Then
    assert_eq!(
        daemon.clones_recorded_for(&session_id),
        Vec::<String>::new()
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_closed_session_whose_clone_was_torn_down_needs_no_common_room() {
    // Given a closed session whose clone went with the close, and a common room that is down
    let (daemon, session_id, peer) = a_session_with_a_recorded_clone().await;
    let closed = daemon.closes(&session_id).await;
    peer.leaves_the_room();

    // When
    let deleted = daemon.delete_of(&session_id).await;

    // Then
    assert_eq!(
        (
            closed.outcome().ok,
            deleted.map_err(|status| status.message().to_string()),
            daemon.session_dir(&session_id).exists(),
        ),
        (true, Ok(()), false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_whose_clone_cannot_be_reached_fails_that_removal_and_records_nothing() {
    // Given a session whose clone's daemon cannot be reached
    let (daemon, session_id, peer) = a_session_with_a_recorded_clone().await;
    peer.leaves_the_room();

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    let outcome = events.outcome();
    let clone_removal = outcome
        .failed_steps
        .iter()
        .find(|step| step.subject == CLONE_WORKSPACE)
        .map(|step| (step.kind(), step.state()));
    assert_eq!(
        (
            outcome.ok,
            clone_removal,
            failed_steps_of(outcome, CloseSessionStepKind::Record),
            daemon
                .session_dir(&session_id)
                .join(WORKTREE_REMOVED_MARKER)
                .exists(),
        ),
        (
            false,
            Some((CloseSessionStepKind::Remove, CloseSessionStepState::Failed)),
            vec![CloseSessionStepState::Skipped],
            false
        ),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_tears_down_a_clone_only_its_record_names() {
    // Given a session whose clone this daemon knows from the record alone (as after a restart)
    let (daemon, session_id, peer) = a_session_with_a_recorded_clone().await;

    // When
    let deleted = daemon.delete_of(&session_id).await;

    // Then
    assert_eq!(
        (
            deleted.map_err(|status| status.message().to_string()),
            peer.was_asked::<DeleteSessionRequest>("DeleteSession")
                .into_iter()
                .map(|asked| asked.session_id)
                .collect::<Vec<_>>(),
        ),
        (Ok(()), vec![CLONE_WORKSPACE.to_string()])
    );
}

// ---------------------------------------------------------------------------
// Conversations open on the session
// ---------------------------------------------------------------------------

/// A conversation open on a session, its turn loop on [`PEER`].
const REMOTE_CONVERSATION: &str = "a-remote-conversation";

/// A daemon in a room with `peer`, and a running session with [`REMOTE_CONVERSATION`] open on it.
async fn a_session_with_a_conversation_on(
    peer: std::sync::Arc<ScriptedPeer>,
) -> (TmuxAgentDaemon, String) {
    let daemon = a_daemon_in_a_room_with(peer).await;
    let (session_id, _tmux) = daemon.a_running_tmux_session().await;
    tddy_session_lifecycle::test_util::open_a_remote_conversation(
        &daemon.host,
        REMOTE_CONVERSATION,
        &session_id,
        &format!("explorer@{PEER}"),
        PEER,
    )
    .await;
    (daemon, session_id)
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_cancels_a_conversation_on_the_daemon_running_its_turn_loop() {
    // Given
    let peer = ScriptedPeer::in_the_room();
    peer.answers(
        "CancelAgentConversation",
        Ok(CancelAgentConversationResponse::default()),
    );
    let (daemon, session_id) = a_session_with_a_conversation_on(peer.clone()).await;

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    assert_eq!(
        (
            events.outcome().ok,
            events.done_steps().contains(&(
                CloseSessionStepKind::Terminate,
                format!("conversation:{REMOTE_CONVERSATION}")
            )),
            peer.was_asked::<CancelAgentConversationRequest>("CancelAgentConversation")
                .into_iter()
                .map(|asked| asked.conversation_id)
                .collect::<Vec<_>>(),
            tddy_session_lifecycle::test_util::conversation_is_open(
                &daemon.host,
                REMOTE_CONVERSATION
            )
            .await,
        ),
        (true, true, vec![REMOTE_CONVERSATION.to_string()], false),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_whose_conversation_cannot_be_cancelled_removes_no_worktree() {
    // Given a session whose conversation's daemon refuses the cancel
    let peer = ScriptedPeer::in_the_room();
    peer.answers::<CancelAgentConversationResponse>(
        "CancelAgentConversation",
        Err(tddy_rpc::Status::unavailable("the peer is restarting")),
    );
    let (daemon, session_id) = a_session_with_a_conversation_on(peer).await;
    let worktree = daemon.worktree_of(&session_id);

    // When
    let events = daemon.closes(&session_id).await;

    // Then
    let outcome = events.outcome();
    assert_eq!(
        (
            outcome.ok,
            failed_steps_of(outcome, CloseSessionStepKind::Terminate),
            failed_steps_of(outcome, CloseSessionStepKind::Remove),
            worktree.is_dir(),
        ),
        (
            false,
            vec![CloseSessionStepState::Failed],
            vec![CloseSessionStepState::Skipped],
            true
        ),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_whose_conversation_cannot_be_cancelled_is_refused() {
    // Given
    let peer = ScriptedPeer::in_the_room();
    peer.answers::<CancelAgentConversationResponse>(
        "CancelAgentConversation",
        Err(tddy_rpc::Status::unavailable("the peer is restarting")),
    );
    let (daemon, session_id) = a_session_with_a_conversation_on(peer).await;

    // When
    let deleted = daemon.delete_of(&session_id).await;

    // Then
    assert_eq!(
        (
            deleted.err().map(|status| status.code()),
            daemon.session_dir(&session_id).is_dir(),
        ),
        (Some(Code::Unavailable), true)
    );
}

// ---------------------------------------------------------------------------
// The re-check after every tab ended, and a cancelled close
// ---------------------------------------------------------------------------

/// What the agent of [`AgentTemperament::LeavesAFileWhenEnded`] writes as it is ended.
const WRITTEN_AS_IT_ENDED: &str = "late-notes.md";

#[tokio::test]
#[serial_test::serial]
async fn an_agent_that_writes_as_it_is_ended_makes_the_close_ask_and_removes_nothing() {
    // Given a clean session whose agent saves a file as it is ended
    let daemon =
        a_daemon_whose_agent(AgentTemperament::LeavesAFileWhenEnded(WRITTEN_AS_IT_ENDED)).await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let worktree = daemon.worktree_of(&session_id);

    // When it is closed without a dialog
    let events = daemon.closes(&session_id).await;

    // Then its tab was ended, the close asks about the new file, and no removal was attempted
    assert_eq!(
        (
            events
                .done_steps()
                .contains(&(CloseSessionStepKind::Terminate, "main".to_string())),
            dirty_files_of_the_only_worktree(events.needs_confirmation()),
            events.streamed_a_step_of(CloseSessionStepKind::Remove),
            worktree.join(WRITTEN_AS_IT_ENDED).is_file(),
        ),
        (
            true,
            vec![a_dirty(WRITTEN_AS_IT_ENDED, "untracked")],
            false,
            true
        ),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_close_cancelled_while_it_ends_the_tabs_skips_every_removal() {
    // Given a close under way, its agent holding it in the terminate's grace
    let daemon =
        a_daemon_whose_agent(AgentTemperament::IgnoresHangupAndTerminationAndNeverReadsItsPane)
            .await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let _agent = KilledOnDrop(daemon.agent_pid_of(&session_id));
    let worktree = daemon.worktree_of(&session_id);
    let mut close = daemon
        .starts_closing(&session_id)
        .await
        .expect("the close starts");
    let started = futures_util::StreamExt::next(&mut close)
        .await
        .expect("the close streams its start")
        .expect("the start is not an error");
    let task_id = CloseEvents(vec![started]).started().task_id.clone();

    // When its task is cancelled
    let cancelled = daemon
        .host
        .task_registry()
        .cancel_task(&tddy_task::TaskId(task_id))
        .await;

    // Then the removal and the record are skipped as cancelled, and the worktree is intact
    let events = CloseEvents::read_to_end(close).await;
    let outcome = events.outcome();
    let skipped_as_cancelled: Vec<CloseSessionStepKind> = outcome
        .failed_steps
        .iter()
        .filter(|step| step.message.contains("cancelled"))
        .map(|step| step.kind())
        .collect();
    assert_eq!(
        (
            cancelled,
            outcome.ok,
            skipped_as_cancelled,
            worktree.is_dir()
        ),
        (
            true,
            false,
            vec![CloseSessionStepKind::Remove, CloseSessionStepKind::Record],
            true
        ),
        "{events:?}"
    );
}
