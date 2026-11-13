//! `SetSessionMark`, `DeleteSession`, `ClearWorktree`, `DeleteWorktree` and `RestoreWorktree`
//! answered on this host (`#orchestrators` 5/11): the mark is written to the target's
//! `.session.yaml`, a delete is the web's Delete, and the three worktree actions call the
//! `tddy-worktree-service` functions the Worktrees screen's RPCs call.

use std::path::{Path, PathBuf};
use std::process::Command;

use async_trait::async_trait;
use tddy_core::orchestration_mark::OrchestrationMark;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Request;
use tddy_service::proto::session::DeleteSessionRequest;
use tddy_tool_engine::project_home_lifecycle_tools::ProjectHomeLifecyclePort;
use tddy_worktree_service::home_worktree::home_worktree_path;
use tddy_worktree_service::service::project_main_repo_on_host;
use tddy_worktree_service::session_target::SessionTarget;
use tddy_worktree_service::worktrees::{
    clean_worktree_under_repo, remove_worktree_under_repo, CleanWorktreeError, RemoveWorktreeError,
};

use crate::exec_tool::project_home::DaemonProjectHome;

#[async_trait]
impl ProjectHomeLifecyclePort for DaemonProjectHome {
    async fn set_session_mark(
        &self,
        target: &SessionTarget,
        mark: Option<&OrchestrationMark>,
    ) -> Result<(), String> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &target.session_id);
        let mark = mark.cloned();
        let session_id = target.session_id.clone();
        tokio::task::spawn_blocking(move || {
            let mut metadata = tddy_core::read_session_metadata(&session_dir)
                .map_err(|e| format!("cannot read session {session_id}'s record: {e}"))?;
            metadata.orchestration_mark = mark;
            tddy_core::session_metadata::write_session_metadata(&session_dir, &metadata)
                .map_err(|e| format!("cannot record session {session_id}'s mark: {e}"))
        })
        .await
        .map_err(|e| format!("recording the mark failed: {e}"))?
    }

    async fn delete_session(&self, target: &SessionTarget) -> Result<(), String> {
        self.launch
            .delete_session_at_session_coordinate(Request::direct(DeleteSessionRequest {
                session_token: self.session_token.clone(),
                session_id: target.session_id.clone(),
            }))
            .await
            .map(|_| ())
            .map_err(|status| format!("cannot delete: {}", status.message()))
    }

    async fn clear_worktree(&self, project_id: &str, path: &Path) -> Result<(), String> {
        let main_repo = self.main_repo(project_id)?;
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            clean_worktree_under_repo(&main_repo, &path).map_err(|e| match e {
                CleanWorktreeError::GitFailed { message } => message,
                CleanWorktreeError::NotListed => {
                    format!("{} is not a worktree of this project", path.display())
                }
                CleanWorktreeError::CannotCleanPrimary => {
                    format!("{} is the project's main checkout", path.display())
                }
                CleanWorktreeError::Io(message) => message,
            })
        })
        .await
        .map_err(|e| format!("clearing the worktree failed: {e}"))?
    }

    async fn delete_worktree(&self, project_id: &str, path: &Path) -> Result<(), String> {
        let main_repo = self.main_repo(project_id)?;
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            if worktree_is_dirty(&path) {
                return Err(format!(
                    "worktree {} has uncommitted changes: commit them with CommitSessionWorktree or set them aside with StashSessionWorktree first",
                    path.display()
                ));
            }
            remove_worktree_under_repo(&main_repo, &path).map_err(|e| match e {
                RemoveWorktreeError::GitFailed { message } => {
                    if message.contains("modified or untracked")
                        || message.contains("contains modified")
                    {
                        format!(
                            "worktree {} has uncommitted changes: commit them with CommitSessionWorktree or set them aside with StashSessionWorktree first",
                            path.display()
                        )
                    } else {
                        message
                    }
                }
                RemoveWorktreeError::NotListed => {
                    format!("{} is not a worktree of this project", path.display())
                }
                RemoveWorktreeError::CannotRemovePrimary => {
                    format!("{} is the project's main checkout", path.display())
                }
                RemoveWorktreeError::Io(message) => message,
            })
        })
        .await
        .map_err(|e| format!("deleting the worktree failed: {e}"))?
    }

    async fn project_home_worktree(&self, project_id: &str) -> Result<PathBuf, String> {
        let main_repo = self.main_repo(project_id)?;
        Ok(home_worktree_path(
            &tddy_core::worktree_dir(&main_repo),
            project_id,
        ))
    }

    async fn restore_worktree(&self, target: &SessionTarget) -> Result<PathBuf, String> {
        let main_repo = self.main_repo(&target.project_id)?;
        let session_dir = unified_session_dir_path(&self.sessions_base, &target.session_id);
        tokio::task::spawn_blocking(move || {
            tddy_core::resolve_persisted_worktree_integration_base_for_session(
                &session_dir,
                &main_repo,
            )
            .and_then(|base_ref| {
                tddy_core::setup_worktree_for_session_with_integration_base(
                    &main_repo,
                    &session_dir,
                    &base_ref,
                )
            })
        })
        .await
        .map_err(|e| format!("restoring the worktree failed: {e}"))?
    }
}

impl DaemonProjectHome {
    fn main_repo(&self, project_id: &str) -> Result<PathBuf, String> {
        project_main_repo_on_host(
            &self.tddy_data_dir,
            &self.os_user,
            &self.daemon_instance_id,
            project_id,
        )
        .map_err(|status| status.message().to_string())
    }
}

/// Whether `path` holds uncommitted changes (tracked or untracked), as `git status` reports them.
fn worktree_is_dirty(path: &Path) -> bool {
    Command::new("git")
        .current_dir(path)
        .args(["status", "--porcelain"])
        .output()
        .map(|out| out.status.success() && !out.stdout.is_empty())
        .unwrap_or(false)
}
