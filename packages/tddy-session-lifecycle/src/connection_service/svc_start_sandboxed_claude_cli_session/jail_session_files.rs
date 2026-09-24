use std::sync::Arc;

use crate::connection_service::agent_roster;

use super::JailSession;

use super::super::roster_replacement_pairs;

use tddy_rpc::Status;

use super::JailDirs;

use std::path::Path;

pub(super) fn prepare_jail_dirs(session_dir: &Path) -> Result<JailDirs, Status> {
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

    // Resolve to the real (symlink-free) paths now that the dirs exist. Seatbelt
    // evaluates file rules — including AF_UNIX socket bind — against the fully
    // resolved path, so the socket/marker paths the runner binds must match the
    // canonical paths baked into the SBPL profile. Session dirs live under TMPDIR,
    // which on macOS is reached via the /tmp -> /private/tmp symlink; without this
    // the tool-IPC socket bind fails with "Operation not permitted".
    let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
    let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
    let scratch_dir = sandbox_root.join(".work");
    // scratch_home (jail $HOME) is the persistent daemon-wide claude home, resolved and mounted
    // below — not a per-session dir — so auth/history persist across sessions.
    let scratch_tmp = scratch_dir.join("tmp");
    let context_dir = sandbox_root.join("context");
    Ok(JailDirs {
        sandbox_root,
        egress_dir,
        scratch_dir,
        scratch_tmp,
        context_dir,
    })
}

pub(super) fn prepare_jail_context_dir(
    started_agents: &[tddy_core::SessionAgentRecord],
    worktree_path: &Path,
    context_dir: &Path,
) -> Result<(), Status> {
    let replacement_pairs = roster_replacement_pairs(started_agents);
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
        crate::context_files::context_globs_for_session_type("claude-cli"),
    )
    .map_err(Status::internal)?;
    tddy_daemon_sandbox::sandbox_session::copy_dir_all(ctx.path(), context_dir)
        .map_err(Status::internal)?;
    Ok(())
}

pub(super) fn write_jail_session_metadata(
    jail: &JailSession<'_>,
    model: &str,
    managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    started_agents: Vec<tddy_core::SessionAgentRecord>,
    pid: u32,
) -> Result<(), Status> {
    let JailSession {
        session_id,
        project_id,
        session_dir,
        worktree_path,
    } = *jail;
    let meta = tddy_core::SessionMetadata {
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(pid),
        model: Some(model.to_string()),
        sandbox: Some(true),
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        agents_rev: agent_roster::started_roster_rev(&started_agents),
        agents: started_agents,
        ..crate::connection_service::starting_session_metadata(session_id, project_id, "claude-cli")
    };
    tddy_core::write_session_metadata(session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
    Ok(())
}
