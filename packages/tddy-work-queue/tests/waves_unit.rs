//! Waves, the derived line and readiness (`#workqueue` 10/14). The main fixture is the planning rule's
//! own example: n1 → n2, n3, n4, n5; n2 → n4; n5 → n6, n7, n8 (`a → b`: b needs a).

use pretty_assertions::assert_eq;
use tddy_pr_stack::lineage::{DependencyEdge, LineageError};
use tddy_pr_stack::waves::{
    derive_line, line_check, readiness, transitive_dependents, waves, LineCheck,
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
