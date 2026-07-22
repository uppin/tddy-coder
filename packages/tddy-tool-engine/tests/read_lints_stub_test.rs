//! ReadLints falls back to the no-linter stub when no language-server executor is
//! registered in the process (separate test binary so the global registry is empty).

use std::path::Path;

use serde_json::Value;
use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool;

#[tokio::test]
async fn read_lints_returns_the_stub_when_no_executor_is_registered() {
    // Given no LSP executor registered in this process
    let registry = TaskRegistry::new();

    // When ReadLints is invoked
    let outcome = execute_tool(Path::new("/tmp"), "ReadLints", "{}", &registry, "session-1").await;

    // Then it returns the empty no-linter stub rather than erroring
    assert!(
        !outcome.is_error,
        "unexpected error: {}",
        outcome.error_message
    );
    let value: Value = serde_json::from_str(&outcome.result_json).unwrap();
    assert_eq!(value["lints"], serde_json::json!([]));
    assert!(value["note"]
        .as_str()
        .unwrap_or_default()
        .contains("no linter configured"));
}
