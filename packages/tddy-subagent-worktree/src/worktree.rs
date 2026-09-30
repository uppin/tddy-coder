//! Creating, committing in, handing over and removing one conversation's worktree.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change_facts::{FileCounts, LineCounts, WorktreeChange};
use crate::conversation_id::ConversationId;

/// Where conversation worktrees live, relative to the session worktree. Added to the repository's
/// `info/exclude` on first use, so it never shows in the caller's `git status`.
pub const SUBAGENT_WORKTREES_DIR: &str = "tmp/subagent-worktrees";

/// Every conversation worktree of one session: the session worktree they are cut from and hand
/// back to, and the session id their branches are namespaced by — sessions of one project share
/// the repository's common dir, and the caller chooses conversation ids, so two sessions may both
/// name one `explore`.
#[derive(Debug, Clone)]
pub struct ConversationWorktrees {
    session_worktree: PathBuf,
    session_id: String,
}

/// One conversation's worktree, as found or created.
#[derive(Debug, Clone)]
pub struct ConversationWorktree {
    root: PathBuf,
    caller: PathBuf,
    branch: String,
    base: String,
}

/// What a hand-over applied to the caller's worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullOutcome {
    pub files: FileCounts,
    pub lines: LineCounts,
    /// Paths written with conflict markers because the caller changed the same lines.
    pub conflicts: Vec<String>,
}

/// A git or filesystem failure, naming what was attempted.
#[derive(Debug)]
pub enum WorktreeError {
    /// `git <args>` exited unsuccessfully.
    Git { args: Vec<String>, stderr: String },
    /// A filesystem operation failed.
    Io {
        context: String,
        source: std::io::Error,
    },
    /// The session worktree is not a git checkout.
    NotARepository(PathBuf),
}

impl ConversationWorktrees {
    pub fn new(session_worktree: impl Into<PathBuf>, session_id: impl Into<String>) -> Self {
        Self {
            session_worktree: session_worktree.into(),
            session_id: session_id.into(),
        }
    }

    /// `<session worktree>/tmp/subagent-worktrees/<conversation>`.
    pub fn path_of(&self, conversation: &ConversationId) -> PathBuf {
        self.session_worktree
            .join(SUBAGENT_WORKTREES_DIR)
            .join(conversation.as_str())
    }

    /// `tddy/subagent/<session>/<conversation>`.
    pub fn branch_of(&self, conversation: &ConversationId) -> String {
        format!("tddy/subagent/{}/{}", self.session_id, conversation)
    }

    /// The conversation's worktree if it has one; never creates anything.
    pub async fn existing(
        &self,
        conversation: &ConversationId,
    ) -> Result<Option<ConversationWorktree>, WorktreeError> {
        // TODO(isolated-edits): implement
        todo!("existing({conversation})")
    }

    /// The conversation's worktree, created on first use: cut from the session worktree's `HEAD`,
    /// with the session worktree's uncommitted state — staged, unstaged and untracked-not-ignored —
    /// as one commit on the new branch. The session worktree's index, branch and files are not
    /// touched.
    pub async fn ensure(
        &self,
        conversation: &ConversationId,
    ) -> Result<ConversationWorktree, WorktreeError> {
        // TODO(isolated-edits): implement
        todo!("ensure({conversation})")
    }
}

impl ConversationWorktree {
    /// Where the conversation's tools run.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The conversation's branch.
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// The commit the conversation started from: the inherited-changes commit, or the caller's
    /// `HEAD` when the caller was clean.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The session worktree this conversation hands back to.
    pub fn caller(&self) -> &Path {
        &self.caller
    }

    /// Commit everything that changed since the last commit, with `subject`, and say what it was.
    /// Changing nothing commits nothing and returns counts of zero with no `commit`.
    pub async fn commit_changes(&self, subject: &str) -> Result<WorktreeChange, WorktreeError> {
        // TODO(isolated-edits): implement
        todo!("commit_changes({subject:?})")
    }

    /// Apply everything committed since the base to the caller's worktree as uncommitted changes,
    /// 3-way: a hunk that no longer applies is written with conflict markers and its path reported.
    /// The caller's `HEAD` never moves.
    pub async fn pull_into_caller(&self) -> Result<PullOutcome, WorktreeError> {
        // TODO(isolated-edits): implement
        todo!("pull_into_caller")
    }

    /// Delete the worktree, its branch and its base ref.
    pub async fn remove(self) -> Result<(), WorktreeError> {
        // TODO(isolated-edits): implement
        todo!("remove")
    }
}

impl fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git { args, stderr } => {
                write!(f, "git {} failed: {}", args.join(" "), stderr.trim())
            }
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::NotARepository(path) => {
                write!(f, "{} is not a git worktree", path.display())
            }
        }
    }
}

impl std::error::Error for WorktreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
