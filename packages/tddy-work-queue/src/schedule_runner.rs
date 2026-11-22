//! Fires armed schedules (`#workqueue` 14/14): re-evaluated on every inbox event and queue change, a
//! schedule whose conditions all hold fires **once** and is disarmed in the same step, however many
//! events arrive at once.

use std::sync::Arc;

use async_trait::async_trait;

use crate::schedule::ScheduleView;

/// What firing a schedule does (the land or propagate engine, through the daemon).
#[async_trait]
pub trait FireSchedule: Send + Sync {
    async fn fire(&self, view: &ScheduleView) -> Result<(), String>;
}

pub struct ScheduleRunner {
    fire: Arc<dyn FireSchedule>,
}

impl ScheduleRunner {
    pub fn new(fire: Arc<dyn FireSchedule>) -> Self {
        Self { fire }
    }

    /// Re-evaluated `views` arrived (an inbox event or a queue change): fire each ready one that is
    /// still armed, exactly once. Answers the item ids fired.
    pub async fn on_change(&self, views: &[ScheduleView]) -> Vec<String> {
        // TODO(n14 land-propagate): implement
        let _ = (&self.fire, views);
        todo!("TODO(n14): implement ScheduleRunner::on_change")
    }

    /// Whether `item_id`'s schedule is still armed.
    pub fn is_armed(&self, item_id: &str) -> bool {
        // TODO(n14 land-propagate): implement
        let _ = item_id;
        todo!("TODO(n14): implement ScheduleRunner::is_armed")
    }
}
