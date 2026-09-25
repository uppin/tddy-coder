use super::super::roster_replacement_pairs;

use tddy_rpc::Status;

use std::path::PathBuf;

use std::path::Path;

pub(super) fn prepare_relaunch_dirs(
    session_dir: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, PathBuf), Status> {
    let sandbox_root = session_dir.join("sandbox");
    let egress_dir = session_dir.join("egress");
    std::fs::create_dir_all(sandbox_root.join(".work").join("home"))
        .map_err(|e| Status::internal(format!("mkdir sandbox scratch: {e}")))?;
    std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
        .map_err(|e| Status::internal(format!("mkdir sandbox tmp: {e}")))?;
    std::fs::create_dir_all(sandbox_root.join("context"))
        .map_err(|e| Status::internal(format!("mkdir sandbox context: {e}")))?;
    std::fs::create_dir_all(&egress_dir)
        .map_err(|e| Status::internal(format!("mkdir sandbox egress: {e}")))?;
    let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
    let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
    let scratch_dir = sandbox_root.join(".work");
    // scratch_home (jail $HOME) is the persistent daemon-wide claude home, resolved and mounted
    // below — not a per-session dir — so auth/history persist across sessions.
    let scratch_tmp = scratch_dir.join("tmp");
    let context_dir = sandbox_root.join("context");
    Ok((
        sandbox_root,
        egress_dir,
        scratch_dir,
        scratch_tmp,
        context_dir,
    ))
}

pub(super) fn refresh_relaunch_context_dir(
    worktree_path: &Path,
    agents: &[tddy_core::SessionAgentRecord],
    context_dir: &PathBuf,
) -> Result<(), Status> {
    let replacement_pairs = roster_replacement_pairs(agents);
    let replacement_refs: Vec<Vec<&str>> = replacement_pairs
        .iter()
        .map(|(_, tools)| tools.iter().map(String::as_str).collect())
        .collect();
    let replacements: Vec<tddy_sandbox::SubagentReplacement<'_>> = replacement_pairs
        .iter()
        .zip(replacement_refs.iter())
        .map(|((name, _), refs)| tddy_sandbox::SubagentReplacement {
            name,
            replaced: refs,
        })
        .collect();
    let ctx = tddy_daemon_sandbox::sandbox_session::prepare_context_dir_with_subagent(
        worktree_path,
        &replacements,
        // The relaunch path serves `claude-cli` alone (`resume_sandboxed_claude_cli_session` is
        // its only caller), so the agent's allow-list is that backend's.
        crate::context_files::context_globs_for_session_type("claude-cli"),
    )
    .map_err(|e| Status::internal(format!("prepare context dir: {e}")))?;
    if context_dir.exists() {
        std::fs::remove_dir_all(context_dir)
            .map_err(|e| Status::internal(format!("clear context dir: {e}")))?;
    }
    std::fs::create_dir_all(context_dir)
        .map_err(|e| Status::internal(format!("mkdir context dir: {e}")))?;
    tddy_daemon_sandbox::sandbox_session::copy_dir_all(ctx.path(), context_dir)
        .map_err(|e| Status::internal(format!("copy context dir: {e}")))?;
    Ok(())
}
