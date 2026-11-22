//! `RequestPathAccess` answered on this host (`#orchestra` 6/9).
//!
//! Routed from `run_locally` ahead of the local runner, to an orchestrator role alone. Paths the
//! session can already use at the asked mode — in its read reach (`orchestrator_reach`), or inside
//! a write grant it holds — are answered at once without asking. The rest are recorded in the host's [`PathAccessRequests`] — the store the activity service lists from and
//! answers into — announced as a `path_access` pending-question notification, and waited on for
//! [`PATH_ACCESS_WAIT`]. Allowing writes the session's grants (the store does that), so the call
//! returns once the paths are in reach.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::session_notification::QuestionNotice;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::path_access::{
    mode_name, PathAccessOutcome, PathAccessRequests, PendingPathAccess, PATH_ACCESS_QUESTION_KIND,
    PATH_ACCESS_WAIT,
};
use tddy_session_activity::path_grants::PathGrantMode;
use tddy_session_activity::session_notification_publishing::resolve_session_label;
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_tool_engine::path_access_tools::{
    execute_path_access_tool, PathAccessAnswer, PathAccessPort, RequestPathAccessArgs,
    RequestedAccess,
};
use tddy_tool_engine::reach::{ReadReach, REQUEST_PATH_ACCESS_TOOL};
use tddy_worktree_service::session_target::HomeCaller;

use super::orchestrator_reach::granted_write_path;
use super::project_home::caller::orchestrator_caller;
use super::project_home::caller::record_call;
use super::ExecToolRpcHandler;

/// [`PathAccessPort`] over this host: the caller's reach, the request store and the notification
/// bus.
pub(super) struct DaemonPathAccess {
    os_user: String,
    sessions_base: PathBuf,
    /// The caller's read reach, built for this call.
    reach: ReadReach,
    requests: PathAccessRequests,
    bus: Option<Arc<SessionNotificationBus>>,
}

/// Withdraws a request when the call waiting on it is dropped (its caller went away), so a dead
/// request does not stay in the operator's list.
struct Withdraw<'a> {
    requests: &'a PathAccessRequests,
    request_id: &'a str,
}

impl Drop for Withdraw<'_> {
    fn drop(&mut self) {
        self.requests.withdraw(self.request_id);
    }
}

fn grant_mode(access: RequestedAccess) -> PathGrantMode {
    match access {
        RequestedAccess::Read => PathGrantMode::Read,
        RequestedAccess::Write => PathGrantMode::Write,
    }
}

/// Whether `path` is readable through `reach`: inside it, and listed by git where a worktree hides
/// what git does not list.
fn readable(reach: &ReadReach, path: &str) -> bool {
    reach
        .resolve(path)
        .and_then(|resolved| reach.check_listed(&resolved))
        .is_ok()
}

impl DaemonPathAccess {
    /// Tell the operator about `pending` on the bus, as a `path_access` pending question.
    async fn announce(&self, bus: &SessionNotificationBus, pending: &PendingPathAccess) {
        let label = resolve_session_label(None, &self.sessions_base, &pending.session_id);
        let paths: Vec<String> = pending
            .paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let summary = format!(
            "{} access to {}: {}",
            mode_name(pending.mode),
            paths.join(", "),
            pending.reason
        );
        bus.publish(SessionNotification {
            session_id: pending.session_id.clone(),
            os_user: self.os_user.clone(),
            label: label.clone(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::PendingQuestion,
            text: format!("🔔 Session {label} asks for {summary}"),
            agent: None,
            question: Some(QuestionNotice {
                request_id: pending.request_id.clone(),
                kind: PATH_ACCESS_QUESTION_KIND.to_string(),
                tool_name: REQUEST_PATH_ACCESS_TOOL.to_string(),
                summary,
                questions: Vec::new(),
            }),
            ended: None,
            activity_status: None,
            message: None,
            at_unix_ms: pending.requested_at_unix_ms,
        })
        .await;
    }
}

#[async_trait]
impl PathAccessPort for DaemonPathAccess {
    async fn paths_outside_reach(
        &self,
        caller: &HomeCaller,
        paths: &[String],
        mode: RequestedAccess,
    ) -> Result<Vec<String>, String> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &caller.session_id);
        let reach = self.reach.clone();
        let paths = paths.to_vec();
        // Reading a grant store and asking git what a worktree lists both block.
        tokio::task::spawn_blocking(move || {
            paths
                .into_iter()
                .filter(|path| match mode {
                    RequestedAccess::Read => !readable(&reach, path),
                    RequestedAccess::Write => granted_write_path(&session_dir, path).is_none(),
                })
                .collect()
        })
        .await
        .map_err(|e| format!("checking the session's reach failed: {e}"))
    }

    async fn request_path_access(
        &self,
        caller: &HomeCaller,
        args: RequestPathAccessArgs,
    ) -> Result<PathAccessAnswer, String> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &caller.session_id);
        let pending = self.requests.request(
            &session_dir,
            &caller.session_id,
            args.paths.iter().map(PathBuf::from).collect(),
            grant_mode(args.mode),
            args.reason,
        )?;
        let _withdraw = Withdraw {
            requests: &self.requests,
            request_id: &pending.request_id,
        };

        if let Some(bus) = &self.bus {
            self.announce(bus, &pending).await;
        }

        Ok(
            match self
                .requests
                .wait(&pending.request_id, PATH_ACCESS_WAIT)
                .await
            {
                PathAccessOutcome::Allowed => PathAccessAnswer::Allowed,
                PathAccessOutcome::Denied => PathAccessAnswer::Denied,
                PathAccessOutcome::TimedOut => PathAccessAnswer::Unanswered,
            },
        )
    }
}

impl ExecToolRpcHandler {
    /// The [`PathAccessPort`] over this host for `req`'s orchestrator caller, whose home worktree is
    /// `worktree_root`, with its read reach built now.
    pub(super) async fn path_access_port(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> Result<DaemonPathAccess, Status> {
        let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
        Ok(DaemonPathAccess {
            os_user: self.os_user_of(&req.session_token)?,
            sessions_base: sessions_base.to_path_buf(),
            reach: self
                .orchestrator_read_reach(&req.session_token, session_dir, worktree_root)
                .await?,
            requests: self.project_home_host.path_access.clone(),
            bus: self.project_home_host.notification_bus.clone(),
        })
    }

    /// Serve `req`'s `RequestPathAccess` for an orchestrator caller whose home worktree is
    /// `worktree_root`, and record the call.
    pub(super) async fn run_path_access_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let caller = orchestrator_caller(sessions_base, &req.session_id, &req.tool_name)?;
        let port = self
            .path_access_port(req, sessions_base, worktree_root)
            .await?;
        let outcome = execute_path_access_tool(&port, &caller, &req.args_json).await;
        let response = ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        };
        record_call(sessions_base, req, &response);
        Ok(response)
    }
}
