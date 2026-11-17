//! `PostSessionMessage` and `ReadSessionMessages` answered on this host (`#activity-wake` 5/5).
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
//! - **The inbox watch** a session that is not an orchestrator may hold is the child module
//!   [`inbox_watch`]: the watch tools are an orchestrator's, but a watch-tool call from such a
//!   session is served there rather than refused by the project-home role gate — `CreateSessionWatch`
//!   with the kind `message` alone creates only its inbox watch
//!   ([`tddy_session_activity::session_watches::WatchFilter::inbox_of`]), streamed by
//!   `tddy-tools monitor --kinds message`; any other watch it asks for is refused.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_agent_launch::conversation_spawn::session_tool_role;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_notification_publishing::resolve_session_label;
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::session_watches::{SessionWatchStore, WatchOwner};
use tddy_tool_engine::project_home_inbox_tools::NO_NOTIFICATION_LOG;
use tddy_tool_engine::project_home_tools::SessionToolRole;
use tddy_tool_engine::project_home_watch_tools::NO_SESSION_WATCHES;
use tddy_tool_engine::session_message_tools::{
    execute_session_message_tool, Addressable, MessageRefusal, PostMessageRequest,
    ReadMessagesRequest, SessionMessagePort, NO_SESSION_MESSAGES,
};
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_target::HomeCaller;

use super::ExecToolRpcHandler;

mod inbox_watch;

/// A served answer, as [`execute_session_message_tool`] answers the message tools — the
/// constructors are `tddy-tool-engine`'s, and these mirror them over its public fields.
fn outcome_ok(result_json: String) -> ToolOutcome {
    ToolOutcome {
        result_json,
        is_error: false,
        error_message: String::new(),
        job_id: String::new(),
        job_running: false,
    }
}

/// A refused call, as a tool refusal is answered: `is_error` with the reason, here and in the
/// result's `"error"` key.
fn outcome_err(message: impl Into<String>) -> ToolOutcome {
    let message = message.into();
    ToolOutcome {
        result_json: json!({ "error": message }).to_string(),
        is_error: true,
        error_message: message,
        job_id: String::new(),
        job_running: false,
    }
}

/// [`SessionMessagePort`] over this host, for one caller.
pub(super) struct DaemonSessionMessages {
    /// The OS user the caller's sessions belong to: the only user its messages reach.
    os_user: String,
    /// Where the caller's and its recipients' records are read.
    sessions_base: PathBuf,
    /// Where the projects' registry is read, for the project's main session.
    tddy_data_dir: PathBuf,
    /// The bus a posted message is published on; `None` on a host with none.
    bus: Option<Arc<SessionNotificationBus>>,
    /// The log the inboxes read; `None` on a host with none.
    notification_log: Option<Arc<ProjectNotificationLog>>,
    /// The store the inboxes live in, beside the watches; `None` on a host with none.
    session_watches: Option<Arc<SessionWatchStore>>,
}

/// The watches and log this host keeps, or the refusal a tool without them is answered.
fn inbox_parts<'a>(
    store: &'a Option<Arc<SessionWatchStore>>,
    log: &'a Option<Arc<ProjectNotificationLog>>,
) -> Result<(&'a SessionWatchStore, &'a ProjectNotificationLog), String> {
    let store = store
        .as_deref()
        .ok_or_else(|| NO_SESSION_WATCHES.to_string())?;
    let log = log
        .as_deref()
        .ok_or_else(|| NO_NOTIFICATION_LOG.to_string())?;
    Ok((store, log))
}

