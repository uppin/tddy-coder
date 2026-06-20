//! Acceptance tests (PRD Testing Plan): changeset workflow/demo persistence and context merge.
//! Acceptance: workflow block round-trip and merge into session `Context`.

use std::fs;

use serde_yaml::Value;
use tddy_core::changeset::{
    merge_persisted_workflow_into_context, read_changeset, write_changeset, Changeset,
};
use tddy_core::workflow::context::Context;
use tddy_testing_commons::fs::temp_session_dir;

/// PRD: round-trip — `workflow` block in YAML must survive load/save once serde schema exists.
#[test]
fn changeset_round_trips_workflow_demo_fields() {
    // Given
    let dir = temp_session_dir("changeset-demo-roundtrip");
    write_changeset(&dir, &Changeset::default()).expect("seed");
    let path = dir.join("changeset.yaml");
    let mut doc: Value = serde_yaml::from_str(&fs::read_to_string(&path).expect("read"))
        .expect("parse changeset yaml");
    doc["workflow"] = serde_yaml::from_str(
        r"
run_optional_step_x: true
demo_options:
  - script-based
tool_schema_id: urn:tddy:changeset-workflow
",
    )
    .expect("workflow fragment");
    fs::write(&path, serde_yaml::to_string(&doc).expect("serialize")).expect("write");

    // When
    let cs = read_changeset(&dir).expect("read changeset");
    write_changeset(&dir, &cs).expect("write round-trip");

    // Then
    let before = fs::read_to_string(&path).expect("read before");
    assert!(
        before.contains("workflow:"),
        "fixture must include workflow block; got:\n{before}"
    );
    assert!(
        cs.workflow.is_some(),
        "changeset must deserialize workflow block; got workflow={:?}",
        cs.workflow
    );
    assert_eq!(
        cs.workflow
            .as_ref()
            .and_then(|w| w.tool_schema_id.as_deref()),
        Some("urn:tddy:changeset-workflow"),
        "expected tool_schema_id on changeset workflow once persistence is implemented"
    );
    let after = fs::read_to_string(&path).expect("read after");
    assert!(
        after.contains("workflow:") && after.contains("run_optional_step_x"),
        "round-trip must preserve workflow/demo keys on disk; after write:\n{after}"
    );
}

/// PRD: engine merge — persisted boolean must appear in Context before graph `next` after green.
#[test]
fn context_loads_persisted_demo_flag_at_workflow_start() {
    // Given
    let dir = temp_session_dir("changeset-demo-merge");
    write_changeset(&dir, &Changeset::default()).expect("seed");
    let path = dir.join("changeset.yaml");
    let mut doc: Value =
        serde_yaml::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
    doc["workflow"] =
        serde_yaml::from_str("run_optional_step_x: true\ndemo_options: []\n").expect("workflow");
    fs::write(&path, serde_yaml::to_string(&doc).expect("serialize")).expect("write");
    let ctx = Context::new();

    // When
    merge_persisted_workflow_into_context(&dir, &ctx).expect("merge");

    // Then
    assert_eq!(
        ctx.get_sync::<bool>("run_optional_step_x"),
        Some(true),
        "Context must receive run_optional_step_x from changeset workflow block"
    );
}
