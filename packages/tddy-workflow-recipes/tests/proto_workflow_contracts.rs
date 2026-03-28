//! F1 — workflow goal outputs must be defined as `.proto` under this package (single source of truth).

use std::path::Path;

#[test]
fn proto_definitions_exist_for_workflow_goals() {
    let proto_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("proto");
    assert!(
        proto_root.is_dir(),
        "expected packages/tddy-workflow-recipes/proto/ containing workflow goal messages (PRD F1)"
    );
}
