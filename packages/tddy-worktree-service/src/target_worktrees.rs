//! The worktree of a session whose agent runs on a **target** — a machine the daemon reaches
//! through a remote shell (`#target-parity` 2/7).
//!
//! The project's clone on this host never holds such a worktree, so the Worktree tab's actions —
//! Clean, Delete, Restore — resolve the target session the request names (by its `worktree_path`,
//! or by `session_id` for Restore) and run on its target, through the shell [`TargetShells`] gives.
//! They never touch this host's clone.
//!
//! The shell comes through a port because the code that builds it (an ssh transport, or a Coder
//! workspace reached with its owner's vault credentials) lives in `tddy-agent-launch`, which
//! depends on this crate.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-21-target-worktree-actions.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::SessionTarget;
use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;

/// The shell of a session's target, resolved for `subject` — who the calling session's token
/// resolved to, the owner of the vault a Coder target's account is read from (as
/// `CoderCredentialSource::credentials_for`'s `subject`).
#[async_trait]
pub trait TargetShells: Send + Sync {
    /// The shell that reaches `target`. A target that cannot be reached now — a locked Coder
    /// vault, an unknown account — is an error whose message says why.
    async fn shell_for(
        &self,
        target: &SessionTarget,
        subject: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status>;
}

/// A daemon that reaches no target: every target session's worktree action is refused, saying so.
pub struct NoTargetShells;

#[async_trait]
impl TargetShells for NoTargetShells {
    async fn shell_for(
        &self,
        _target: &SessionTarget,
        _subject: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status> {
        Err(Status::failed_precondition(
            "this daemon reaches no session target",
        ))
    }
}

/// A session of the caller's whose agent runs on a target, and the worktree it has there.
#[derive(Debug, Clone)]
pub struct TargetSession {
    /// The session's directory on this host.
    pub session_dir: PathBuf,
    pub target: SessionTarget,
    /// The worktree on the target, as the session records it (`repo_path`).
    pub worktree: String,
}

/// The target session `.session.yaml` in `session_dir` records, or `None` for a session whose
/// agent runs on this host (or that records no worktree).
#[must_use]
pub fn target_session_in(session_dir: &Path) -> Option<TargetSession> {
    let metadata = tddy_core::read_session_metadata(session_dir).ok()?;
    Some(TargetSession {
        session_dir: session_dir.to_path_buf(),
        target: metadata.target?,
        worktree: metadata.repo_path?,
    })
}

/// The target session of `project_id` whose worktree on its target is `worktree`, among the
/// sessions under `sessions_dir`. `None` when no target session names that path — the path is then
/// a worktree of this host's clone.
#[must_use]
pub fn target_session_of_worktree(
    sessions_dir: &Path,
    project_id: &str,
    worktree: &str,
) -> Option<TargetSession> {
    std::fs::read_dir(sessions_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            tddy_core::read_session_metadata(&entry.path())
                .is_ok_and(|metadata| metadata.project_id == project_id)
        })
        .filter_map(|entry| target_session_in(&entry.path()))
        .find(|session| session.worktree == worktree)
}

/// The clone a target worktree belongs to: `<root>/.worktrees/<session>` → `<root>`. `None` for a
/// path that is not a tddy worktree path.
#[must_use]
pub fn target_checkout_root_of(worktree: &str) -> Option<String> {
    let worktrees = Path::new(worktree).parent()?;
    (worktrees.file_name()?.to_str()? == ".worktrees")
        .then(|| worktrees.parent())
        .flatten()
        .map(|root| root.to_string_lossy().into_owned())
}

fn quoted(text: &str) -> String {
    tddy_core::ssh_exec::shell_single_quote(text)
}

/// Run `command` on the target; a non-zero exit is an `internal` error carrying its output.
async fn run_on_target(shell: &dyn RemoteShell, command: String) -> Result<Vec<u8>, Status> {
    let output = shell
        .exec(tddy_remote_shell::ExecRequest::new(command))
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    let output = output
        .success_or_exited()
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(output.stdout)
}

/// Remove a target session's worktree on its target, through `shell`: the worktree, the pointer
/// inside it, and the clone's registration of it (`git worktree remove`, then `prune`), so the
/// clone does not keep a dangling entry.
pub async fn remove_target_worktree(shell: &dyn RemoteShell, worktree: &str) -> Result<(), Status> {
    let root = target_checkout_root_of(worktree).ok_or_else(|| {
        Status::invalid_argument(format!(
            "the target worktree {worktree} is not under a clone's .worktrees directory"
        ))
    })?;
    for command in [
        format!(
            "git -C {} worktree remove --force {}",
            quoted(&root),
            quoted(worktree)
        ),
        format!("git -C {} worktree prune", quoted(&root)),
    ] {
        run_on_target(shell, command).await?;
    }
    Ok(())
}

/// Clean a target session's worktree on its target: untracked and ignored files go.
pub async fn clean_target_worktree(shell: &dyn RemoteShell, worktree: &str) -> Result<(), Status> {
    run_on_target(shell, format!("git -C {} clean -fdx", quoted(worktree)))
        .await
        .map(|_| ())
}

/// Re-create a target session's worktree on its target at the recorded path, and answer that path.
///
/// The branch the session recorded is checked out again when the target's clone still has it;
/// otherwise it is cut from the integration base the session recorded, after a fetch. Nothing is
/// rewritten on this host: the session keeps naming its worktree on the target.
pub async fn restore_target_worktree(
    shell: &dyn RemoteShell,
    session: &TargetSession,
) -> Result<String, Status> {
    let worktree = session.worktree.as_str();
    let root = target_checkout_root_of(worktree).ok_or_else(|| {
        Status::invalid_argument(format!(
            "the target worktree {worktree} is not under a clone's .worktrees directory"
        ))
    })?;
    let changeset = tddy_core::read_changeset(&session.session_dir)
        .map_err(|e| Status::internal(format!("the session's changeset does not read: {e}")))?;
    let branch = changeset
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .ok_or_else(|| Status::failed_precondition("the session records no branch to restore"))?;

    let present = shell
        .exec(tddy_remote_shell::ExecRequest::new(format!(
            "test -d {}",
            quoted(worktree)
        )))
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    if present.status == 0 {
        return Ok(worktree.to_string());
    }

    run_on_target(shell, format!("git -C {} worktree prune", quoted(&root))).await?;
    let has_branch = shell
        .exec(tddy_remote_shell::ExecRequest::new(format!(
            "git -C {} rev-parse --verify --quiet {}",
            quoted(&root),
            quoted(&format!("refs/heads/{branch}"))
        )))
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .status
        == 0;
    let add = if has_branch {
        format!(
            "git -C {} worktree add {} {}",
            quoted(&root),
            quoted(worktree),
            quoted(branch)
        )
    } else {
        let base = changeset
            .effective_worktree_integration_base_ref
            .as_deref()
            .map(str::trim)
            .filter(|base| !base.is_empty())
            .ok_or_else(|| {
                Status::failed_precondition(
                    "the session's branch is gone on its target and no integration base is recorded",
                )
            })?;
        run_on_target(shell, format!("git -C {} fetch --all", quoted(&root))).await?;
        format!(
            "git -C {} worktree add -b {} {} {}",
            quoted(&root),
            quoted(branch),
            quoted(worktree),
            quoted(base)
        )
    };
    run_on_target(shell, add).await?;
    Ok(worktree.to_string())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_remote_shell::{ExecOutput, InMemoryShell};

    use super::{remove_target_worktree, target_checkout_root_of};

    const ROOT: &str = "/home/coder/.tddy/repos/qape-hq";
    const WORKTREE: &str = "/home/coder/.tddy/repos/qape-hq/.worktrees/01a1b2c3";

    #[test]
    fn a_worktrees_clone_is_two_directories_up() {
        assert_eq!(
            (
                target_checkout_root_of(WORKTREE),
                target_checkout_root_of("/home/coder/somewhere/else"),
            ),
            (Some(ROOT.to_string()), None)
        );
    }

    #[tokio::test]
    async fn removing_a_target_worktree_unregisters_it_from_the_clone() {
        // Given a target that answers every command with success
        let target = InMemoryShell::new("coder.remote-dev");
        target.answers_when(|_| Some(ExecOutput::succeeded(Vec::new())));

        // When
        remove_target_worktree(&target, WORKTREE)
            .await
            .expect("the removal succeeds");

        // Then git removes and prunes the worktree in its clone; nothing is merely `rm -rf`ed
        let run = target.commands_run().join("\n");
        assert!(
            run.contains(&format!(
                "git -C '{ROOT}' worktree remove --force '{WORKTREE}'"
            )) && run.contains(&format!("git -C '{ROOT}' worktree prune"))
                && !run.contains("rm -rf"),
            "the clone is told the worktree is gone:\n{run}"
        );
    }
}
