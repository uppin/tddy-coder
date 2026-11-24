use std::path::Path;
use tddy_service::proto::session::CloseSessionCommit;
use tddy_service::proto::session::CloseSessionDirtyFile;
use tddy_service::proto::session::CloseSessionWorktree;
use tddy_worktree_service::worktree_loss::read_local_worktree_loss;
use tddy_worktree_service::worktree_loss::WorktreeLoss;
use tddy_worktree_service::worktree_sync::DirtyChange;

/// The base the session's worktree was cut from, as its changeset recorded it.
pub(super) fn recorded_base(session_dir: &Path) -> Option<String> {
    tddy_core::read_changeset(session_dir)
        .ok()?
        .effective_worktree_integration_base_ref
        .filter(|base| !base.trim().is_empty())
}

/// The branch the session's changeset records — what a worktree no longer on disk was on.
pub(super) fn recorded_branch(session_dir: &Path) -> String {
    tddy_core::read_changeset(session_dir)
        .ok()
        .and_then(|changeset| changeset.branch)
        .unwrap_or_default()
}

/// A worktree on this host: what removing it would lose, and the branch it is on.
pub(super) async fn read_local_worktree(path: &Path, base: Option<&str>) -> CloseSessionWorktree {
    let view = CloseSessionWorktree::default();
    let (worktree, base) = (path.to_path_buf(), base.map(str::to_string));
    let read = tokio::task::spawn_blocking(move || {
        read_local_worktree_loss(&worktree, base.as_deref())
            .map(|loss| (loss, local_branch(&worktree)))
    })
    .await;
    match read {
        Ok(Ok((loss, branch))) => CloseSessionWorktree {
            branch,
            ..with_loss(view, loss)
        },
        Ok(Err(error)) => unavailable(view, error.to_string()),
        Err(error) => unavailable(view, format!("the read did not finish: {error}")),
    }
}

/// The branch checked out in `worktree`; empty on a detached `HEAD`.
fn local_branch(worktree: &Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["symbolic-ref", "--short", "-q", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The branch checked out in `worktree` on a target; empty on a detached `HEAD`.
pub(super) async fn target_branch(
    shell: &dyn tddy_remote_shell::RemoteShell,
    worktree: &str,
) -> String {
    let command = format!(
        "git -C {} symbolic-ref --short -q HEAD",
        tddy_core::shell_single_quote(worktree)
    );
    match shell
        .exec(tddy_remote_shell::ExecRequest::new(command))
        .await
    {
        Ok(output) if output.status == 0 => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

/// `view` carrying what removing it would lose.
pub(super) fn with_loss(view: CloseSessionWorktree, loss: WorktreeLoss) -> CloseSessionWorktree {
    CloseSessionWorktree {
        dirty_files: loss
            .dirty_files
            .into_iter()
            .map(|file| CloseSessionDirtyFile {
                path: file.path,
                change: change_name(file.change).to_string(),
            })
            .collect(),
        unpushed_commits: loss
            .unpushed
            .into_iter()
            .map(|commit| CloseSessionCommit {
                sha: commit.sha,
                subject: commit.subject,
            })
            .collect(),
        unpushed_commit_count: loss.unpushed_count,
        ..view
    }
}

/// `view`, which could not be read, saying why.
pub(super) fn unavailable(view: CloseSessionWorktree, reason: String) -> CloseSessionWorktree {
    CloseSessionWorktree {
        unavailable_reason: reason,
        ..view
    }
}

/// A dirty file's change as the wire names it.
fn change_name(change: DirtyChange) -> &'static str {
    match change {
        DirtyChange::Modified => "modified",
        DirtyChange::Staged => "staged",
        DirtyChange::Untracked => "untracked",
        DirtyChange::Deleted => "deleted",
        DirtyChange::Renamed => "renamed",
    }
}
