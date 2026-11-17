//! The watch tools for a session that is not an orchestrator (`#activity-wake` 5/5).
//!
//! The watch tools are an orchestrator's, but any session may hold one watch: its inbox
//! ([`tddy_session_activity::session_watches::WatchFilter::inbox_of`]) — asked for as
//! `CreateSessionWatch` with the kind `message` alone, and streamed by
//! `tddy-tools monitor --kinds message` through `WaitSessionWatchEvents`. A watch-tool call from
//! such a session is served here ([`ExecToolRpcHandler::run_inbox_watch_tool`]) rather than
//! refused by the project-home role gate; any other watch it asks for is refused.

use std::path::Path;

use serde_json::{json, Value};
use tddy_agent_launch::conversation_spawn::session_tool_role;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_watches::{
    SessionWatchStore, WatchFilter, WatchOwner, MAX_WATCH_TTL_SECS,
};
use tddy_tool_engine::project_home_tools::SessionToolRole;
use tddy_tool_engine::project_home_watch_stream_tools::{
    is_project_home_watch_stream_tool, request_from as watch_stream_request, WatchStreamRequest,
    WAIT_SESSION_WATCH_EVENTS,
};
use tddy_tool_engine::project_home_watch_tools::{
    is_project_home_watch_tool, CANCEL_SESSION_WATCH, CREATE_SESSION_WATCH, LIST_SESSION_WATCHES,
    RENEW_SESSION_WATCH,
};
use tddy_tool_engine::ToolOutcome;

use super::super::project_home_watch_stream::hold_and_take;
use super::super::project_home_watches::watch_json;
use super::{inbox_parts, outcome_err, outcome_ok, ExecToolRpcHandler};

impl ExecToolRpcHandler {
    /// Whether `req` is a watch-tool call (`CreateSessionWatch`, `ListSessionWatches`,
    /// `RenewSessionWatch`, `CancelSessionWatch`, `WaitSessionWatchEvents`) from a session that is
    /// not an orchestrator: one [`Self::run_inbox_watch_tool`] serves. A caller whose record cannot
    /// be read is not: the project-home role gate refuses it as before.
    pub(in crate::exec_tool) fn is_inbox_watch_call(
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
    pub(in crate::exec_tool) async fn run_inbox_watch_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let os_user = self.os_user_of(&req.session_token)?;
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, &req.session_id);
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
