//! Creating, committing in, handing over and removing one conversation's worktree.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change_facts::count_change;
use crate::change_facts::{FileCounts, LineCounts, WorktreeChange};
use crate::conversation_id::ConversationId;
use crate::git::{git, git_raw};
use crate::inherit;
use crate::serialise;

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
    /// The ref that keeps `base` findable for a later `existing`, and alive when it is the
    /// inherited-changes commit no branch points at.
    base_ref: String,
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

    pub(crate) fn session_worktree(&self) -> &Path {
        &self.session_worktree
    }

    fn base_ref_of(&self, conversation: &ConversationId) -> String {
        format!(
            "refs/tddy/subagent-base/{}/{}",
            self.session_id, conversation
        )
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
        let base = self.recorded_base(conversation).await?;
        Ok(base.and_then(|base| self.live_worktree(conversation, base)))
    }

    /// The commit the conversation's base ref records, if the ref exists.
    async fn recorded_base(
        &self,
        conversation: &ConversationId,
    ) -> Result<Option<String>, WorktreeError> {
        let (_, found) = git_raw(
            &self.session_worktree,
            [
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{}^{{commit}}", self.base_ref_of(conversation)),
            ],
            &[],
            None,
        )
        .await?;
        Ok(found
            .success
            .then(|| String::from_utf8_lossy(&found.stdout).trim().to_string()))
    }

    /// The worktree for `base`, if its directory is still there.
    fn live_worktree(
        &self,
        conversation: &ConversationId,
        base: String,
    ) -> Option<ConversationWorktree> {
        let root = self.path_of(conversation);
        root.exists().then(|| ConversationWorktree {
            root,
            caller: self.session_worktree.clone(),
            branch: self.branch_of(conversation),
            base,
            base_ref: self.base_ref_of(conversation),
        })
    }

    /// The conversation's worktree, created on first use: cut from the session worktree's `HEAD`,
    /// with the session worktree's uncommitted state — staged, unstaged and untracked-not-ignored —
    /// as one commit on the new branch. The session worktree's index, branch and files are not
    /// touched.
    ///
    /// Concurrent calls for one conversation are serialised, so the worktree is created once. A
    /// conversation whose directory was deleted but whose branch survived gets its worktree back
    /// on that branch, with its commits; any other leftover is cleared and the conversation
    /// starts afresh.
    pub async fn ensure(
        &self,
        conversation: &ConversationId,
    ) -> Result<ConversationWorktree, WorktreeError> {
        let root = self.path_of(conversation);
        let _exclusive = serialise::exclusive(&root).await;
        let recorded = self.recorded_base(conversation).await?;
        if let Some(found) = recorded
            .clone()
            .and_then(|base| self.live_worktree(conversation, base))
        {
            return Ok(found);
        }
        let caller = &self.session_worktree;
        let branch = self.branch_of(conversation);
        // A directory git still has registered, but that is gone, would refuse `worktree add`.
        git(caller, ["worktree", "prune"], &[], None).await?;
        let branch_survived = self.branch_exists(&branch).await?;
        if let (Some(base), true) = (recorded, branch_survived) {
            return self.reattach(conversation, root, base).await;
        }
        self.clear_leftovers(&root, &branch, branch_survived)
            .await?;
        inherit::exclude_conversation_worktrees(&inherit::common_dir(caller).await?).await?;
        let base = inherit::base_commit(caller).await?;
        let base_ref = self.base_ref_of(conversation);
        git(
            caller,
            [
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("-q"),
                OsStr::new("-b"),
                OsStr::new(&branch),
                root.as_os_str(),
                OsStr::new(&base),
            ],
            &[],
            None,
        )
        .await?;
        git(caller, ["update-ref", &base_ref, &base], &[], None).await?;
        Ok(ConversationWorktree {
            root,
            caller: caller.clone(),
            branch,
            base,
            base_ref,
        })
    }

    async fn branch_exists(&self, branch: &str) -> Result<bool, WorktreeError> {
        let (_, found) = git_raw(
            &self.session_worktree,
            [
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            &[],
            None,
        )
        .await?;
        Ok(found.success)
    }

    /// Check the surviving branch out again at `root`, keeping its commits and its recorded base.
    async fn reattach(
        &self,
        conversation: &ConversationId,
        root: PathBuf,
        base: String,
    ) -> Result<ConversationWorktree, WorktreeError> {
        let branch = self.branch_of(conversation);
        git(
            &self.session_worktree,
            [
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("-q"),
                root.as_os_str(),
                OsStr::new(&branch),
            ],
            &[],
            None,
        )
        .await?;
        Ok(ConversationWorktree {
            root,
            caller: self.session_worktree.clone(),
            branch,
            base,
            base_ref: self.base_ref_of(conversation),
        })
    }

    /// Clear what a creation that never finished left behind: a branch whose base ref was lost
    /// (its commits have no known base to be handed back against) and a worktree directory that
    /// was never recorded.
    async fn clear_leftovers(
        &self,
        root: &Path,
        branch: &str,
        branch_survived: bool,
    ) -> Result<(), WorktreeError> {
        let caller = &self.session_worktree;
        if root.exists() {
            git(
                caller,
                [
                    OsStr::new("worktree"),
                    OsStr::new("remove"),
                    OsStr::new("--force"),
                    root.as_os_str(),
                ],
                &[],
                None,
            )
            .await?;
        }
        if branch_survived {
            git(caller, ["branch", "-D", branch], &[], None).await?;
        }
        Ok(())
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
        let _exclusive = serialise::exclusive(&self.root).await;
        git(&self.root, ["add", "-A"], &[], None).await?;
        let name_status = git(&self.root, ["diff", "--cached", "--name-status"], &[], None).await?;
        if name_status.trim().is_empty() {
            return Ok(WorktreeChange::default());
        }
        let numstat = git(&self.root, ["diff", "--cached", "--numstat"], &[], None).await?;
        // The subagent's snapshot is not the developer's commit: their hooks and signing do not
        // apply (`crate::git` runs every call with hooks and signing off; `--no-verify` is the
        // explicit statement of it at the one call that commits).
        git(
            &self.root,
            ["commit", "-q", "--no-verify", "-m", subject],
            &[],
            None,
        )
        .await?;
        let commit = git(&self.root, ["rev-parse", "--short", "HEAD"], &[], None).await?;
        let (files, lines) = count_change(&name_status, &numstat);
        Ok(WorktreeChange {
            commit: Some(commit.trim().to_string()),
            files,
            lines,
        })
    }

    /// Apply everything committed since the base to the caller's worktree as uncommitted changes,
    /// 3-way: a hunk that no longer applies is written with conflict markers and its path reported.
    /// The caller's `HEAD` never moves.
    pub async fn pull_into_caller(&self) -> Result<PullOutcome, WorktreeError> {
        let range = format!("{}..{}", self.base, self.branch);
        let name_status = git(&self.caller, ["diff", "--name-status", &range], &[], None).await?;
        if name_status.trim().is_empty() {
            return Ok(PullOutcome::default());
        }
        let numstat = git(&self.caller, ["diff", "--numstat", &range], &[], None).await?;
        let (files, lines) = count_change(&name_status, &numstat);
        let patch = git(&self.caller, ["diff", "--binary", &range], &[], None).await?;
        let touched = git(
            &self.caller,
            ["diff", "--name-only", "--no-renames", "-z", &range],
            &[],
            None,
        )
        .await?;
        let conflicts = self
            .apply_3way(patch.as_bytes(), touched.as_bytes())
            .await?;
        Ok(PullOutcome {
            files,
            lines,
            conflicts,
        })
    }

    /// Delete the worktree, its branch and its base ref.
    pub async fn remove(self) -> Result<(), WorktreeError> {
        let _exclusive = serialise::exclusive(&self.root).await;
        git(
            &self.caller,
            [
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                self.root.as_os_str(),
            ],
            &[],
            None,
        )
        .await?;
        git(&self.caller, ["branch", "-D", &self.branch], &[], None).await?;
        git(
            &self.caller,
            ["update-ref", "-d", &self.base_ref],
            &[],
            None,
        )
        .await?;
        Ok(())
    }

    /// Apply `patch` to the caller's files 3-way and return the paths left with conflict markers.
    ///
    /// `git apply --3way` implies `--index` and refuses a file whose working copy differs from the
    /// index — exactly the caller's unstaged edits. So it runs against a scratch index in which
    /// the touched paths are refreshed from the working tree: the caller's own index is never read
    /// for its staging state and never written, and the pulled changes arrive unstaged.
    pub(crate) async fn apply_3way(
        &self,
        patch: &[u8],
        touched: &[u8],
    ) -> Result<Vec<String>, WorktreeError> {
        let index = git(
            &self.caller,
            ["rev-parse", "--git-path", "index"],
            &[],
            None,
        )
        .await?;
        let index = self.caller.join(index.trim());
        let scratch = inherit::scratch_path(&index);
        tokio::fs::copy(&index, &scratch)
            .await
            .map_err(|source| WorktreeError::Io {
                context: format!("seeding the scratch index from {}", index.display()),
                source,
            })?;
        let result = self.apply_with_index(patch, touched, &scratch).await;
        if let Err(error) = tokio::fs::remove_file(&scratch).await {
            log::debug!("scratch index {} not removed: {error}", scratch.display());
        }
        result
    }

    async fn apply_with_index(
        &self,
        patch: &[u8],
        touched: &[u8],
        scratch: &Path,
    ) -> Result<Vec<String>, WorktreeError> {
        let env = [("GIT_INDEX_FILE", scratch.as_os_str())];
        git(
            &self.caller,
            ["update-index", "--add", "--remove", "-z", "--stdin"],
            &env,
            Some(touched),
        )
        .await?;
        let (args, applied) = git_raw(&self.caller, ["apply", "--3way"], &env, Some(patch)).await?;
        let conflicts = conflicted_paths(&applied.stderr);
        if applied.success || !conflicts.is_empty() {
            return Ok(conflicts);
        }
        Err(WorktreeError::Git {
            args,
            stderr: applied.stderr,
        })
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

/// Paths `git apply --3way` reports as applied "with conflicts" (`Applied patch to 'path' with
/// conflicts.`), in the order git reported them.
fn conflicted_paths(apply_stderr: &str) -> Vec<String> {
    apply_stderr
        .lines()
        .filter_map(|line| {
            line.strip_prefix("Applied patch to '")?
                .strip_suffix("' with conflicts.")
        })
        .map(str::to_string)
        .collect()
}
