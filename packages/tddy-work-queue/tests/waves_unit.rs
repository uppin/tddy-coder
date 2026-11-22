//! Waves, the derived line and readiness (`#workqueue` 10/14). The main fixture is the planning rule's
//! own example: n1 → n2, n3, n4, n5; n2 → n4; n5 → n6, n7, n8 (`a → b`: b needs a).

use pretty_assertions::assert_eq;
use tddy_core::changeset::{GithubPrStatus, PrInternalStatus, WorkItem, WorkQueue};
use tddy_work_queue::lineage::{DependencyEdge, LineageError};
use tddy_work_queue::waves::{
    derive_line, line_check, merged_items, queue_waves, readiness, transitive_dependents, waves,
    LineCheck,
};

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

fn edges(pairs: &[(&str, &str)]) -> Vec<DependencyEdge> {
    pairs
        .iter()
        .map(|(from, to)| DependencyEdge {
            from: from.to_string(),
            to: to.to_string(),
        })
        .collect()
}

fn the_example() -> (Vec<String>, Vec<DependencyEdge>) {
    (
        ids(&["n1", "n2", "n3", "n4", "n5", "n6", "n7", "n8"]),
        edges(&[
            ("n1", "n2"),
            ("n1", "n3"),
            ("n1", "n4"),
            ("n1", "n5"),
            ("n2", "n4"),
            ("n5", "n6"),
            ("n5", "n7"),
            ("n5", "n8"),
        ]),
    )
}

#[test]
fn an_item_with_no_needs_is_wave_one_wherever_it_sits() {
    // Given — c needs a; b, last in the queue, needs nothing
    let items = ids(&["a", "c", "b"]);

    // When
    let waves = waves(&items, &edges(&[("a", "c")])).expect("waves");

    // Then
    assert_eq!(waves, vec![ids(&["a", "b"]), ids(&["c"])]);
}

#[test]
fn inside_a_wave_the_item_with_most_transitive_dependents_leads() {
    // Given
    let (items, edges) = the_example();

    // When
    let line = derive_line(&items, &edges).expect("line");

    // Then — n5 (3 dependents) before n2 (1) before n3 (0); ties keep queue order
    assert_eq!(line, ids(&["n1", "n5", "n2", "n3", "n4", "n6", "n7", "n8"]));
    assert_eq!(
        transitive_dependents(&edges, "n5"),
        ids(&["n6", "n7", "n8"])
    );
}

#[test]
fn ties_keep_the_queues_order() {
    assert_eq!(
        derive_line(&ids(&["x", "y", "z"]), &[]).expect("line"),
        ids(&["x", "y", "z"])
    );
}

#[test]
fn readiness_names_only_needs_in_the_wave_immediately_below() {
    // Given — n4 needs n1 (wave 1) and n2 (wave 2)
    let (items, edges) = the_example();

    // When
    let waiting = readiness(&items, &edges, "n4", &[]).expect("readiness");
    let ready = readiness(&items, &edges, "n4", &ids(&["n2"])).expect("readiness");

    // Then
    assert_eq!((waiting, ready), (ids(&["n2"]), Vec::<String>::new()));
}

#[test]
fn a_line_that_steps_back_a_wave_is_out_of_order_at_its_first_offender() {
    // Given — n4 (wave 3) placed before n3 (wave 2)
    let (items, edges) = the_example();
    let line = ids(&["n1", "n2", "n4", "n3", "n5", "n6", "n7", "n8"]);

    // When
    let checked = line_check(&items, &edges, &line).expect("checked");

    // Then
    assert_eq!(
        checked,
        LineCheck::OutOfOrder {
            first: "n3".to_string()
        }
    );
    assert_eq!(
        line_check(&items, &edges, &derive_line(&items, &edges).unwrap()).unwrap(),
        LineCheck::InOrder
    );
}

#[test]
fn a_cycle_reports_the_cycle_instead_of_waves() {
    // When
    let result = waves(&ids(&["a", "b"]), &edges(&[("a", "b"), ("b", "a")]));

    // Then
    assert!(matches!(result, Err(LineageError::Cycle(_))), "{result:?}");
}

#[test]
fn a_landed_need_is_met_so_the_item_no_longer_waits_on_it() {
    // Given — n4 needs n1 and n2; n2 has merged
    let (items, edges) = the_example();

    // When
    let derived = queue_waves(&items, &edges, &ids(&["n2"])).expect("queue waves");
    let n4 = derived
        .readiness
        .iter()
        .find(|(item, _)| item == "n4")
        .map(|(_, waits_on)| waits_on.clone());

    // Then — met needs leave readiness; the waves themselves stay structural
    assert_eq!(
        (n4, derived.waves[1].clone()),
        (Some(Vec::<String>::new()), ids(&["n2", "n3", "n5"]))
    );
}

#[test]
fn an_unlanded_need_still_blocks() {
    // Given — nothing merged
    let (items, edges) = the_example();

    // When
    let derived = queue_waves(&items, &edges, &[]).expect("queue waves");
    let n4 = derived.readiness.iter().find(|(item, _)| item == "n4");

    // Then
    assert_eq!(n4, Some(&("n4".to_string(), ids(&["n2"]))));
}

fn item(id: &str) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        title: id.to_string(),
        ..Default::default()
    }
}

#[test]
fn an_item_whose_stored_pr_or_status_says_merged_has_landed() {
    // Given — n1's PR merged, n2's derived status is merged, n3 is open, n4 has neither
    let mut queue = WorkQueue::default();
    let mut n1 = item("n1");
    n1.pr_status = Some(GithubPrStatus {
        phase: "merged".to_string(),
        ..Default::default()
    });
    let mut n2 = item("n2");
    n2.internal_status = Some(PrInternalStatus {
        kind: "merged".to_string(),
        note: None,
        source: "derived".to_string(),
    });
    let mut n3 = item("n3");
    n3.pr_status = Some(GithubPrStatus {
        phase: "open".to_string(),
        ..Default::default()
    });
    queue.nodes = vec![n1, n2, n3, item("n4")];

    // When / Then
    assert_eq!(merged_items(&queue), ids(&["n1", "n2"]));
}
