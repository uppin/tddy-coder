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
//! Feature: docs/ft/web/session-worktree-inspector.md § On a target session.

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
    target_session_from(
        session_dir,
        tddy_core::read_session_metadata(session_dir).ok()?,
    )
}

pub(crate) fn target_session_from(
    session_dir: &Path,
    metadata: tddy_core::SessionMetadata,
) -> Option<TargetSession> {
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
    fs: &dyn tddy_fs::Fs,
    sessions_dir: &Path,
    project_id: &str,
    worktree: &str,
) -> Option<TargetSession> {
    // `metadata_follow`, as before: `is_dir` follows a link.
    fs.read_dir(sessions_dir)
        .ok()?
        .into_iter()
        .filter(|dir| {
            matches!(
                fs.metadata_follow(dir),
                Ok(m) if m.kind == tddy_fs::FsKind::Dir
            )
        })
        .filter_map(|dir| {
            let metadata = tddy_core::read_session_metadata(&dir).ok()?;
            (metadata.project_id == project_id)
                .then(|| target_session_from(&dir, metadata))
                .flatten()
        })
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

/// [`target_checkout_root_of`], refusing a path outside a clone's `.worktrees` directory.
fn checkout_root_of(worktree: &str) -> Result<String, Status> {
    target_checkout_root_of(worktree).ok_or_else(|| {
        Status::invalid_argument(format!(
            "the target worktree {worktree} is not under a clone's .worktrees directory"
        ))
    })
}

fn quoted(text: &str) -> String {
    tddy_core::ssh_exec::shell_single_quote(text)
}

/// Whether `command` exits zero on the target. Only a failure to run it at all is an error.
async fn succeeds_on_target(shell: &dyn RemoteShell, command: String) -> Result<bool, Status> {
    let output = shell
        .exec(tddy_remote_shell::ExecRequest::new(command))
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(output.status == 0)
}

/// Run `command` on the target and answer its stdout; a non-zero exit is an `internal` error
/// carrying its output.
pub(crate) async fn run_on_target(
    shell: &dyn RemoteShell,
    command: String,
) -> Result<Vec<u8>, Status> {
    let output = shell
        .exec(tddy_remote_shell::ExecRequest::new(command))
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .success_or_exited()
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(output.stdout)
}

/// Whether `path` is on the target, through `shell`. Absence is answered only when it can be told:
/// the nearest ancestor of `path` that is there is a directory this account can search. A path
/// whose ancestor cannot be searched — `test -e` failing on a permission, not on absence — is the
/// error, never read as gone.
pub async fn target_path_is_present(shell: &dyn RemoteShell, path: &str) -> Result<bool, Status> {
    let probe = format!(
        "p={}; if [ -e \"$p\" ]; then echo present; exit 0; fi; d=$(dirname -- \"$p\"); \
         while [ ! -e \"$d\" ]; do d=$(dirname -- \"$d\"); done; \
         if [ -d \"$d\" ] && [ -x \"$d\" ]; then echo absent; \
         else echo \"$d cannot be searched, so whether $p is there cannot be told\" >&2; exit 3; fi",
        quoted(path)
    );
    let answer = run_on_target(shell, probe).await?;
    match String::from_utf8_lossy(&answer).trim() {
        "present" => Ok(true),
        "absent" => Ok(false),
        other => Err(Status::internal(format!(
            "whether {path} is on the target cannot be told: the probe answered {other:?}"
        ))),
    }
}

/// Remove a target session's worktree on its target, through `shell`: the worktree, the pointer
/// inside it, and the clone's registration of it (`git worktree remove`, then `prune`), so the
/// clone does not keep a dangling entry. Then checked: a worktree still on the target, or still
/// listed by its clone, fails the removal.
///
/// A worktree already gone counts as removed: one removed before leaves nothing to do, and one
/// whose directory was deleted outside git leaves only its registration, which the prune clears.
/// `git worktree remove` runs only while the directory is there, since git refuses a path it does
/// not list. A worktree whose presence cannot be told ([`target_path_is_present`]) is refused.
pub async fn remove_target_worktree(shell: &dyn RemoteShell, worktree: &str) -> Result<(), Status> {
    let root = checkout_root_of(worktree)?;
    if target_path_is_present(shell, worktree).await? {
        run_on_target(
            shell,
            format!(
                "git -C {} worktree remove --force {}",
                quoted(&root),
                quoted(worktree)
            ),
        )
        .await?;
    }
    run_on_target(shell, format!("git -C {} worktree prune", quoted(&root))).await?;
    target_worktree_is_gone(shell, &root, worktree).await
}

/// The post-condition of a target removal: `worktree` is neither on the target nor among the
/// worktrees of its clone at `root`.
async fn target_worktree_is_gone(
    shell: &dyn RemoteShell,
    root: &str,
    worktree: &str,
) -> Result<(), Status> {
    if target_path_is_present(shell, worktree).await? {
        return Err(Status::internal(format!(
            "{worktree} is still on the target after its removal"
        )));
    }
    let listed = run_on_target(
        shell,
        format!("git -C {} worktree list --porcelain", quoted(root)),
    )
    .await?;
    let still_listed = String::from_utf8_lossy(&listed)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|path| path.trim_end_matches('/') == worktree.trim_end_matches('/'));
    if still_listed {
        return Err(Status::internal(format!(
            "the target's clone still lists {worktree} among its worktrees after its removal"
        )));
    }
    Ok(())
}

