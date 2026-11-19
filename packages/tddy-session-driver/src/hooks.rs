//! A driven session's lifecycle hooks, wired and followed as a daemon start wires and follows a
//! target session's, and kept for the MCP client to read.
//!
//! Every target is treated as a daemon treats a target, a local one included: the agent's hooks
//! are rendered with `HookCommandParams::remote_log`, so each one is the shell one-liner that
//! appends `<event>\t<token>\t<payload>` to `.tddy/hooks.log` in the worktree
//! (`tddy_core::remote_hook_command`); the log is created with
//! `remote_hook_follower::create_remote_hook_log` and read with a `RemoteHookFollower` through the
//! session's shell (`LocalShell` on this machine); and each line is mapped to the daemon's two
//! hook reports by `ActivityHookSink`. Only the end differs: where a daemon serves those reports
//! to its activity service, the driver's [`HookReportDelivery`] records them beside the hook that
//! raised them.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_agent_launch::remote_hook_follower::{
    create_remote_hook_log, remove_remote_hook_log, ActivityHookSink, HookLine, HookReportDelivery,
    HookSink, RemoteHookFollower, RemoteHookFollowers,
};
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::HookCommandParams;
use tddy_remote_shell::RemoteShell;
use tddy_service::proto::activity::{ReportAgentActivityRequest, ReportSessionStatusRequest};

/// One hook the session's agent fired, with the reports the daemon's activity handling would have
/// been served for it.
#[derive(Debug, Clone, PartialEq)]
pub struct HookRecord {
    /// Its place among the session's hooks, from 0.
    pub index: usize,
    pub event: String,
    /// The payload as the agent wrote it to the hook's stdin.
    pub payload: Value,
    /// Each report, as `{kind: "session_status" | "agent_activity", …}`, without its hook token.
    pub reports: Vec<Value>,
}

impl HookRecord {
    /// The record as `read_hooks` and `wait_for_hook` answer it.
    pub fn to_json(&self) -> Value {
        json!({
            "index": self.index,
            "event": self.event,
            "payload": self.payload,
            "reports": self.reports,
        })
    }
}

/// Every hook a session's agent fired, in the order its log holds them.
#[derive(Default)]
pub struct HookRecorder {
    records: Mutex<Vec<HookRecord>>,
    arrived: tokio::sync::Notify,
}

impl HookRecorder {
    /// The hooks from the `since`-th on.
    pub fn since(&self, since: usize) -> Vec<HookRecord> {
        self.lock().iter().skip(since).cloned().collect()
    }

    /// How many hooks have arrived.
    pub fn count(&self) -> usize {
        self.lock().len()
    }

