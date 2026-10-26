//! The branch a cancelled start cut, and its removal (`#start-parity` 4/9).
//!
//! A start that resolves to `new_branch_from_base` cuts a branch of its own in its project's
//! repository, with `git worktree add -b`. A cancel removes the session that start made, and the
//! branch goes with it: a branch left behind would outlive the start that was cancelled. A start on
//! `work_on_selected_branch` cuts no branch, and a branch it works on is never deleted.

use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::BranchWorktreeIntent;
use tddy_rpc::Status;
use tddy_session_split::service_util;

use crate::launch_ports::LaunchSessions;

/// A branch a start cut in its project's repository, and the repository it was cut in.
pub(crate) struct CutBranch {
    repo_root: PathBuf,
    pub(crate) branch: String,
}

impl LaunchSessions {
    /// The branch the session `session_id` cut for itself, read from its changeset. Read before the
    /// session's directory is removed, since the changeset is the only record of the branch.
    ///
    /// `None` when the session cut none: its changeset did not resolve to `new_branch_from_base`, or
    /// it has no worktree of its own under the project's `.worktrees` (a client's checkout is never
    /// cut, so a branch named there is never ours to delete).
    pub(crate) fn branch_a_start_cut(
        &self,
        session_token: &str,
        session_id: &str,
        project_id: &str,
    ) -> Result<Option<CutBranch>, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(&sessions_base, session_id);
        let changeset = tddy_core::read_changeset(&session_dir).map_err(|e| {
            Status::internal(format!(
                "the changeset of session {session_id} could not be read: {e}"
            ))
        })?;
        let intent = changeset
            .workflow
            .as_ref()
            .and_then(|w| w.branch_worktree_intent);
        let branch = changeset
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty());
        let cut_its_own_worktree = changeset.worktree.as_deref().is_some_and(|w| {
            tddy_session_activity::session_deletion::is_daemon_managed_worktree(Path::new(w))
        });
        if intent != Some(BranchWorktreeIntent::NewBranchFromBase) || !cut_its_own_worktree {
            return Ok(None);
        }
        let Some(branch) = branch else {
            return Ok(None);
        };
        let (_, project) =
            service_util::find_registered_project(&self.tddy_data_dir, &os_user, project_id)?;
        Ok(Some(CutBranch {
            repo_root: service_util::project_repo_root(&project)?,
            branch: branch.to_string(),
        }))
    }
}

impl CutBranch {
    /// Force-deletes the branch. Called once the session's worktree is gone: git refuses a branch
    /// still checked out in any worktree, so a branch another session uses is protected by git, and a
    /// failure here is returned, never worked around.
    ///
    /// A branch already absent is the state this asks for, and is not an error.
    pub(crate) fn delete(&self) -> Result<(), String> {
        let refname = format!("refs/heads/{}", self.branch);
        let present = Command::new("git")
            .current_dir(&self.repo_root)
            .args(["show-ref", "--verify", "--quiet", &refname])
            .status()
            .map_err(|e| format!("git show-ref: {e}"))?;
        match present.code() {
            Some(0) => {}
            Some(1) => {
                log::info!(
                    "cancelled start: branch {} is already absent from {}",
                    self.branch,
                    self.repo_root.display()
                );
                return Ok(());
            }
            other => {
                return Err(format!(
                    "git show-ref {refname} in {} exited with {other:?}",
                    self.repo_root.display()
                ))
            }
        }
        let out = Command::new("git")
            .current_dir(&self.repo_root)
            .args(["branch", "-D", &self.branch])
            .output()
            .map_err(|e| format!("git branch -D: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        log::info!(
            "cancelled start: deleted branch {} from {}",
            self.branch,
            self.repo_root.display()
        );
        Ok(())
    }
}
