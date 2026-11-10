//! Acceptance: the usage tap appends exactly the protocol events its spec declares, verbatim, and a
//! tap that cannot write never fails the turn that feeds it (`#usage-binding 3/5`).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-live-tap.md

use pretty_assertions::assert_eq;
use tddy_agent_backend::usage_tap::{UsageTap, UsageTapSpec};

/// A tap writing to `path`, keeping the events that satisfy every predicate in `predicates`.
fn a_tap_for(path: std::path::PathBuf, predicates: serde_json::Value) -> UsageTap {
    UsageTap::open(UsageTapSpec {
        path,
        events: serde_json::from_value(predicates).expect("a predicate list"),
    })
}

fn the_tapped_lines(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn only_the_events_the_spec_declares_are_tapped() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage").join("conv-1.jsonl");
    let mut tap = a_tap_for(
        path.clone(),
        serde_json::json!([{ "field": "type", "equals": "result" }]),
    );

    // When
    tap.offer_line(r#"{"type":"assistant","message":{"id":"m1"}}"#);
    tap.offer_line(r#"{"type":"result","total_cost_usd":0.5}"#);
    tap.offer_line("not json at all");

    // Then
    assert_eq!(
        the_tapped_lines(&path),
        vec![r#"{"type":"result","total_cost_usd":0.5}"#.to_string()]
    );
}

#[test]
fn an_acp_usage_update_is_tapped() {
    // Given — an ACP `session/update` the driver holds as a value
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage").join("conv-1.jsonl");
    let mut tap = a_tap_for(
        path.clone(),
        serde_json::json!([{ "field": "sessionUpdate", "equals": "usage_update" }]),
    );
    let update = serde_json::json!({
        "sessionUpdate": "usage_update", "used": 53000, "size": 200000,
        "cost": { "amount": 0.045, "currency": "USD" }
    });

    // When
    tap.offer_value(&serde_json::json!({ "sessionUpdate": "agent_message_chunk" }));
    tap.offer_value(&update);

    // Then
    let tapped: Vec<serde_json::Value> = the_tapped_lines(&path)
        .iter()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    assert_eq!(tapped, vec![update]);
}

#[test]
fn a_tap_that_cannot_write_does_not_fail_the_turn() {
    // Given — the tap's directory is a file
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("usage"), "a file, not a directory").unwrap();
    let mut tap = a_tap_for(
        dir.path().join("usage").join("conv-1.jsonl"),
        serde_json::json!([{ "field": "type", "equals": "result" }]),
    );

    // When — offering events neither panics nor errors
    tap.offer_line(r#"{"type":"result"}"#);
    tap.offer_line(r#"{"type":"result"}"#);

    // Then — the file that stands in the way is untouched
    assert_eq!(
        std::fs::read_to_string(dir.path().join("usage")).unwrap(),
        "a file, not a directory"
    );
}
