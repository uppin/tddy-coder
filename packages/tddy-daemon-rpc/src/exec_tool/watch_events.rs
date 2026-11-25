//! The events an orchestrator's session watches matched, attached to every tool answer it receives
//! on this host as `watchEvents` (`#activity-wake` 2/3).
//!
//! Only a successful answer whose result is a JSON object carries them, and only for a caller
//! that is a session of this host holding a live watch; any other answer consumes nothing. A call a
//! peer forwarded here for a session it holds is that peer's to decorate, and finds no session — so
//! no watches — here.

use serde_json::{json, Map, Value};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::project_notification_log::record_json_on;
use tddy_session_activity::session_watches::WatchOwner;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;

use super::ExecToolRpcHandler;

/// The key the events ride under in an answer's result object.
pub(super) const WATCH_EVENTS_KEY: &str = "watchEvents";

impl ExecToolRpcHandler {
    /// `response` to `req`, with the caller's matched watch events attached when it has any.
    pub(super) async fn with_watch_events(
        &self,
        req: &ExecuteToolRequest,
        response: ExecuteToolResponse,
    ) -> ExecuteToolResponse {
        let (Some(watches), Some(log)) = (
            self.project_home_host.session_watches.as_deref(),
            self.project_home_host.notification_log.as_deref(),
        ) else {
            return response;
        };
        if response.is_error {
            return response;
        }
        let Ok(mut result) = serde_json::from_str::<Value>(&response.result_json) else {
            return response;
        };
        let Some(object) = result.as_object_mut() else {
            return response;
        };
        let Some(owner) = self.watch_owner_of(req) else {
            return response;
        };
        let delivery = watches
            .take_delivery(&owner, log, tddy_daemon_kernel::now_unix_ms())
            .await;
        if delivery.is_empty() {
            return response;
        }
        let this_host =
            tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config);
        let Some(events) = delivery_events_json(&delivery, &this_host) else {
            return response;
        };
        object.insert(WATCH_EVENTS_KEY.to_string(), events);
        let mut answered = response;
        answered.result_json = result.to_string();
        answered
    }

    /// The watch owner `req`'s caller is on this host, or `None` for a caller this host holds no
    /// session for — a peer's caller, whose own host attaches its events.
    fn watch_owner_of(&self, req: &ExecuteToolRequest) -> Option<WatchOwner> {
        let os_user = self.os_user_of(&req.session_token).ok()?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir).ok()?;
        if metadata.project_id.is_empty() {
            return None;
        }
        Some(WatchOwner {
            project_id: metadata.project_id,
            session_id: req.session_id.clone(),
            os_user,
        })
    }
}

/// A delivery as it rides on an answer: the records in the log's own JSON, how many were lost to
/// eviction, and the watches that ended. `None` when a record cannot be spelled.
fn delivery_events_json(
    delivery: &tddy_session_activity::session_watches::WatchDelivery,
    this_host: &str,
) -> Option<Value> {
    let events = delivery
        .events
        .iter()
        .map(|record| record_json_on(record, this_host))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let ended = delivery
        .ended
        .iter()
        .map(|end| {
            serde_json::to_value(end.reason)
                .map(|reason| json!({ "watch_id": end.watch_id, "reason": reason }))
        })
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let mut object = Map::new();
    object.insert("events".to_string(), Value::Array(events));
    object.insert("lost".to_string(), json!(delivery.lost));
    object.insert("ended".to_string(), Value::Array(ended));
    Some(Value::Object(object))
}
