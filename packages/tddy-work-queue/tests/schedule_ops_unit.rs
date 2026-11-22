//! Arming, disarming and the check/review verdicts (`#workqueue` 13/14).

use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_work_queue::schedule::{
    arm, arm_wave, checks_verdict, disarm, review_verdict, ChecksVerdict, ReviewVerdict,
    RunOutcome, ScheduleError, ScheduleKind,
};

fn a_queue_of(ids: &[&str]) -> WorkQueue {
    WorkQueue {
        nodes: ids
            .iter()
            .map(|id| WorkItem {
                node_id: id.to_string(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn arming_an_item_twice_keeps_one_schedule_with_the_latest_kind() {
    let mut queue = a_queue_of(&["n1"]);
    arm(&mut queue, "n1", ScheduleKind::Propagate, "t1", "octocat").unwrap();
    arm(
        &mut queue,
        "n1",
        ScheduleKind::Merge { skip_review: true },
        "t2",
        "octocat",
    )
    .unwrap();
    assert_eq!(
        queue
            .schedules
            .iter()
            .map(|s| (s.kind.clone(), s.armed_at.as_str()))
            .collect::<Vec<_>>(),
        vec![(ScheduleKind::Merge { skip_review: true }, "t2")]
    );
}

#[test]
fn arming_or_disarming_what_does_not_exist_is_refused() {
    let mut queue = a_queue_of(&["n1"]);
    assert_eq!(
        (
            arm(&mut queue, "n9", ScheduleKind::Propagate, "", "").unwrap_err(),
            disarm(&mut queue, "n1").unwrap_err(),
        ),
        (
            ScheduleError::NoSuchItem("n9".to_string()),
            ScheduleError::NotArmed("n1".to_string())
        )
    );
}

#[test]
fn arming_a_wave_stamps_each_open_item_and_leaves_the_rest_unarmed() {
    let mut queue = a_queue_of(&["n1", "n2", "n3"]);
    let armed = arm_wave(
        &mut queue,
        &["n1".to_string(), "n2".to_string(), "n3".to_string()],
        |id| id != "n2",
        false,
        "t",
        "octocat",
    )
    .unwrap();
    assert_eq!(
        (
            armed.iter().map(|s| s.item_id.as_str()).collect::<Vec<_>>(),
            queue.schedules.len()
        ),
        (vec!["n1", "n3"], 2)
    );
}

#[test]
fn no_check_runs_is_pending_and_a_failure_outranks_a_running_run() {
    assert_eq!(
        (
            checks_verdict(&[]),
            checks_verdict(&[RunOutcome::Passed, RunOutcome::Running]),
            checks_verdict(&[RunOutcome::Failed, RunOutcome::Running]),
            checks_verdict(&[RunOutcome::Passed]),
        ),
        (
            ChecksVerdict::Pending,
            ChecksVerdict::Pending,
            ChecksVerdict::Failed,
            ChecksVerdict::Green
        )
    );
}

#[test]
fn a_reviewers_latest_verdict_counts_and_a_dismissal_withdraws_it() {
    assert_eq!(
        (
            review_verdict([("a", "CHANGES_REQUESTED"), ("a", "APPROVED")]),
            review_verdict([("a", "APPROVED"), ("b", "CHANGES_REQUESTED")]),
            review_verdict([("a", "CHANGES_REQUESTED"), ("a", "DISMISSED")]),
            review_verdict([("a", "COMMENTED")]),
        ),
        (
            ReviewVerdict::Approved,
            ReviewVerdict::ChangesRequested,
            ReviewVerdict::Pending,
            ReviewVerdict::Pending
        )
    );
}
