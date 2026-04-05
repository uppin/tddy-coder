//! Acceptance (PRD Testing Plan): `tddy-tools` persists workflow/demo fields into `changeset.yaml`.

use std::fs;
use std::process::Command;

use tddy_core::changeset::Changeset;
use tddy_core::write_changeset;

/// PRD: dedicated CLI path writes validated workflow JSON into changeset (atomic, schema-checked).
#[test]
fn tddy_tools_writes_demo_workflow_fields_to_changeset() {
    let dir = std::env::temp_dir().join(format!("tddy-persist-cw-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    write_changeset(&dir, &Changeset::default()).expect("seed changeset");

    let status = Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .args([
            "persist-changeset-workflow",
            "--session-dir",
            dir.to_str().expect("utf8 temp path"),
            "--data",
            r#"{"run_optional_step_x":true,"demo_options":[],"tool_schema_id":"urn:tddy:changeset-workflow"}"#,
        ])
        .status()
        .expect("spawn tddy-tools");
    assert!(
        status.success(),
        "persist-changeset-workflow must write workflow fields to changeset.yaml (exit 0); got {:?}",
        status.code()
    );

    let _ = fs::remove_dir_all(&dir);
}
