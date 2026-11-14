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
//! PRD: docs/ft/daemon/project-concept.md § Git tools.

use std::fmt;

use crate::session_target::SessionTarget;

mod run;

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

/// The `GIT_*` environment every git command of an operation runs with — the identity the target
/// session's own commits and pushes are made under (its project's account, or the host's `gh`).
/// Empty when the target's identity does not resolve: the checkout's inherited identity applies.
///
/// A push goes to [`Self::remote_name`] — the project's remote — and authenticates with
/// [`Self::account_token`], which a daemon-run git has no other way to obtain: it is handed to git
/// through `tddy-askpass` (`GIT_ASKPASS`), never on a command line (`#orchestrators` 10/11).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PushIdentity {
    pub env: Vec<(String, String)>,
    /// The project's remote (`ProjectData.remote_name`); `origin` when `None`.
    pub remote_name: Option<String>,
    /// The project account's GitHub token, for a push over HTTPS. `None` when the project resolves
    /// no account: the checkout's own credentials apply.
    pub account_token: Option<String>,
}

impl fmt::Debug for PushIdentity {
    /// Never shows the token: an identity is logged with the operation it failed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PushIdentity")
            .field("env", &self.env)
            .field("remote_name", &self.remote_name)
            .field(
                "account_token",
                &self.account_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
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
///
/// Commit and a plain push only add history or publish it, so they run beside an active agent. The
/// rest rewrite the target's files or history under it, and a force-with-lease push rewrites the
/// remote branch the agent builds on.
pub fn requires_inactive_target(op: &GitOp) -> bool {
    match op {
        GitOp::Commit { .. } => false,
        GitOp::Push { force_with_lease } => *force_with_lease,
        GitOp::Rebase { .. }
        | GitOp::Stash
        | GitOp::Reset { .. }
        | GitOp::Checkout { .. }
        | GitOp::Clean => true,
    }
}

/// Run `op` on `target`, applying the activity gate first.
///
/// The gate is checked before any git process starts, so a refused operation changes nothing.
pub fn run_git_op(
    target: &SessionTarget,
    op: GitOp,
    push_identity: &PushIdentity,
) -> Result<GitOpOutcome, GitOpError> {
    if target.is_active && requires_inactive_target(&op) {
        return Err(GitOpError::TargetActive {
            session_id: target.session_id.clone(),
        });
    }
    // TODO(stack-git-tools): implement — push to `push_identity.remote_name` (`origin` when
    // `None`) and hand `account_token` to git through `tddy-askpass`; today every push goes to
    // `origin` with the checkout's credentials.
    run::run(&target.worktree, &push_identity.env, op)
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

    // ── push remote (`#orchestrators` 10/11) ──────────────────────────────────────────────────

    fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} in {dir:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A session worktree on `feature` whose only remote is a bare repository named `upstream`.
    /// Answers the temp dir, the worktree and the bare remote.
    fn a_worktree_whose_only_remote_is_upstream(
    ) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("upstream.git");
        let worktree = tmp.path().join("work");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::create_dir_all(&worktree).unwrap();
        git_in(&bare, &["init", "-q", "--bare", "-b", "main"]);
        git_in(&worktree, &["init", "-q", "-b", "feature"]);
        git_in(&worktree, &["commit", "-q", "--allow-empty", "-m", "work"]);
        git_in(
            &worktree,
            &["remote", "add", "upstream", bare.to_str().unwrap()],
        );
        (tmp, worktree, bare)
    }

    fn a_target_at(worktree: &std::path::Path) -> SessionTarget {
        SessionTarget {
            session_id: "child-1".to_string(),
            project_id: "proj".to_string(),
            worktree: worktree.to_path_buf(),
            branch: "feature".to_string(),
            base_ref: "upstream/main".to_string(),
            is_active: false,
        }
    }

    #[test]
    fn a_push_goes_to_the_remote_its_identity_names() {
        // Given a worktree whose only remote is `upstream`, and an identity naming it
        let (_tmp, worktree, bare) = a_worktree_whose_only_remote_is_upstream();
        let identity = PushIdentity {
            remote_name: Some("upstream".to_string()),
            ..PushIdentity::default()
        };

        // When its branch is pushed
        let pushed = run_git_op(
            &a_target_at(&worktree),
            GitOp::Push {
                force_with_lease: false,
            },
            &identity,
        );

        // Then the push lands on `upstream`
        assert!(pushed.is_ok(), "{pushed:?}");
        assert_eq!(
            git_in(&bare, &["rev-parse", "refs/heads/feature"]),
            git_in(&worktree, &["rev-parse", "HEAD"])
        );
    }

    #[test]
    fn a_push_identity_never_shows_its_token() {
        // Given an identity carrying the account's token
        let identity = PushIdentity {
            account_token: Some("gho_secret".to_string()),
            ..PushIdentity::default()
        };

        // Then its debug form does not show it
        assert!(!format!("{identity:?}").contains("gho_secret"));
    }
}
