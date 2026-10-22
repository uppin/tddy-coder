//! The two **hook** reports' bodies: `ReportSessionStatus` and `ReportAgentActivity`, as inherent
//! methods the `ActivityService` trait impl in [`super::rpc`] delegates to.
//!
//! TODO(restructure-engine): moved by hand (the `#653` precedent), because `extract_method` is
//! refused inside an `#[async_trait]` impl and a trait impl cannot be split across files —
//! `docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md`. Bodies are
//! byte-identical apart from taking the already-unwrapped request.

use crate::service::ActivityServiceImpl;
use crate::session_notifications::{
    notification_for_activity_status, notification_for_agent_tool_call,
};
use std::path::PathBuf;
use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::{
    ReportAgentActivityRequest, ReportAgentActivityResponse, ReportSessionStatusRequest,
    ReportSessionStatusResponse,
};

impl ActivityServiceImpl {
    /// The per-worktree `claude` / `cursor` hook reporting what its session is doing.
    ///
    /// Everything is validated before any IO, and the credential before any write: an unknown
    /// status or a bad `hook_token` must not be able to touch a session directory at all.
    pub(super) async fn report_session_status_impl(
        &self,
        req: ReportSessionStatusRequest,
    ) -> Result<Response<ReportSessionStatusResponse>, Status> {
        // The status string is checked before the path is resolved: an unknown status is a bad
        // request whatever session it names.
        tddy_core::SessionActivityStatus::from_wire(&req.status)
            .ok_or_else(|| Status::invalid_argument(format!("unknown status: {}", req.status)))?;

        let (sessions_base, session_dir) = self.hook_session_dir(&req.os_user, &req.session_id)?;
        let meta = self.hook_session(&session_dir)?;

        // claude-cli and cursor-cli sessions support hook status reporting. Refused rather than
        // recorded for anything else: a status on a session type that never raises one would be a
        // row nothing keeps current — and refused *before* the credential, so a hook wired to the
        // wrong session is told that rather than that its token is bad.
        let session_type = meta.session_type.as_deref().unwrap_or("");
        if session_type != "claude-cli" && session_type != "cursor-cli" {
            return Err(Status::failed_precondition(
                "session_type is not claude-cli or cursor-cli",
            ));
        }

        // A local-process string comparison: the token is a per-session secret written into the
        // hook command at worktree-prep time, and both ends are on this host.
        if meta.hook_token.as_deref().unwrap_or("") != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        tddy_core::update_activity_status(&session_dir, &req.status)
            .map_err(|e| Status::internal(format!("failed to update activity status: {}", e)))?;

        log::debug!(
            target: "tddy_daemon::connection_service",
            "report_session_status: session={} status={}",
            req.session_id,
            req.status
        );

        // One publish, every interested subscriber: Telegram renders the attention-worthy ones,
        // and the notification stream carries all of them to the drawer's indicators. A subscriber
        // that fails is logged by the bus and never fails this hook (PRD NFR3).
        //
        // The notification names `req.os_user` as its owner — the same user whose sessions
        // directory the hook token was just checked against — so the stream can hand it to that
        // operator's clients and to no one else's.
        self.publish_for_session(&sessions_base, &req.session_id, |label| {
            notification_for_activity_status(
                &req.session_id,
                &req.os_user,
                label,
                &req.status,
                tddy_daemon_kernel::now_unix_ms(),
            )
        })
        .await;

        Ok(Response::new(ReportSessionStatusResponse { ok: true }))
    }

