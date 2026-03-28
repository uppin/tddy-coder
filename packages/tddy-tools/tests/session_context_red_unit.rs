//! Lower-level Red tests for `session_context::apply_session_context_merge` (library API).

use serde_json::json;
use tddy_tools::session_context::apply_session_context_merge;
use tempfile::tempdir;

#[test]
fn apply_session_context_merge_completes_without_error_when_session_exists() {
    let dir = tempdir().expect("tempdir");
    let wf = dir.path().join(".workflow");
    std::fs::create_dir_all(&wf).expect("mkdir");
    let session_id = "unit-sess-1";
    let initial = format!(
        r#"{{"id":"{session_id}","graph_id":"tdd_full_workflow","current_task_id":"green","status_message":null,"context":{{}}}}"#
    );
    std::fs::write(wf.join(format!("{session_id}.session.json")), initial).expect("write");

    let patch = json!({"run_optional_step_x": true});
    let result = apply_session_context_merge(&wf, session_id, &patch);
    assert!(
        result.is_ok(),
        "merge must succeed and persist; got {:?}",
        result
    );
}
