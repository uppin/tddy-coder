//! An agent's slot files and its session pointer, written on the target its worktree lives on
//! through the remote-shell port ([`tddy_remote_shell::RemoteShell`]).
//!
//! The port bounds every call and says why one failed, so nothing here names a transport: the same
//! two functions serve an ssh target and any transport added later.

use std::path::{Path, PathBuf};

use tddy_core::session_pointer::SessionPointer;
use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;

/// Write each of `files` (a rendered launch's slot files: path on the target, bytes) through
/// `shell`, creating its directory, with the target's own default permissions. Refused
/// `DEADLINE_EXCEEDED` naming the target when it does not answer in time.
pub async fn write_slot_files_through(
    shell: &dyn RemoteShell,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<(), Status> {
    // TODO(shell-port): implement
    let _ = (shell, files);
    Err(Status::unimplemented(
        "slot files are not written through the remote-shell port yet",
    ))
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
    // TODO(shell-port): implement
    let _ = (shell, pointer, worktree);
    Err(Status::unimplemented(
        "the session pointer is not placed through the remote-shell port yet",
    ))
}