/// Clean a target session's worktree on its target: untracked and ignored files go.
pub async fn clean_target_worktree(shell: &dyn RemoteShell, worktree: &str) -> Result<(), Status> {
    run_on_target(shell, format!("git -C {} clean -fdx", quoted(worktree))).await?;
    Ok(())
}

/// A recorded, non-blank changeset field.
fn recorded(field: Option<&str>) -> Option<&str> {
    field.map(str::trim).filter(|value| !value.is_empty())
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
    let root = checkout_root_of(worktree)?;
    let changeset = tddy_core::read_changeset(&session.session_dir)
        .map_err(|e| Status::internal(format!("the session's changeset does not read: {e}")))?;
    let branch = recorded(changeset.branch.as_deref())
        .ok_or_else(|| Status::failed_precondition("the session records no branch to restore"))?;

    if succeeds_on_target(shell, format!("test -d {}", quoted(worktree))).await? {
        return Ok(worktree.to_string());
    }
    run_on_target(shell, format!("git -C {} worktree prune", quoted(&root))).await?;
    let base = recorded(changeset.effective_worktree_integration_base_ref.as_deref());
    let add = worktree_add_command(shell, &root, worktree, branch, base).await?;
    run_on_target(shell, add).await?;
    Ok(worktree.to_string())
}

/// The `git worktree add` that re-creates `worktree` on `branch`: checked out again when the clone
/// still has the branch, otherwise cut from `base` after a fetch.
async fn worktree_add_command(
    shell: &dyn RemoteShell,
    root: &str,
    worktree: &str,
    branch: &str,
    base: Option<&str>,
) -> Result<String, Status> {
    let has_branch = format!(
        "git -C {} rev-parse --verify --quiet {}",
        quoted(root),
        quoted(&format!("refs/heads/{branch}"))
    );
    if succeeds_on_target(shell, has_branch).await? {
        return Ok(format!(
            "git -C {} worktree add {} {}",
            quoted(root),
            quoted(worktree),
            quoted(branch)
        ));
    }
    let base = base.ok_or_else(|| {
        Status::failed_precondition(
            "the session's branch is gone on its target and no integration base is recorded",
        )
    })?;
    run_on_target(shell, format!("git -C {} fetch --all", quoted(root))).await?;
    Ok(format!(
        "git -C {} worktree add -b {} {} {}",
        quoted(root),
        quoted(branch),
        quoted(worktree),
        quoted(base)
    ))
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
        // Given a target holding the worktree until git removes it, and answering every other
        // command with success
        let target = InMemoryShell::new("coder.remote-dev");
        let removed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = std::sync::Arc::clone(&removed);
        target.answers_when(move |command| {
            use std::sync::atomic::Ordering;
            if command.command.contains("worktree remove --force") {
                seen.store(true, Ordering::SeqCst);
            }
            let answer = if !command.command.contains("echo present") {
                ""
            } else if seen.load(Ordering::SeqCst) {
                "absent\n"
            } else {
                "present\n"
            };
            Some(ExecOutput::succeeded(answer.as_bytes().to_vec()))
        });

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