    /// The first hook of `event` from the `since`-th on, waiting up to `timeout` for one to arrive.
    pub async fn wait_for(
        &self,
        event: &str,
        since: usize,
        timeout: Duration,
    ) -> Option<HookRecord> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            // Registered before the look, so a hook that arrives between the two still wakes it.
            let arrived = self.arrived.notified();
            if let Some(found) = self
                .lock()
                .iter()
                .skip(since)
                .find(|record| record.event == event)
            {
                return Some(found.clone());
            }
            if tokio::time::timeout_at(deadline, arrived).await.is_err() {
                return None;
            }
        }
    }

    fn record_hook(&self, line: &HookLine) {
        let mut records = self.lock();
        let index = records.len();
        records.push(HookRecord {
            index,
            event: line.event.clone(),
            payload: line.payload.clone(),
            reports: Vec::new(),
        });
    }

    /// Add `report` to the hook that raised it: the last one recorded, since the follower delivers
    /// the next hook only once this one's reports have been served.
    fn record_report(&self, report: Value) {
        if let Some(last) = self.lock().last_mut() {
            last.reports.push(report);
        }
    }

    fn announce(&self) {
        self.arrived.notify_waiters();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<HookRecord>> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Records each followed hook, then hands it to the daemon's own mapping.
struct RecordingSink {
    recorder: Arc<HookRecorder>,
    activity: ActivityHookSink,
}

#[async_trait]
impl HookSink for RecordingSink {
    async fn deliver(&self, line: HookLine) {
        self.recorder.record_hook(&line);
        self.activity.deliver(line).await;
        self.recorder.announce();
    }
}

/// The driver's end of a delivered hook: each report the daemon's activity service would serve,
/// recorded beside its hook.
struct RecordingDelivery {
    recorder: Arc<HookRecorder>,
}

#[async_trait]
impl HookReportDelivery for RecordingDelivery {
    async fn report_session_status(
        &self,
        request: ReportSessionStatusRequest,
    ) -> Result<(), String> {
        self.recorder.record_report(json!({
            "kind": "session_status",
            "session_id": request.session_id,
            "os_user": request.os_user,
            "status": request.status,
            "message": request.message,
            "hook_event_name": request.hook_event_name,
            "tool_name": request.tool_name,
            "tool_input_json": request.tool_input_json,
            "agent_conversation_id": request.agent_conversation_id,
            "hook_fields": request.hook_fields,
        }));
        Ok(())
    }

    async fn report_agent_activity(
        &self,
        request: ReportAgentActivityRequest,
    ) -> Result<(), String> {
        self.recorder.record_report(json!({
            "kind": "agent_activity",
            "session_id": request.session_id,
            "os_user": request.os_user,
            "event": request.event,
            "tool_name": request.tool_name,
            "input_json": request.input_json,
            "result_json": request.result_json,
            "is_error": request.is_error,
            "error_message": request.error_message,
            "tool_use_id": request.tool_use_id,
        }));
        Ok(())
    }
}

/// A hooked session's wiring: its token, the shell its log lives on, and what has arrived.
pub struct SessionHooks {
    pub hook_token: String,
    pub os_user: String,
    shell: Arc<dyn RemoteShell>,
    log_path: String,
    recorder: Arc<HookRecorder>,
    followers: RemoteHookFollowers,
}

impl SessionHooks {
    /// Wire `spec`'s hooks for a session in `worktree` on `shell`'s machine, when its spec declares
    /// the `hooks` capability: a fresh token, and the hook log created there as a daemon start
    /// creates it. `None` for an agent without hooks.
    pub async fn wired(
        spec: &CodingAgentSpec,
        shell: Arc<dyn RemoteShell>,
        worktree: &Path,
    ) -> Result<Option<Self>, String> {
        if !spec.has(tddy_coding_agent_specs::Capability::Hooks) {
            return Ok(None);
        }
        let log_path = create_remote_hook_log(shell.as_ref(), worktree)
            .await
            .map_err(|error| format!("the session's hook log could not be created: {error}"))?;
        Ok(Some(Self {
            hook_token: uuid::Uuid::new_v4().to_string(),
            os_user: std::env::var("USER").unwrap_or_default(),
            shell,
            log_path,
            recorder: Arc::new(HookRecorder::default()),
            followers: RemoteHookFollowers::new(),
        }))
    }

    /// The hooks' command parameters, as a daemon start on a target renders them: every hook
    /// appends to the log relative to the worktree the agent runs in.
    pub fn params<'a>(
        &'a self,
        spec: &CodingAgentSpec,
        session_id: &'a str,
    ) -> HookCommandParams<'a> {
        HookCommandParams {
            tddy_tools_path: "",
            daemon_url: "",
            session_id,
            os_user: &self.os_user,
            hook_token: &self.hook_token,
            daemon_socket: None,
            answer_wait_secs: spec.answer_wait_secs(),
            remote_log: Some(tddy_core::REMOTE_HOOK_LOG),
            route_env: &[],
        }
    }

    /// Follow the log from its first byte, once the agent is up.
    pub fn follow(&self, session_id: &str, worktree: &Path) {
        let sink = Arc::new(RecordingSink {
            recorder: Arc::clone(&self.recorder),
            activity: ActivityHookSink::new(
                Arc::new(RecordingDelivery {
                    recorder: Arc::clone(&self.recorder),
                }),
                session_id,
                &self.hook_token,
                &self.os_user,
            ),
        });
        let follower = RemoteHookFollower::start(
            Arc::clone(&self.shell),
            &self.log_path,
            &self.hook_token,
            sink,
        );
        self.followers.insert(
            session_id,
            follower,
            Arc::clone(&self.shell),
            worktree.to_path_buf(),
        );
    }

    /// The registry the follower is kept in, for the session's exit and stop to end it.
    pub fn followers(&self) -> RemoteHookFollowers {
        self.followers.clone()
    }

    /// What has arrived.
    pub fn recorder(&self) -> Arc<HookRecorder> {
        Arc::clone(&self.recorder)
    }

    /// Remove the session's log from `worktree`: a start that failed before any follower was
    /// registered, or a stop making sure its exit's removal is done. A log already gone is not an
    /// error.
    pub async fn remove_log(&self, worktree: &Path) {
        if let Err(error) = remove_remote_hook_log(self.shell.as_ref(), worktree).await {
            log::warn!("removing the session's hook log: {error}");
        }
    }
}
