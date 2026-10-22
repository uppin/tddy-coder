//! Git on a project-home session's target worktree (`#project-home` 7/7).
//!
//! Commit and a plain push change no files under the target's agent, so they always run. Every
//! operation that rewrites the target's files or history — rebase, stash, reset, checkout, clean,
//! and a force-with-lease push — runs only while the target session is **inactive**, and is
//! refused naming it otherwise. A rebase that conflicts is aborted, leaving the worktree as it was,
//! and reports the conflicting paths.
//!
//! The target comes from `session_target::resolve_session_target` (node 5) and carries its
//! activity.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-git-tools.md.

use std::fmt;

use crate::session_target::SessionTarget;

/// `git reset` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

/// One git operation on a target worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOp {
    /// Commit `paths` (every change when empty) with `message`.
    Commit {
        message: String,
        paths: Vec<String>,
    },
    /// Push the session's branch to its remote; `force_with_lease` rewrites the remote branch.
    Push {
        force_with_lease: bool,
    },
    Rebase {
        onto: String,
    },
    Stash,
    Reset {
        mode: ResetMode,
        to: String,
    },
    Checkout {
        to: String,
    },
    Clean,
}

/// Credentials and environment a push runs with — the identity the target session itself pushes
/// with (its project's account, or the host's own `gh`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushIdentity {
    pub env: Vec<(String, String)>,
}

/// What a git operation did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOpOutcome {
    Done {
        summary: String,
    },
    /// The rebase was aborted; the worktree is as it was before it.
    RebaseConflict {
        paths: Vec<String>,
    },
}

/// Why a git operation did not run or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOpError {
    /// The operation rewrites the target and its session is active.
    TargetActive {
        session_id: String,
    },
    Git(String),
}

impl fmt::Display for GitOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetActive { session_id } => {
                write!(
                    f,
                    "session {session_id} is active; stop it before rewriting its worktree"
                )
            }
            Self::Git(msg) => write!(f, "git failed: {msg}"),
        }
    }
}

impl std::error::Error for GitOpError {}

/// Whether `op` may run only while the target session is inactive.
pub fn requires_inactive_target(op: &GitOp) -> bool {
    // TODO(home-git-tools): implement
    let _ = op;
    false
}

/// Run `op` on `target`, applying the activity gate first.
pub fn run_git_op(
    target: &SessionTarget,
    op: GitOp,
    push_identity: &PushIdentity,
) -> Result<GitOpOutcome, GitOpError> {
    // TODO(home-git-tools): implement
    let _ = (target, op, push_identity);
    Err(GitOpError::Git("run_git_op is not implemented".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_history_or_file_rewriting_ops_require_an_inactive_target() {
        // Given every operation
        let ops = [
            (
                GitOp::Commit {
                    message: "m".into(),
                    paths: vec![],
                },
                false,
            ),
            (
                GitOp::Push {
                    force_with_lease: false,
                },
                false,
            ),
            (
                GitOp::Push {
                    force_with_lease: true,
                },
                true,
            ),
            (
                GitOp::Rebase {
                    onto: "origin/main".into(),
                },
                true,
            ),
            (GitOp::Stash, true),
            (
                GitOp::Reset {
                    mode: ResetMode::Hard,
                    to: "HEAD~1".into(),
                },
                true,
            ),
            (GitOp::Checkout { to: "main".into() }, true),
            (GitOp::Clean, true),
        ];

        // Then
        for (op, expected) in ops {
            assert_eq!(requires_inactive_target(&op), expected, "{op:?}");
        }
    }
}
