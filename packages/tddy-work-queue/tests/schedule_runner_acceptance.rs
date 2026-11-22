//! Armed schedules fire once (`#workqueue` 14/14).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_work_queue::schedule::{ConditionState, Schedule, ScheduleKind, ScheduleView};
use tddy_work_queue::schedule_runner::{FireSchedule, ScheduleRunner};

#[derive(Default)]
struct ACountingFire {
    fired: AtomicU32,
}

#[async_trait]
impl FireSchedule for ACountingFire {
    async fn fire(&self, _view: &ScheduleView) -> Result<(), String> {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        self.fired.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn a_merge_view(status: ConditionState) -> ScheduleView {
    ScheduleView {
        schedule: Schedule {
            item_id: "n2".to_string(),
            kind: ScheduleKind::Merge { skip_review: false },
            armed_at: "2026-11-22T12:00:00Z".to_string(),
            armed_by: "octocat".to_string(),
        },
        conditions: Vec::new(),
        status,
        held_up_on: None,
    }
}

#[tokio::test]
async fn an_armed_merge_fires_once_when_its_last_condition_turns_met_and_disarms() {
    // Given
    let fire = Arc::new(ACountingFire::default());
    let runner = ScheduleRunner::new(fire.clone());

    // When — waiting, then met, then met again
    let while_waiting = runner
        .on_change(&[a_merge_view(ConditionState::Waiting)])
        .await;
    let when_met = runner.on_change(&[a_merge_view(ConditionState::Met)]).await;
    let again = runner.on_change(&[a_merge_view(ConditionState::Met)]).await;

    // Then
    assert_eq!(
        (
            while_waiting,
            when_met,
            again,
            fire.fired.load(Ordering::SeqCst),
            runner.is_armed("n2")
        ),
        (vec![], vec!["n2".to_string()], vec![], 1, false)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_events_fire_a_schedule_exactly_once() {
    // Given
    let fire = Arc::new(ACountingFire::default());
    let runner = Arc::new(ScheduleRunner::new(fire.clone()));

    // When — five events land at once
    let mut events = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let runner = runner.clone();
        events.spawn(async move { runner.on_change(&[a_merge_view(ConditionState::Met)]).await });
    }
    events.join_all().await;

    // Then
    assert_eq!(fire.fired.load(Ordering::SeqCst), 1);
}
