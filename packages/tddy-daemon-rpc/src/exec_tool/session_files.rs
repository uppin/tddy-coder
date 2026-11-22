//! `WriteSessionFile` and `ListSessionFiles` answered on this host (`#orchestra` 3/9).
//!
//! Served to orchestrator roles alone, routed from `run_locally` ahead of the local runner as the
//! message tools are. The write goes into the calling session's own directory, resolved from the
//! session the exec-tool path authenticated, and nowhere else.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_agent_launch::conversation_spawn::session_tool_role;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_files::session_files::{SessionFile, SessionFileStore};
use tddy_tool_engine::project_home_tools::SessionToolRole;
use tddy_tool_engine::session_file_tools::{
    execute_session_file_tool, SessionFileEntry, SessionFilePort, WriteSessionFileArgs,
};
use tddy_worktree_service::session_target::HomeCaller;

use super::ExecToolRpcHandler;

/// [`SessionFilePort`] over one session directory of this host.
struct DaemonSessionFiles {
    sessions_base: PathBuf,
}

impl DaemonSessionFiles {
    fn store_of(&self, caller: &HomeCaller) -> SessionFileStore {
        SessionFileStore::open(&tddy_core::session_lifecycle::unified_session_dir_path(
            &self.sessions_base,
            &caller.session_id,
        ))
    }
}

fn entry_of(file: SessionFile) -> SessionFileEntry {
    SessionFileEntry {
        name: file.name,
        size_bytes: file.size_bytes,
        modified_unix_ms: file.modified_unix_ms,
    }
}

#[async_trait]
impl SessionFilePort for DaemonSessionFiles {
    async fn write_session_file(
        &self,
        caller: &HomeCaller,
        args: WriteSessionFileArgs,
    ) -> Result<SessionFileEntry, String> {
        self.store_of(caller)
            .write(&args.name, &args.content)
            .map(entry_of)
            .map_err(|e| e.to_string())
    }

    async fn list_session_files(
        &self,
        caller: &HomeCaller,
    ) -> Result<Vec<SessionFileEntry>, String> {
        self.store_of(caller)
            .list()
            .map(|files| files.into_iter().map(entry_of).collect())
            .map_err(|e| format!("session files could not be listed: {e}"))
    }

    fn holding_daemon(&self) -> String {
        // TODO(session-files-cross-host): implement — this daemon's instance id.
        unimplemented!("DaemonSessionFiles::holding_daemon")
    }
}

impl ExecToolRpcHandler {
    /// Serve `req`'s session-file tool for its caller, an orchestrator session of `sessions_base`.
    pub(super) async fn run_session_file_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let caller = orchestrator_caller(sessions_base, &req.session_id, &req.tool_name)?;
        let port = DaemonSessionFiles {
            sessions_base: sessions_base.to_path_buf(),
        };
        let outcome =
            execute_session_file_tool(&port, &caller, &req.tool_name, &req.args_json).await;
        Ok(ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        })
    }
}

/// The calling session and its project, from its own record; refused unless it is an orchestrator.
fn orchestrator_caller(
    sessions_base: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<HomeCaller, Status> {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
        Status::permission_denied(format!(
            "tool {tool_name} is refused: cannot read session {session_id}'s role ({e})"
        ))
    })?;
    if session_tool_role(&metadata) != SessionToolRole::ProjectHome {
        return Err(Status::permission_denied(format!(
            "tool {tool_name} is served only to an orchestrator session"
        )));
    }
    Ok(HomeCaller {
        session_id: session_id.to_string(),
        project_id: metadata.project_id,
    })
}
