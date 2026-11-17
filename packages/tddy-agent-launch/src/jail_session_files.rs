use std::sync::Arc;

use tddy_session_agents::agent_roster;

use crate::svc_start_sandboxed_claude_cli_session::JailSession;

use tddy_session_agents::roster_replacement::roster_replacement_pairs;

use tddy_rpc::Status;

use crate::svc_start_sandboxed_claude_cli_session::JailDirs;

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
    // The context-file allow-list the jail is served: its agent's spec `context_globs`,
    // resolved by the caller from the session's type.
    globs: &[String],
    // The agent's guidance files: its spec's `instructions.files`.
    guidance_files: &[std::path::PathBuf],
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
    let glob_refs: Vec<&str> = globs.iter().map(String::as_str).collect();
    let ctx = tddy_daemon_sandbox::sandbox_session::prepare_context_dir_with_subagent(
        worktree_path,
        &replacements,
        &glob_refs,
        guidance_files,
    )
    .map_err(Status::internal)?;
    tddy_daemon_sandbox::sandbox_session::copy_dir_all(ctx.path(), context_dir)
        .map_err(Status::internal)?;
    Ok(())
}

/// What a jailed CLI session's first `.session.yaml` records beyond the jail's own.
pub(super) struct JailRecord<'a> {
    /// The CLI session type the jail runs.
    pub session_type: &'a str,
    pub model: &'a str,
    /// The effort level the start chose (blank for the agent's own default), recorded so a resume
    /// relaunches the jail at it.
    pub effort: &'a str,
    pub managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    pub started_agents: Vec<tddy_core::SessionAgentRecord>,
    pub pid: u32,
    pub managed_codebase: bool,
    pub session_role: &'a str,
    /// The conversation the daemon minted for the session, when the spec's conversation is minted.
    pub chat_id: Option<&'a str>,
    /// The per-session token the agent's hooks report with, when the jail's hooks were written.
    pub hook_token: Option<String>,
}

pub(super) fn write_jail_session_metadata(
    jail: &JailSession<'_>,
    record: JailRecord<'_>,
) -> Result<(), Status> {
    let JailSession {
        session_id,
        project_id,
        session_dir,
        worktree_path,
        ..
    } = *jail;
    let meta = tddy_core::SessionMetadata {
        // As on a host start: the agent runs, so the session reads `Started` from its own
        // first record rather than waiting for a hook that may never fire (Cursor sends
        // no `sessionStart` on its resumed launches).
        activity_status: Some(
            tddy_core::SessionActivityStatus::Started
                .as_wire()
                .to_string(),
        ),
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(record.pid),
        model: Some(record.model.to_string()),
        effort: Some(record.effort.trim().to_string()).filter(|effort| !effort.is_empty()),
        sandbox: Some(true),
        managed_codebase: record.managed_codebase,
        session_role: Some(record.session_role.to_string()).filter(|role| !role.is_empty()),
        recipe: record
            .managed_recipe
            .as_ref()
            .map(|recipe| recipe.name().to_string()),
        cursor_chat_id: record.chat_id.map(str::to_string),
        hook_token: record.hook_token,
        agents_rev: agent_roster::started_roster_rev(&record.started_agents),
        agents: record.started_agents,
        ..tddy_session_split::service_util::starting_session_metadata(
            session_id,
            project_id,
            record.session_type,
        )
    };
    tddy_core::write_session_metadata(session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// A jailed session's first `.session.yaml` records `Started`: its agent runs, and no
    /// hook has refined that yet (a Cursor launch never fires `sessionStart` at all).
    #[test]
    fn a_jailed_sessions_first_metadata_records_started() {
        // Given a jail session dir and its record
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("jailed-1");
        std::fs::create_dir_all(&session_dir).unwrap();
        let worktree = tmp.path().join("worktrees").join("jailed-1");
        std::fs::create_dir_all(&worktree).unwrap();
        let jail = JailSession {
            session_id: "jailed-1",
            project_id: "proj-1",
            session_token: "tok",
            session_dir: &session_dir,
            worktree_path: &worktree,
        };
        let record = JailRecord {
            session_type: "cursor-cli",
            model: "composer-2.5",
            effort: "",
            managed_recipe: None,
            started_agents: Vec::new(),
            pid: 4242,
            managed_codebase: false,
            session_role: "",
            chat_id: Some("chat-1"),
            hook_token: Some("tok-hook".to_string()),
        };

        // When the start writes the session's first metadata
        write_jail_session_metadata(&jail, record).expect("the metadata writes");

        // Then it already reads `Started`
        let meta = tddy_core::read_session_metadata(&session_dir).expect("it reads back");
        assert_eq!(meta.activity_status.as_deref(), Some("Started"));
    }
}
