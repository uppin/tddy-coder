//! An agent's slot files and its session pointer, written on the target its worktree lives on
//! through the remote-shell port ([`tddy_remote_shell::RemoteShell`]).
//!
//! The port bounds every call and says why one failed, so nothing here names a transport: the same
//! two functions serve an ssh target and any transport added later.

use std::path::{Path, PathBuf};

use tddy_core::session_pointer::{SessionPointer, SESSION_POINTER_FILE};
use tddy_core::ssh_exec::shell_single_quote;
use tddy_remote_shell::{ExecRequest, FileMode, RemoteShell, RemoteShellError};
use tddy_rpc::Status;

/// Write each of `files` (a rendered launch's slot files: path on the target, bytes) through
/// `shell`, creating its directory, with the target's own default permissions. Refused
/// `DEADLINE_EXCEEDED` naming the target when it does not answer in time.
pub async fn write_slot_files_through(
    shell: &dyn RemoteShell,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<(), Status> {
    for (path, bytes) in files {
        shell
            .write_file(&path.to_string_lossy(), bytes, FileMode::TargetDefault)
            .await
            .map_err(|error| match error {
                RemoteShellError::TimedOut { .. } => Status::deadline_exceeded(error.to_string()),
                other => Status::internal(other.to_string()),
            })?;
    }
    Ok(())
}

/// Place `pointer` as `worktree`'s session pointer on `shell`'s target: the pointer is added to
/// the checkout's `info/exclude` unless it is there (one command through the port; a `worktree`
/// that is no checkout has nothing to exclude it from), then written owner-only through the port.
/// Returns where the pointer is on the target.
pub async fn place_session_pointer_through(
    shell: &dyn RemoteShell,
    pointer: &SessionPointer,
    worktree: &Path,
) -> Result<PathBuf, Status> {
    let exclude_command = format!(
        "if exclude=$(git -C {worktree} rev-parse --path-format=absolute --git-path info/exclude 2>/dev/null); then \
         mkdir -p \"$(dirname \"$exclude\")\"; \
         grep -qxF '/.tddy/session.json' \"$exclude\" 2>/dev/null || echo '/.tddy/session.json' >> \"$exclude\"; fi",
        worktree = shell_single_quote(&worktree.to_string_lossy()),
    );
    shell
        .exec(ExecRequest::new(exclude_command))
        .await
        .map_err(|error| match error {
            RemoteShellError::TimedOut { .. } => Status::deadline_exceeded(error.to_string()),
            other => Status::internal(other.to_string()),
        })?
        .success_or_exited()
        .map_err(|error| match error {
            RemoteShellError::TimedOut { .. } => Status::deadline_exceeded(error.to_string()),
            other => Status::internal(other.to_string()),
        })?;
    let pointer_path = worktree.join(SESSION_POINTER_FILE);
    let bytes =
        serde_json::to_vec_pretty(pointer).map_err(|error| Status::internal(error.to_string()))?;
    shell
        .write_file(&pointer_path.to_string_lossy(), &bytes, FileMode::OwnerOnly)
        .await
        .map_err(|error| match error {
            RemoteShellError::TimedOut { .. } => Status::deadline_exceeded(error.to_string()),
            other => Status::internal(other.to_string()),
        })?;
    Ok(pointer_path)
}
