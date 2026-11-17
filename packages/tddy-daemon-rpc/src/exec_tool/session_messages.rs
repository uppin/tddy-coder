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
use tddy_session_activity::session_watches::{
    SessionWatchStore, WatchFilter, WatchOwner, MAX_WATCH_TTL_SECS,
};
use tddy_tool_engine::project_home_inbox_tools::NO_NOTIFICATION_LOG;
use tddy_tool_engine::project_home_tools::SessionToolRole;
use tddy_tool_engine::project_home_watch_stream_tools::{
    is_project_home_watch_stream_tool, request_from as watch_stream_request, WatchStreamRequest,
    WAIT_SESSION_WATCH_EVENTS,
};
use tddy_tool_engine::project_home_watch_tools::{
    is_project_home_watch_tool, CANCEL_SESSION_WATCH, CREATE_SESSION_WATCH, LIST_SESSION_WATCHES,
    NO_SESSION_WATCHES, RENEW_SESSION_WATCH,
};
use tddy_tool_engine::session_message_tools::{
    execute_session_message_tool, Addressable, MessageRefusal, PostMessageRequest,
    ReadMessagesRequest, SessionMessagePort, NO_SESSION_MESSAGES,
};
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_target::HomeCaller;

use super::project_home_watch_stream::hold_and_take;
use super::project_home_watches::watch_json;
use super::ExecToolRpcHandler;

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
        tddy_core::read_session_metadata(&session_dir)
            .is_ok_and(|metadata| session_tool_role(&metadata) != SessionToolRole::ProjectHome)
    }

    /// Serve a watch-tool call from a session that is not an orchestrator. `CreateSessionWatch`
    /// creates only its inbox watch — the kind `message` alone, selecting no session, as
    /// [`WatchFilter::inbox_of`] the caller; every other watch it asks for is denied, as the
    /// project-home role gate denies a watch tool to such a session. The other watch tools are
    /// served over the caller's own watches — which can only be that one — and a watch that is not
    /// the caller's is denied. An `Err` here is that denial; a refused call is an `is_error`
    /// outcome, as a tool refusal is.
    pub(super) async fn run_inbox_watch_tool(
        &self,
        req: &ExecuteToolRequest,
        _sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let os_user = self.os_user_of(&req.session_token)?;
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(_sessions_base, &req.session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
            Status::permission_denied(format!(
                "tool {} is refused: cannot read session {}'s record ({e})",
                req.tool_name, req.session_id
            ))
        })?;
        let owner = WatchOwner {
            project_id: metadata.project_id,
            session_id: req.session_id.clone(),
            os_user,
        };
        let outcome = match req.tool_name.as_str() {
            CREATE_SESSION_WATCH => self.create_an_inbox_watch(req, &owner).await,
            LIST_SESSION_WATCHES => self.list_the_inbox_watches(&owner).await,
            RENEW_SESSION_WATCH | CANCEL_SESSION_WATCH => {
                self.work_on_an_inbox_watch(req, &owner).await
            }
            // The only other watch tool: the stream the monitor loops.
            _ => self.stream_the_inbox_watch(req, &owner).await,
        };
        match outcome {
            Ok(outcome) => Ok(ExecuteToolResponse {
                result_json: outcome.result_json,
                is_error: outcome.is_error,
                error_message: outcome.error_message,
                job_id: outcome.job_id,
                job_running: outcome.job_running,
            }),
            Err(denial) => Err(Status::permission_denied(format!(
                "tool {} is refused: {denial}",
                req.tool_name
            ))),
        }
    }

    /// The host's watches and log, or the refusal a tool without them is answered.
    fn inbox_parts(&self) -> Result<(&SessionWatchStore, &ProjectNotificationLog), String> {
        inbox_parts(
            &self.project_home_host.session_watches,
            &self.project_home_host.notification_log,
        )
    }

    /// `CreateSessionWatch` for a session that is not an orchestrator: the inbox watch alone.
    async fn create_an_inbox_watch(
        &self,
        req: &ExecuteToolRequest,
        owner: &WatchOwner,
    ) -> Result<ToolOutcome, String> {
        let args: Value = match serde_json::from_str(&req.args_json) {
            Ok(args) => args,
            Err(e) => {
                return Ok(outcome_err(format!(
                    "{CREATE_SESSION_WATCH}: invalid args_json: {e}"
                )))
            }
        };
        let selects_no_session = args
            .get("session_ids")
            .is_none_or(|ids| ids.as_array().is_none_or(Vec::is_empty))
            && args.get("started_by_me").and_then(Value::as_bool) != Some(true);
        let names_its_own_messages =
            args.get("kinds")
                .and_then(Value::as_array)
                .is_some_and(|kinds| {
                    kinds.iter().filter_map(Value::as_str).collect::<Vec<_>>() == ["message"]
                });
        if !selects_no_session || !names_its_own_messages {
            return Err(
                "a session that is not an orchestrator may hold only its own message watch: CreateSessionWatch with the kind message alone and no session selected"
                    .to_string(),
            );
        }
        let ttl_seconds = match args.get("ttl_seconds").and_then(Value::as_u64) {
            Some(ttl) if ttl > 0 && ttl <= MAX_WATCH_TTL_SECS => ttl,
            Some(ttl) => {
                return Ok(outcome_err(format!(
                    "{CREATE_SESSION_WATCH}: ttl_seconds must be between 1 and {MAX_WATCH_TTL_SECS} (24 hours), not {ttl}"
                )))
            }
            None => {
                return Ok(outcome_err(format!(
                    "{CREATE_SESSION_WATCH}: ttl_seconds is required: how long the watch lives, in seconds"
                )))
            }
        };
        let (watches, log) = match self.inbox_parts() {
            Ok(parts) => parts,
            Err(refusal) => return Ok(outcome_err(refusal)),
        };
        match watches
            .create(
                owner,
                WatchFilter::inbox_of(&owner.session_id),
                std::time::Duration::from_secs(ttl_seconds),
                log,
                tddy_daemon_kernel::now_unix_ms(),
            )
            .await
        {
            Ok(watch) => Ok(outcome_ok(watch_json(&watch, None).to_string())),
            Err(refusal) => Ok(outcome_err(format!("{CREATE_SESSION_WATCH}: {refusal}"))),
        }
    }

    /// The caller's own watches, listed.
    async fn list_the_inbox_watches(&self, owner: &WatchOwner) -> Result<ToolOutcome, String> {
        let (watches, log) = match self.inbox_parts() {
            Ok(parts) => parts,
            Err(refusal) => return Ok(outcome_err(refusal)),
        };
        let listed = watches
            .list(owner, log, tddy_daemon_kernel::now_unix_ms())
            .await;
        Ok(outcome_ok(
            json!({
                "watches": listed
                    .iter()
                    .map(|listed| watch_json(&listed.watch, Some(listed.waiting)))
                    .collect::<Vec<_>>(),
            })
            .to_string(),
        ))
    }

    /// `RenewSessionWatch` / `CancelSessionWatch` over the caller's own watches.
    async fn work_on_an_inbox_watch(
        &self,
        req: &ExecuteToolRequest,
        owner: &WatchOwner,
    ) -> Result<ToolOutcome, String> {
        let args: Value = match serde_json::from_str(&req.args_json) {
            Ok(args) => args,
            Err(e) => {
                return Ok(outcome_err(format!(
                    "{}: invalid args_json: {e}",
                    req.tool_name
                )))
            }
        };
        let watch_id = match args.get("watch_id").and_then(Value::as_str) {
            Some(watch_id) if !watch_id.is_empty() => watch_id.to_string(),
            _ => {
                return Ok(outcome_err(format!(
                    "{}: watch_id is required: the watch_id CreateSessionWatch or ListSessionWatches returned",
                    req.tool_name
                )))
            }
        };
        let ttl_seconds = if req.tool_name == RENEW_SESSION_WATCH {
            match args.get("ttl_seconds").and_then(Value::as_u64) {
                Some(ttl) if ttl > 0 && ttl <= MAX_WATCH_TTL_SECS => Some(ttl),
                Some(ttl) => {
                    return Ok(outcome_err(format!(
                        "{}: ttl_seconds must be between 1 and {MAX_WATCH_TTL_SECS} (24 hours), not {ttl}",
                        req.tool_name
                    )))
                }
                None => {
                    return Ok(outcome_err(format!(
                        "{}: ttl_seconds is required: how long the watch lives, in seconds",
                        req.tool_name
                    )))
                }
            }
        } else {
            None
        };
        let (watches, _log) = match self.inbox_parts() {
            Ok(parts) => parts,
            Err(refusal) => return Ok(outcome_err(refusal)),
        };
        self.refuse_a_watch_that_is_not_the_callers(watches, _log, owner, &watch_id)
            .await?;
        let renewed = if let Some(ttl_seconds) = ttl_seconds {
            watches
                .renew(
                    owner,
                    &watch_id,
                    std::time::Duration::from_secs(ttl_seconds),
                    tddy_daemon_kernel::now_unix_ms(),
                )
                .await
                .map(|watch| watch_json(&watch, None).to_string())
        } else {
            watches
                .cancel(owner, &watch_id)
                .await
                .map(|_| json!({ "watch_id": watch_id, "cancelled": true }).to_string())
        };
        match renewed {
            Ok(answer) => Ok(outcome_ok(answer)),
            Err(refusal) => Ok(outcome_err(format!("{}: {refusal}", req.tool_name))),
        }
    }

    /// `WaitSessionWatchEvents` over the caller's own watches, held by #934's loop.
    async fn stream_the_inbox_watch(
        &self,
        req: &ExecuteToolRequest,
        owner: &WatchOwner,
    ) -> Result<ToolOutcome, String> {
        let args: Value = match serde_json::from_str(&req.args_json) {
            Ok(args) => args,
            Err(e) => {
                return Ok(outcome_err(format!(
                    "{WAIT_SESSION_WATCH_EVENTS}: invalid args_json: {e}"
                )))
            }
        };
        let request: WatchStreamRequest = match watch_stream_request(&args) {
            Ok(request) => request,
            Err(refusal) => {
                return Ok(outcome_err(format!(
                    "{WAIT_SESSION_WATCH_EVENTS}: {refusal}"
                )))
            }
        };
        let (watches, log) = match self.inbox_parts() {
            Ok(parts) => parts,
            Err(refusal) => return Ok(outcome_err(refusal)),
        };
        for watch_id in &request.watch_ids {
            self.refuse_a_watch_that_is_not_the_callers(watches, log, owner, watch_id)
                .await?;
        }
        match hold_and_take(
            watches,
            log,
            owner.clone(),
            owner.session_id.clone(),
            request,
        )
        .await
        {
            Ok(batch) => Ok(outcome_ok(batch.to_string())),
            Err(refusal) => Ok(outcome_err(format!(
                "{WAIT_SESSION_WATCH_EVENTS}: {refusal}"
            ))),
        }
    }

    /// Deny (`Err`) when `watch_id` is not a live watch of `owner`: a watch is worked on and
    /// streamed by its holder alone.
    async fn refuse_a_watch_that_is_not_the_callers(
        &self,
        watches: &SessionWatchStore,
        log: &ProjectNotificationLog,
        owner: &WatchOwner,
        watch_id: &str,
    ) -> Result<(), String> {
        let live = watches
            .list(owner, log, tddy_daemon_kernel::now_unix_ms())
            .await
            .iter()
            .any(|listed| listed.watch.watch_id == watch_id);
        if live {
            Ok(())
        } else {
            Err(format!(
                "no live watch {watch_id} of yours: a session that is not an orchestrator works only on its own watches"
            ))
        }
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
