//! Armed merges and propagations, evaluated (`#workqueue` 13/14).

use pretty_assertions::assert_eq;
use tddy_work_queue::schedule::{
    evaluate, schedule_wave, ChecksVerdict, ConditionState, ReviewVerdict, Schedule, ScheduleKind,
    ScheduleSnapshot, WorktreeVerdict,
};

fn a_merge(skip_review: bool) -> Schedule {
    Schedule {
        item_id: "n2".to_string(),
        kind: ScheduleKind::Merge { skip_review },
        armed_at: "2026-11-22T12:00:00Z".to_string(),
        armed_by: "octocat".to_string(),
    }
}

fn a_propagation() -> Schedule {
    Schedule {
        item_id: "n2".to_string(),
        kind: ScheduleKind::Propagate,
        armed_at: "2026-11-22T12:00:00Z".to_string(),
        armed_by: "octocat".to_string(),
    }
}

/// Everything a merge needs, met.
fn all_met() -> ScheduleSnapshot {
    ScheduleSnapshot {
        base_is_trunk: true,
        inbox_clean: 0,
        inbox_conflicting: 0,
        own_checks: ChecksVerdict::Green,
        base_checks: ChecksVerdict::Green,
        review: ReviewVerdict::Approved,
        worktree: WorktreeVerdict::Clean,
    }
}

fn state_of(schedule: &Schedule, snapshot: &ScheduleSnapshot, condition: &str) -> ConditionState {
    evaluate(schedule, snapshot)
        .conditions
        .into_iter()
        .find(|c| c.name == condition)
        .unwrap_or_else(|| panic!("a {condition} condition"))
        .state
}

#[test]
fn a_merge_on_an_item_based_on_an_open_item_waits_on_order() {
    let snapshot = ScheduleSnapshot {
        base_is_trunk: false,
        ..all_met()
    };
    assert_eq!(
        state_of(&a_merge(false), &snapshot, "order"),
        ConditionState::Waiting
    );
}

#[test]
fn a_clean_inbox_entry_makes_current_wait_and_a_predicted_conflict_blocks_it() {
    let clean = ScheduleSnapshot {
        inbox_clean: 1,
        ..all_met()
    };
    let conflicting = ScheduleSnapshot {
        inbox_clean: 1,
        inbox_conflicting: 1,
        ..all_met()
    };
    assert_eq!(
        (
            state_of(&a_merge(false), &clean, "current"),
            state_of(&a_merge(false), &conflicting, "current"),
        ),
        (ConditionState::Waiting, ConditionState::Blocked)
    );
}

#[test]
fn a_propagations_checks_condition_reads_the_bases_ci() {
    // Given — the item's own CI failed, the base's is green, and there is something to take
    let snapshot = ScheduleSnapshot {
        inbox_clean: 1,
        own_checks: ChecksVerdict::Failed,
        base_checks: ChecksVerdict::Green,
        ..all_met()
    };

    // Then
    assert_eq!(
        state_of(&a_propagation(), &snapshot, "checks"),
        ConditionState::Met
    );
}

#[test]
fn a_change_request_blocks_review_even_with_skip_review() {
    let snapshot = ScheduleSnapshot {
        review: ReviewVerdict::ChangesRequested,
        ..all_met()
    };
    assert_eq!(
        state_of(&a_merge(true), &snapshot, "review"),
        ConditionState::Blocked
    );
}

#[test]
fn a_dirty_or_unpushed_worktree_blocks_propagation() {
    let dirty = ScheduleSnapshot {
        inbox_clean: 1,
        worktree: WorktreeVerdict::Dirty,
        ..all_met()
    };
    let unpushed = ScheduleSnapshot {
        inbox_clean: 1,
        worktree: WorktreeVerdict::Unpushed,
        ..all_met()
    };
    assert_eq!(
        (
            state_of(&a_propagation(), &dirty, "worktree"),
            state_of(&a_propagation(), &unpushed, "worktree"),
        ),
        (ConditionState::Blocked, ConditionState::Blocked)
    );
}

#[test]
fn the_status_is_the_worst_condition_and_held_up_on_prefers_blocked() {
    // Given — order waits (first), review is blocked (later)
    let snapshot = ScheduleSnapshot {
        base_is_trunk: false,
        review: ReviewVerdict::ChangesRequested,
        ..all_met()
    };

    // When
    let view = evaluate(&a_merge(false), &snapshot);

    // Then
    assert_eq!(
        (view.status, view.held_up_on),
        (ConditionState::Blocked, Some("review"))
    );
}

#[test]
fn a_fully_met_merge_is_ready_with_its_conditions_in_render_order() {
    // When
    let view = evaluate(&a_merge(false), &all_met());

    // Then
    assert_eq!(
        (
            view.status,
            view.held_up_on,
            view.conditions.iter().map(|c| c.name).collect::<Vec<_>>()
        ),
        (
            ConditionState::Met,
            None,
            vec!["order", "current", "checks", "review"]
        )
    );
}

#[test]
fn arming_a_wave_arms_a_merge_for_each_open_prd_item_bottom_up() {
    // When — n3 has no open PR
    let armed = schedule_wave(
        &[
            ("n1".to_string(), true),
            ("n3".to_string(), false),
            ("n2".to_string(), true),
        ],
        false,
        "octocat",
    );

    // Then
    assert_eq!(
        armed
            .iter()
            .map(|s| (s.item_id.as_str(), s.kind.clone(), s.armed_at.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("n1", ScheduleKind::Merge { skip_review: false }, ""),
            ("n2", ScheduleKind::Merge { skip_review: false }, ""),
        ]
    );
}
