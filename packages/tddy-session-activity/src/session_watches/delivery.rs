use crate::project_notification_log::ProjectNotificationRecord;
use serde::{Deserialize, Serialize};

/// Why a watch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEndReason {
    /// Its time to live passed.
    Expired,
    /// Its orchestrator's agent ended.
    OrchestratorEnded,
    /// Every session it names ended.
    SessionsEnded,
}

/// A watch that ended, reported once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEnd {
    pub watch_id: String,
    pub reason: WatchEndReason,
}

/// What one delivery hands an orchestrator.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchDelivery {
    /// The records its watches matched, oldest first, each once; at most
    /// [`MAX_EVENTS_PER_ANSWER`].
    pub events: Vec<ProjectNotificationRecord>,
    /// Matched records the log evicted before they could be delivered.
    pub lost: u64,
    /// The watches that ended since the last delivery.
    pub ended: Vec<WatchEnd>,
}

impl WatchDelivery {
    /// Whether there is nothing to hand over.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.lost == 0 && self.ended.is_empty()
    }
}