    /// The per-worktree hook reporting one of its agent's tool calls, before and after it runs.
    pub(super) async fn report_agent_activity_impl(
        &self,
        req: ReportAgentActivityRequest,
    ) -> Result<Response<ReportAgentActivityResponse>, Status> {
        let (sessions_base, session_dir) = self.hook_session_dir(&req.os_user, &req.session_id)?;
        let meta = self.hook_session(&session_dir)?;
        // Validated before anything is read off the checkout or appended to the log: the token is a
        // per-session secret, and this path writes.
        if meta.hook_token.as_deref().unwrap_or("") != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        // AC1/AC2 of `docs/ft/daemon/session-worktree-sync.md`: the record names the commit it was
        // made against and the paths it declared, so a consumer holding a patch can place it. The
        // HEAD is read from the filesystem rather than by spawning `git rev-parse` — an agent makes
        // a great many tool calls, and a subprocess on each would be paid on every one of them.
        //
        // A session with no checkout on this host stamps neither: `read_head_commit` returns an
        // empty string when HEAD cannot be resolved, and a path has nothing to be relative to. That
        // is the honest answer AC1 asks for, and the reason no sha is invented in its place.
        let worktree_root = meta.repo_path.as_deref().map(PathBuf::from);
        let head_commit = worktree_root
            .as_deref()
            .map(tddy_core::git_head::read_head_commit)
            .unwrap_or_default();
        let input = tddy_core::agent_activity::parse_activity_json(&req.input_json);
        let changed_paths = worktree_root
            .as_deref()
            .map(|root| tddy_core::agent_activity::declared_paths(&req.tool_name, &input, root))
            .unwrap_or_default();

        let record = match req.event.as_str() {
            "PreToolUse" => {
                // A tool call started: mint a call_id, remember it so the paired PostToolUse can
                // reuse it, and append the `running` row.
                let call_id = uuid::Uuid::new_v4().to_string();
                self.ports.activity.push_pending(&req.session_id, &call_id);
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: tddy_core::agent_activity::STATUS_RUNNING.to_string(),
                    result: serde_json::Value::Null,
                    error_message: String::new(),
                    started_unix_ms: tddy_daemon_kernel::now_unix_ms(),
                    completed_unix_ms: 0,
                    source: "claude-cli".to_string(),
                    head_commit,
                    // The tick that covers this call has not been measured yet; the poll loop
                    // attributes it when it runs. `NO_TICK` is the wire's "no tick has covered it
                    // yet", which a session's first real tick is distinguishable from — see
                    // [`crate::next_tick`].
                    activity_seq: crate::NO_TICK,
                    changed_paths,
                }
            }
            "PostToolUse" => {
                // The tool call finished: pair with the most-recent pending call_id (fresh id when
                // none is outstanding, e.g. a hook restart), and append the terminal row.
                let call_id = self
                    .ports
                    .activity
                    .pop_pending(&req.session_id)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let status = if req.is_error {
                    tddy_core::agent_activity::STATUS_ERROR
                } else {
                    tddy_core::agent_activity::STATUS_COMPLETED
                };
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: status.to_string(),
                    result: tddy_core::agent_activity::parse_activity_json(&req.result_json),
                    error_message: req.error_message,
                    started_unix_ms: 0,
                    completed_unix_ms: tddy_daemon_kernel::now_unix_ms(),
                    source: "claude-cli".to_string(),
                    head_commit,
                    // As on the `running` row: the covering tick is the poll loop's to attribute.
                    activity_seq: crate::NO_TICK,
                    changed_paths,
                }
            }
            other => {
                return Err(Status::invalid_argument(format!(
                    "unknown event: {other} (expected PreToolUse or PostToolUse)"
                )));
            }
        };

        // The durable log is the source of truth; a write failure must not fail the hook call.
        if let Err(e) = tddy_core::agent_activity::append_agent_activity(&session_dir, &record) {
            log::warn!(
                target: "tddy_daemon::connection_service",
                "agent_activity: failed to persist {} for session {}: {}",
                req.event,
                req.session_id,
                e
            );
        }
        // The agent's own tool loop is the other thing that means "this session is working", and
        // the only one a cursor-cli or tool session reports at all. Owned by `req.os_user`, as at
        // the activity-status site above: the notification stream relays it to that operator only.
        let tool_name = record.tool_name.clone();
        self.publish_for_session(&sessions_base, &req.session_id, |label| {
            Some(notification_for_agent_tool_call(
                &req.session_id,
                &req.os_user,
                label,
                &tool_name,
                tddy_daemon_kernel::now_unix_ms(),
            ))
        })
        .await;
        self.ports.activity.publish(&req.session_id, record);
        // The record is **not** broadcast into the session room from here, deliberately.
        //
        // A record announced at this point names a tick nothing has measured yet: its
        // `activity_seq` is still `NO_TICK` and the delta covering its files is produced by the
        // next poll tick, so a participant that reacted to it and asked for the call's delta would
        // be told `UnknownCall` — an announcement that arrives before the thing it announces.
        //
        // The poll loop is the single broadcaster instead, tailing `agent-activity.jsonl`, which is
        // also what makes cursor-cli and tool sessions visible: their agents never call this RPC at
        // all, and a room fed only from here would carry claude-cli activity and nothing else.
        Ok(Response::new(ReportAgentActivityResponse { ok: true }))
    }
}
