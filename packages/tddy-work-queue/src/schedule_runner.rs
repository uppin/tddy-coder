//! Fires armed schedules (`#workqueue` 14/14): re-evaluated on every inbox event and queue change, a
//! schedule whose conditions all hold fires **once** and is disarmed in the same step, however many
//! events arrive at once.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::schedule::{ConditionState, ScheduleView};

/// What firing a schedule does (the land or propagate engine, through the daemon).
#[async_trait]
pub trait FireSchedule: Send + Sync {
    async fn fire(&self, view: &ScheduleView) -> Result<(), String>;
}

/// Schedules that have fired, by item id, with the `armed_at` of the firing one: a schedule armed
/// again later carries a new `armed_at` and fires again.
pub struct ScheduleRunner {
    fire: Arc<dyn FireSchedule>,
    fired: Mutex<HashMap<String, String>>,
}

impl ScheduleRunner {
    pub fn new(fire: Arc<dyn FireSchedule>) -> Self {
        Self {
            fire,
            fired: Mutex::new(HashMap::new()),
        }
    }

    /// Records the firing in the same step that decides it, so a concurrent event finds it taken.
    fn claim(&self, view: &ScheduleView) -> bool {
        let mut fired = self.fired.lock().unwrap_or_else(|e| e.into_inner());
        let item = &view.schedule.item_id;
        if fired.get(item) == Some(&view.schedule.armed_at) {
            return false;
        }
        fired.insert(item.clone(), view.schedule.armed_at.clone());
        true
    }

    /// Re-evaluated `views` arrived (an inbox event or a queue change): fire each ready one that is
    /// still armed, exactly once. Answers the item ids fired.
    pub async fn on_change(&self, views: &[ScheduleView]) -> Vec<String> {
        let mut fired = Vec::new();
        for view in views {
            if view.status != ConditionState::Met || !self.claim(view) {
                continue;
            }
            if let Err(why) = self.fire.fire(view).await {
                log::warn!(
                    "schedule for {} failed to fire: {why}",
                    view.schedule.item_id
                );
            }
            fired.push(view.schedule.item_id.clone());
        }
        fired
    }

    /// Whether `item_id`'s schedule is still armed (has not fired).
    pub fn is_armed(&self, item_id: &str) -> bool {
        !self
            .fired
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(item_id)
    }
}
