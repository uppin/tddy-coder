//! Acceptance: the Worktree tab's Clean, Delete and Restore of a session whose agent runs on a
//! **target** act on its worktree there, and never on this host's clone (`#target-parity` 2/7).
//!
//! Reproduces the Coder probe of 2026-11-21: Restore of the target session `01a1fcba-…` cut a
//! worktree in the project's *local* clone and rewrote the session's changeset to point at it,
//! while the agent kept working on the target.
//!
//! The target setup is [`target_session_support`]'s.
//!
//! PRD: docs/ft/web/session-worktree-inspector.md.

mod target_session_support;

use pretty_assertions::assert_eq;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::worktree::*;
use tddy_worktree_service::test_util::TEST_TOKEN;

use target_session_support::{
    a_daemon_with_a_target_session, git_output, reachable, run_git, ADaemon, Reach, BRANCH,
    REMOTE_WORKTREE, SESSION_ID,
};

/// A file the agent left in its worktree, which git does not track.
const UNTRACKED: &str = "scratch.log";

impl ADaemon {
    /// How many worktrees this host's clone of the project has (its main one included).
    fn local_worktree_count(&self) -> usize {
        git_output(&self.local_clone, &["worktree", "list", "--porcelain"])
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count()
    }

    /// The worktree the session records.
    fn recorded_worktree(&self) -> Option<String> {
        tddy_core::read_changeset(&self.session_dir)
            .expect("the changeset reads")
            .repo_path
    }

    /// The agent left a file git does not track in its worktree on the target.
    fn agent_left_an_untracked_file(&self) {
        std::fs::write(self.remote_worktree().join(UNTRACKED), "x\n").unwrap();
    }

    /// The session's worktree was removed on the target; its branch is still there.
    fn worktree_is_gone_on_the_target(&self) {
        run_git(
            &self.remote_clone(),
            &[
                "worktree",
                "remove",
                "--force",
                self.remote_worktree().to_str().unwrap(),
            ],
        );
    }

    async fn restores(&self) -> Result<RestoreSessionWorktreeResponse, Status> {
        self.service
            .restore_session_worktree(Request::direct(RestoreSessionWorktreeRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: self.project_id.clone(),
                session_id: SESSION_ID.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn cleans(&self) -> Result<CleanWorktreeResponse, Status> {
        self.service
            .clean_worktree(Request::direct(CleanWorktreeRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: self.project_id.clone(),
                worktree_path: REMOTE_WORKTREE.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn removes(&self) -> Result<RemoveWorktreeResponse, Status> {
        self.service
            .remove_worktree(Request::direct(RemoveWorktreeRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: self.project_id.clone(),
                worktree_path: REMOTE_WORKTREE.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }
}

#[tokio::test]
async fn restore_of_a_target_sessions_worktree_never_cuts_one_on_the_daemons_host() {
    // Given a target session whose worktree is gone on the target
    let daemon = a_daemon_with_a_target_session(reachable);
    daemon.worktree_is_gone_on_the_target();

    // When the Worktree tab's Restore is pressed for it
    let _ = daemon.restores().await;

    // Then no worktree was cut in the local clone, and the session still names its worktree on
    // the target
    assert_eq!(
        (daemon.local_worktree_count(), daemon.recorded_worktree()),
        (1, Some(REMOTE_WORKTREE.to_string()))
    );
}

#[tokio::test]
async fn restore_of_a_target_sessions_worktree_recreates_it_on_the_target() {
    // Given a target session whose worktree is gone on the target, its branch still there
    let daemon = a_daemon_with_a_target_session(reachable);
    daemon.worktree_is_gone_on_the_target();

    // When
    let restored = daemon.restores().await;

    // Then the worktree is back where the session records it, on its branch
    assert_eq!(
        (
            restored
                .map(|r| r.worktree_path)
                .map_err(|s| s.message().to_string()),
            daemon.remote_worktree().is_dir(),
            git_output(&daemon.remote_worktree(), &["branch", "--show-current"]),
        ),
        (Ok(REMOTE_WORKTREE.to_string()), true, BRANCH.to_string())
    );
}

#[tokio::test]
async fn clean_of_a_target_sessions_worktree_cleans_it_on_the_target() {
    // Given a target session whose agent left an untracked file in its worktree there
    let daemon = a_daemon_with_a_target_session(reachable);
    daemon.agent_left_an_untracked_file();

    // When
    let cleaned = daemon.cleans().await;

    // Then the file is gone, and the tracked files stay
    assert_eq!(
        (
            cleaned.map(|r| r.ok).map_err(|s| s.message().to_string()),
            daemon.remote_worktree().join(UNTRACKED).exists(),
            daemon.remote_worktree().join("README.md").exists(),
        ),
        (Ok(true), false, true)
    );
}

#[tokio::test]
async fn delete_of_a_target_sessions_worktree_removes_it_on_the_target() {
    // Given a target session with its worktree on the target
    let daemon = a_daemon_with_a_target_session(reachable);

    // When
    let removed = daemon.removes().await;

    // Then the worktree is gone there, the target's clone no longer lists it, and the local clone
    // is untouched
    let listed = git_output(&daemon.remote_clone(), &["worktree", "list", "--porcelain"]);
    assert_eq!(
        (
            removed.map(|r| r.ok).map_err(|s| s.message().to_string()),
            daemon.remote_worktree().exists(),
            listed.contains(SESSION_ID),
            daemon.local_worktree_count(),
        ),
        (Ok(true), false, false, 1)
    );
}

#[tokio::test]
async fn an_action_on_an_unreachable_target_fails_with_its_reason_and_changes_nothing() {
    // Given a target session whose target cannot be reached now
    let daemon = a_daemon_with_a_target_session(|_| {
        Reach::Refused("the Coder vault of ada is locked".to_string())
    });
    daemon.agent_left_an_untracked_file();

    // When
    let refused = daemon.cleans().await;

    // Then the action fails with the reason, and nothing changed on either side
    assert_eq!(
        (
            refused
                .err()
                .map(|s| (s.code(), s.message().contains("vault of ada is locked"))),
            daemon.remote_worktree().join(UNTRACKED).exists(),
            daemon.local_worktree_count(),
        ),
        (Some((Code::FailedPrecondition, true)), true, 1)
    );
}
