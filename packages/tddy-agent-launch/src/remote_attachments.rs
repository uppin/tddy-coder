//! A target session's attachments, copied where its agent can read them: its worktree on the
//! target, under [`TARGET_ATTACHMENTS_DIR`] — the path its first prompt names them by
//! (`tddy_session_files::attachment_prompt`). The daemon materializes them into the session
//! directory on its own host, which an agent on a target cannot reach.

use std::path::Path;

use tddy_remote_shell::{ExecRequest, FileMode, RemoteShell};
use tddy_rpc::Status;
use tddy_session_files::attachment_prompt::TARGET_ATTACHMENTS_DIR;

use crate::remote_hook_follower::shell_quote;

/// Copy every attachment of the session in `session_dir` into `worktree` on `shell`'s target, under
/// [`TARGET_ATTACHMENTS_DIR`], readable by its owner alone and kept out of the checkout's git status
/// (`info/exclude`, as the hook log is). A session with no attachment copies nothing.
pub(crate) async fn copy_attachments_to_target(fs: &dyn tddy_fs::Fs,
    shell: &dyn RemoteShell,
    session_dir: &Path,
    worktree: &str,
) -> Result<(), Status> {
    let attachments = tddy_workflow::list_session_attachments(session_dir);
    if attachments.is_empty() {
        return Ok(());
    }
    let exclude_entry = format!("/{TARGET_ATTACHMENTS_DIR}/");
    shell
        .exec(ExecRequest::new(format!(
            "if exclude=$(git -C {worktree} rev-parse --path-format=absolute --git-path info/exclude 2>/dev/null); then \
             mkdir -p \"$(dirname \"$exclude\")\"; \
             grep -qxF {entry} \"$exclude\" 2>/dev/null || echo {entry} >> \"$exclude\"; fi",
            worktree = shell_quote(worktree),
            entry = shell_quote(&exclude_entry),
        )))
        .await
        .and_then(|output| output.success_or_exited())
        .map_err(|e| Status::internal(format!("excluding the attachments from git failed: {e}")))?;
    let directory = Path::new(worktree).join(TARGET_ATTACHMENTS_DIR);
    for attachment in attachments {
        let bytes = fs.read(&attachment.path).map_err(|e| {
            Status::internal(format!(
                "reading attachment {} failed: {e}",
                attachment.path.display()
            ))
        })?;
        shell
            .write_file(
                &directory.join(&attachment.basename).to_string_lossy(),
                &bytes,
                FileMode::OwnerOnly,
            )
            .await
            .map_err(|e| {
                Status::internal(format!(
                    "copying attachment {} to the target failed: {e}",
                    attachment.basename
                ))
            })?;
    }
    Ok(())
}
