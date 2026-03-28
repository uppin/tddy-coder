//! Lower-level RED tests: each workflow goal must have a concrete `.proto` file (PRD F1).

use tddy_workflow_recipes::schema_pipeline;

#[test]
fn each_expected_proto_file_exists_under_proto_root() {
    let root = schema_pipeline::proto_root();
    assert!(
        root.is_dir(),
        "expected proto root directory at {}",
        root.display()
    );
    for name in schema_pipeline::expected_proto_basenames() {
        let p = root.join(name);
        assert!(
            p.is_file(),
            "missing workflow proto {:?} (define message types for this goal)",
            p
        );
    }
}

#[test]
fn generated_schema_manifest_path_exists_after_pipeline() {
    let p = schema_pipeline::generated_manifest_path();
    assert!(
        p.is_file(),
        "expected generated manifest at {} (PRD F2)",
        p.display()
    );
}