#[async_trait]
impl SessionMessagePort for DaemonSessionMessages {
    async fn post_message(
        &self,
        caller: &HomeCaller,
        request: PostMessageRequest,
    ) -> Result<Value, String> {
        tddy_session_activity::session_messages::refuse_a_bad_message_text(&request.text)?;
        let bus = self
            .bus
            .as_ref()
            .ok_or_else(|| NO_SESSION_MESSAGES.to_string())?;
        let (watches, log) = inbox_parts(&self.session_watches, &self.notification_log)?;

        // Who the caller is, from its own records: its role, and the orchestrator that started it.
        let caller_dir = tddy_core::session_lifecycle::unified_session_dir_path(
            &self.sessions_base,
            &caller.session_id,
        );
        let caller_metadata = tddy_core::read_session_metadata(&caller_dir)
            .map_err(|e| format!("cannot read session {}'s record: {e}", caller.session_id))?;
        let orchestrator = tddy_core::read_changeset(&caller_dir)
            .ok()
            .and_then(|changeset| changeset.orchestrator_session_id);
        let main_session = projects_path_for_user(&self.os_user, Some(&self.tddy_data_dir))
            .and_then(|projects_dir| {
                tddy_projects::project_storage::find_project(&projects_dir, &caller.project_id).ok()
            })
            .flatten()
            .and_then(|project| project.home_session_id)
            .filter(|home| home != &caller.session_id);
        let addressable = Addressable {
            any_visible_session: session_tool_role(&caller_metadata)
                == SessionToolRole::ProjectHome,
            orchestrator_session_id: orchestrator.clone(),
            main_session_id: main_session.clone(),
        };
        let not_addressable = |to_session_id: &str| {
            MessageRefusal::NotAddressable {
                to_session_id: to_session_id.to_string(),
                addressable: addressable.clone(),
            }
            .to_string()
        };

        // The addressing rule: one project, one OS user, never oneself, and one of the caller's
        // own reaches.
        if request.to_session_id == caller.session_id {
            return Err(MessageRefusal::Oneself { addressable }.to_string());
        }
        let recipient_metadata = tddy_core::read_session_metadata(
            &tddy_core::session_lifecycle::unified_session_dir_path(
                &self.sessions_base,
                &request.to_session_id,
            ),
        )
        .map_err(|_| not_addressable(&request.to_session_id))?;
        if recipient_metadata.project_id != caller.project_id {
            return Err(not_addressable(&request.to_session_id));
        }
        let reaches = |to_session_id: &str| {
            addressable.any_visible_session
                || orchestrator.as_deref() == Some(to_session_id)
                || main_session.as_deref() == Some(to_session_id)
        };
        if let Some(reply_to) = &request.reply_to {
            let received = watches
                .received_message(&owner_of(caller, &self.os_user), reply_to, log)
                .ok_or_else(|| {
                    MessageRefusal::NotReceived {
                        message_id: reply_to.clone(),
                    }
                    .to_string()
                })?;
            if received.from_session_id != request.to_session_id && !reaches(&request.to_session_id)
            {
                return Err(not_addressable(&request.to_session_id));
            }
        } else if !reaches(&request.to_session_id) {
            return Err(not_addressable(&request.to_session_id));
        }

        // The message is published on the bus: the log records it, the waker and the long polls
        // wake on it, and the surfaces that skip the source never show it.
        let message_id = format!("m-{}", uuid::Uuid::new_v4());
        let sent_at_unix_ms = tddy_daemon_kernel::now_unix_ms();
        bus.publish(SessionNotification {
            session_id: caller.session_id.clone(),
            os_user: self.os_user.clone(),
            label: resolve_session_label(None, &self.sessions_base, &caller.session_id),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::Message,
            text: request.text.clone(),
            agent: None,
            question: None,
            ended: None,
            activity_status: None,
            message: Some(tddy_daemon_kernel::session_notification::SessionMessage {
                to_session_id: request.to_session_id.clone(),
                message_id: message_id.clone(),
                reply_to: request.reply_to.clone(),
                text: request.text,
            }),
            at_unix_ms: sent_at_unix_ms,
        })
        .await;
        Ok(json!({
            "message_id": message_id,
            "to_session_id": request.to_session_id,
            "sent_at_unix_ms": sent_at_unix_ms,
        }))
    }

    async fn read_messages(
        &self,
        caller: &HomeCaller,
        request: ReadMessagesRequest,
    ) -> Result<Value, String> {
        let (watches, log) = inbox_parts(&self.session_watches, &self.notification_log)?;
        let read = watches
            .read_messages(
                &owner_of(caller, &self.os_user),
                request.after_message_id.as_deref(),
                request.limit,
                log,
                tddy_daemon_kernel::now_unix_ms(),
            )
            .await?;
        Ok(json!({
            "messages": read
                .messages
                .iter()
                .map(|message| json!({
                    "seq": message.seq,
                    "message_id": message.message_id,
                    "from_session_id": message.from_session_id,
                    "from": message.from,
                    "reply_to": message.reply_to,
                    "text": message.text,
                    "sent_at_unix_ms": message.sent_at_unix_ms,
                }))
                .collect::<Vec<_>>(),
            "lost": read.lost,
            "has_more": read.has_more,
        }))
    }
}

/// Whose inbox `caller`'s is on this host, for the OS user its token maps to.
fn owner_of(caller: &HomeCaller, os_user: &str) -> WatchOwner {
    WatchOwner {
        project_id: caller.project_id.clone(),
        session_id: caller.session_id.clone(),
        os_user: os_user.to_string(),
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
            tddy_data_dir: self.tddy_data_dir.clone(),
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
