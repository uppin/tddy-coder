//! The exec engine dispatches the five language-agnostic `Lsp*` tools to the registered
//! `LspExecutor`. Uses a fake executor (the process-global registry is set once per test
//! binary).

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};
use tddy_core::toolcall::lsp::{register_lsp_executor, LspExecutor, LspQuery};
use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool;

struct EchoExecutor;

impl LspExecutor for EchoExecutor {
    fn is_available(&self, _repo: &Path) -> bool {
        true
    }
    fn diagnostics(&self, _repo: &Path, query: &LspQuery) -> Result<Value, String> {
        Ok(json!({ "op": "diagnostics", "target": query.target }))
    }
    fn definition(&self, _repo: &Path, query: &LspQuery) -> Result<Value, String> {
        Ok(json!({
            "op": "definition",
            "target": query.target,
            "file": query.file,
            "line": query.line,
            "character": query.character,
        }))
    }
    fn references(&self, _repo: &Path, _query: &LspQuery) -> Result<Value, String> {
        Ok(json!({ "op": "references" }))
    }
    fn hover(&self, _repo: &Path, _query: &LspQuery) -> Result<Value, String> {
        Ok(json!({ "op": "hover" }))
    }
    fn symbols(&self, _repo: &Path, _query: &LspQuery) -> Result<Value, String> {
        Ok(json!({ "op": "symbols" }))
    }
    fn workspace_diagnostics(&self, _repo: &Path) -> Result<Value, String> {
        Ok(json!({ "lints": [{ "uri": "file:///workspace/src/lib.rs", "message": "unused" }] }))
    }
}

#[tokio::test]
async fn dispatches_the_lsp_definition_tool_to_the_registered_executor() {
    // Given a registered LSP executor
    register_lsp_executor(Arc::new(EchoExecutor));
    let registry = TaskRegistry::new();

    // When an agent invokes the LspDefinition tool
    let outcome = execute_tool(
        Path::new("/tmp"),
        "LspDefinition",
        r#"{"target":"app:bin","file":"src/lib.rs","line":10,"character":4}"#,
        &registry,
        "session-1",
    )
    .await;

    // Then the call reaches the executor and its JSON result is relayed back verbatim
    assert!(
        !outcome.is_error,
        "unexpected error: {}",
        outcome.error_message
    );
    let value: Value = serde_json::from_str(&outcome.result_json).unwrap();
    assert_eq!(
        value,
        json!({
            "op": "definition",
            "target": "app:bin",
            "file": "src/lib.rs",
            "line": 10,
            "character": 4,
        })
    );
}

#[tokio::test]
async fn read_lints_routes_to_the_registered_executors_workspace_diagnostics() {
    // Given a registered LSP executor available for the repo
    register_lsp_executor(Arc::new(EchoExecutor));
    let registry = TaskRegistry::new();

    // When ReadLints is invoked
    let outcome = execute_tool(Path::new("/tmp"), "ReadLints", "{}", &registry, "session-1").await;

    // Then it returns the executor's workspace diagnostics, not the no-linter stub
    assert!(
        !outcome.is_error,
        "unexpected error: {}",
        outcome.error_message
    );
    let value: Value = serde_json::from_str(&outcome.result_json).unwrap();
    assert_eq!(
        value,
        json!({ "lints": [{ "uri": "file:///workspace/src/lib.rs", "message": "unused" }] })
    );
}
