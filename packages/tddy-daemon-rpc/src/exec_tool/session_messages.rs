//! `PostSessionMessage` and `ReadSessionMessages` answered on this host, and the inbox watch a
//! session that is not an orchestrator may hold (`#activity-wake` 5/5).
//!
//! Served to every session role, routed from `run_locally` ahead of the local runner as the stack
//! tools are. The caller is the session the exec-tool path authenticated; its project, its
//! orchestrator and the project's main session are read from the sessions' own records.
//!
//! - **Post.** The addressing rule ([`tddy_tool_engine::session_message_tools`]) decides whether
//!   the caller may address the recipient; the message is then published on the host's
//!   notification bus as a `message` notification — the log records it, the waker and the long
//!   polls wake on it, and the web stream, Telegram and the desktop skip it.
//! - **Read** reads through the caller's inbox position
//!   ([`SessionWatchStore::read_messages`]), a reference into the log.
//! - **The inbox watch.** The watch tools are an orchestrator's, but any session may hold one watch:
//!   its inbox ([`tddy_session_activity::session_watches::WatchFilter::inbox_of`]) — asked for as
//!   `CreateSessionWatch` with the kind `message` alone, and streamed by
//!   `tddy-tools monitor --kinds message` through `WaitSessionWatchEvents`. A watch-tool call from
//!   such a session is served here ([`ExecToolRpcHandler::run_inbox_watch_tool`]) rather than
//!   refused by the project-home role gate; any other watch it asks for is refused.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_notifications::SessionNotificationBus;
use tddy_session_activity::session_watches::SessionWatchStore;
use tddy_tool_engine::project_home_tools::SessionToolRole;
use tddy_tool_engine::project_home_watch_stream_tools::is_project_home_watch_stream_tool;
use tddy_tool_engine::project_home_watch_tools::is_project_home_watch_tool;
use tddy_tool_engine::session_message_tools::{
    execute_session_message_tool, PostMessageRequest, ReadMessagesRequest, SessionMessagePort,
};
use tddy_worktree_service::session_target::HomeCaller;

use super::ExecToolRpcHandler;

/// [`SessionMessagePort`] over this host, for one caller.
pub(super) struct DaemonSessionMessages {
    /// The OS user the caller's sessions belong to: the only user its messages reach.
    os_user: String,
    /// Where the caller's and its recipients' records are read.
    sessions_base: PathBuf,
    /// The bus a posted message is published on; `None` on a host with none.
    bus: Option<Arc<SessionNotificationBus>>,
    /// The log the inboxes read; `None` on a host with none.
    notification_log: Option<Arc<ProjectNotificationLog>>,
    /// The store the inboxes live in, beside the watches; `None` on a host with none.
    session_watches: Option<Arc<SessionWatchStore>>,
}

#[async_trait]
impl SessionMessagePort for DaemonSessionMessages {
    async fn post_message(
        &self,
        caller: &HomeCaller,
        request: PostMessageRequest,
    ) -> Result<Value, String> {
        // TODO(session-messages): implement — refuse a bad text, apply the addressing rule over the
        // sessions' records, publish the `message` notification, answer its id.
        let _ = (
            caller,
            request,
            &self.bus,
            &self.sessions_base,
            &self.os_user,
        );
        Err(
            "TODO(session-messages): PostSessionMessage is not implemented on this host yet"
                .to_string(),
        )
    }

    async fn read_messages(
        &self,
        caller: &HomeCaller,
        request: ReadMessagesRequest,
    ) -> Result<Value, String> {
        // TODO(session-messages): implement — read the caller's inbox and acknowledge it.
        let _ = (
            caller,
            request,
            &self.notification_log,
            &self.session_watches,
        );
        Err(
            "TODO(session-messages): ReadSessionMessages is not implemented on this host yet"
                .to_string(),
        )
    }
}

impl ExecToolRpcHandler {
    /// Serve `req`'s message tool for its caller, a session of `sessions_base` of any role.
    pub(super) async fn run_session_message_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let caller = message_caller(sessions_base, &req.session_id, &req.tool_name)?;
        let port = DaemonSessionMessages {
            os_user: self.os_user_of(&req.session_token)?,
            sessions_base: sessions_base.to_path_buf(),
            bus: self.project_home_host.notification_bus.clone(),
            notification_log: self.project_home_host.notification_log.clone(),
            session_watches: self.project_home_host.session_watches.clone(),
        };
        let outcome =
            execute_session_message_tool(&port, &caller, &req.tool_name, &req.args_json).await;
        Ok(ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        })
    }
}

impl ExecToolRpcHandler {
    /// Whether `req` is a watch-tool call (`CreateSessionWatch`, `ListSessionWatches`,
    /// `RenewSessionWatch`, `CancelSessionWatch`, `WaitSessionWatchEvents`) from a session that is
    /// not an orchestrator: one [`Self::run_inbox_watch_tool`] serves. A caller whose record cannot
    /// be read is not: the project-home role gate refuses it as before.
    pub(super) fn is_inbox_watch_call(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> bool {
        if !is_project_home_watch_tool(&req.tool_name)
            && !is_project_home_watch_stream_tool(&req.tool_name)
        {
            return false;
        }
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, &req.session_id);
        tddy_core::read_session_metadata(&session_dir).is_ok_and(|metadata| {
            tddy_agent_launch::conversation_spawn::session_tool_role(&metadata)
                != SessionToolRole::ProjectHome
        })
    }

    /// Serve a watch-tool call from a session that is not an orchestrator: `CreateSessionWatch`
    /// for its inbox watch alone, and the other watch tools over its own watches — which can only
    /// be that one. Anything else is denied (`PermissionDenied`), as the project-home role gate
    /// denies a watch tool to such a session.
    pub(super) async fn run_inbox_watch_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        // TODO(session-messages): implement — create the inbox watch (`CreateSessionWatch` with the
        // kind `message` alone) as `WatchFilter::inbox_of` the caller, and serve the other watch
        // tools over the caller's own watches through the project-home watch port. Anything else
        // stays denied, as the project-home role gate denies it.
        let _ = sessions_base;
        Err(Status::permission_denied(format!(
            "tool {} is served only to a project-home session — TODO(session-messages): a session's own message watch is not implemented yet",
            req.tool_name
        )))
    }
}

/// The calling session and its project, from its own record. A session whose record cannot be read
/// is refused: its project cannot be known.
fn message_caller(
    sessions_base: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<HomeCaller, Status> {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
        Status::permission_denied(format!(
            "tool {tool_name} is refused: cannot read session {session_id}'s record ({e})"
        ))
    })?;
    Ok(HomeCaller {
        session_id: session_id.to_string(),
        project_id: metadata.project_id,
    })
}
