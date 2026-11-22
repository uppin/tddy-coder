//! `WaitSessionWatchEvents` answered on this host (`#activity-wake` 4/5).
//!
//! The tool ([`tddy_tool_engine::project_home_watch_stream_tools`]) streams the caller's session
//! watches, `tddy_session_activity::session_watches`, for its project and OS user: each call
//! acknowledges what the stream printed, renews its lease and answers the next batch of what it
//! picks. It answers once it holds `max_events` events, `settle_ms` after its first event when that
//! is set, at once when there is a `lost` count or an ended watch to report, or when its window —
//! `timeout_ms` — elapses; while it waits it re-takes its batch whenever the notification bus
//! records something for the caller's project, and on a renewal tick that keeps the lease.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Map, Value};
use tddy_session_activity::project_notification_log::{
    record_json, ProjectNotificationLog, ProjectNotificationScope,
};
use tddy_session_activity::session_watches::{
    SessionWatchStore, StreamBatch, StreamPick, WatchKind, WatchOwner,
};
use tddy_tool_engine::project_home_inbox_tools::NO_NOTIFICATION_LOG;
use tddy_tool_engine::project_home_watch_stream_tools::{
    ProjectHomeWatchStreamPort, WatchStreamRequest,
};
use tddy_tool_engine::project_home_watch_tools::NO_SESSION_WATCHES;
use tddy_worktree_service::session_target::HomeCaller;
use tokio::time::Instant;

use super::project_home::DaemonProjectHome;
use super::watch_wake::wake_event_text;

/// How often a held call re-takes its batch: it renews the stream's lease while it waits, well
/// inside the shortest lease a host is built with.
const RENEW_TICK: Duration = Duration::from_millis(100);

/// One `WaitSessionWatchEvents` call over this host's watches and log, held for `owner` — whose
/// answer excludes its own session's records while it waits. Shared by the port the project-home
/// role serves and by a session's own inbox watch (`#activity-wake` 5/5).
///
/// The call acknowledges once, on its first take: what a later take of the same call hands, the
/// caller has not seen until this call answers.
pub(super) async fn hold_and_take(
    watches: &SessionWatchStore,
    log: &ProjectNotificationLog,
    owner: WatchOwner,
    exclude_session_id: String,
    request: WatchStreamRequest,
) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_millis(request.timeout_ms);
    let settle = Duration::from_millis(request.settle_ms);
    let pick = StreamPick {
        kinds: request
            .kinds
            .iter()
            .map(|name| WatchKind::parse(name).ok_or_else(|| format!("unknown kind {name}")))
            .collect::<Result<Vec<_>, _>>()?,
        session_ids: request.session_ids.clone(),
        max_events: request.max_events,
    };
    let mut stream_id = request.stream_id.clone();
    let mut ack_seq = request.ack_seq;
    let mut settles_at: Option<Instant> = None;
    loop {
        let batch = watches
            .take_stream_batch_picking(
                &owner,
                stream_id.as_deref(),
                &request.watch_ids,
                ack_seq.take(),
                &pick,
                log,
                tddy_daemon_kernel::now_unix_ms(),
            )
            .await
            .map_err(|refusal| refusal.to_string())?;
        stream_id = Some(batch.stream_id.clone());
        let now = Instant::now();
        if settles_at.is_none() && !settle.is_zero() && !batch.events.is_empty() {
            settles_at = Some(now + settle);
        }
        let holds_enough = batch.events.len() >= request.max_events;
        let has_news = batch.lost > 0 || !batch.ended.is_empty();
        let settled = settles_at.is_some_and(|at| now >= at);
        if holds_enough || has_news || settled || now >= deadline {
            return batch_json(batch);
        }
        // Hold until the log records something for the caller's project, or the renewal
        // tick: re-taking the batch then renews the lease while the call is held.
        let until = settles_at.map_or(deadline, |at| at.min(deadline));
        let remaining = until.saturating_duration_since(now);
        let scope = ProjectNotificationScope {
            project_id: owner.project_id.clone(),
            os_user: owner.os_user.clone(),
            exclude_session_id: exclude_session_id.clone(),
            started_by: None,
        };
        let cursor = log.head_seq(&owner.project_id).to_string();
        let _ = log.wait(&scope, &cursor, remaining.min(RENEW_TICK)).await;
    }
}

#[async_trait]
impl ProjectHomeWatchStreamPort for DaemonProjectHome {
    async fn wait_watch_events(
        &self,
        home: &HomeCaller,
        request: WatchStreamRequest,
    ) -> Result<Value, String> {
        let watches = self
            .session_watches
            .as_deref()
            .ok_or_else(|| NO_SESSION_WATCHES.to_string())?;
        let log = self
            .notification_log
            .as_deref()
            .ok_or_else(|| NO_NOTIFICATION_LOG.to_string())?;
        let owner = WatchOwner {
            project_id: home.project_id.clone(),
            session_id: home.session_id.clone(),
            os_user: self.os_user.clone(),
        };
        hold_and_take(watches, log, owner, home.session_id.clone(), request).await
    }
}

/// A batch as the tool answers it: each event carries the monitor line's fields plus its record.
fn batch_json(batch: StreamBatch) -> Result<Value, String> {
    let mut events = Vec::with_capacity(batch.events.len());
    for event in &batch.events {
        let record = &event.record;
        let mut object = Map::new();
        object.insert("seq".to_string(), json!(record.seq));
        object.insert("watch_ids".to_string(), json!(event.watch_ids));
        object.insert(
            "kind".to_string(),
            json!(WatchKind::of(record).map_or("event", WatchKind::as_str)),
        );
        object.insert("session_id".to_string(), json!(record.session_id));
        object.insert("session".to_string(), json!(record.label));
        object.insert("summary".to_string(), json!(wake_event_text(record)));
        if let Some(question) = &record.question {
            object.insert("request_id".to_string(), json!(question.request_id));
        }
        object.insert(
            "record".to_string(),
            record_json(record).map_err(|e| format!("serialize a notification record: {e}"))?,
        );
        events.push(Value::Object(object));
    }
    let ended = batch
        .ended
        .iter()
        .map(|end| {
            serde_json::to_value(end.reason)
                .map(|reason| json!({ "watch_id": end.watch_id, "reason": reason }))
                .map_err(|e| format!("serialize a watch end: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "stream_id": batch.stream_id,
        "events": events,
        "lost": batch.lost,
        "ended": ended,
        "next_ack_seq": batch.next_ack_seq,
    }))
}
