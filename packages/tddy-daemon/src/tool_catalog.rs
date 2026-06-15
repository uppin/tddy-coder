//! Static tool catalog for remote-codebase mode workspace sessions.
//!
//! Each entry corresponds to one tool the daemon can dispatch via `ExecuteTool`.

use tddy_service::proto::connection::ToolDef;

/// Returns the full tool catalog for workspace sessions.
pub fn tool_catalog() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "Read".to_string(),
            description: "Read file contents from the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}}}"#.to_string(),
        },
        ToolDef {
            name: "Write".to_string(),
            description: "Write file contents to the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path","contents"],"properties":{"path":{"type":"string"},"contents":{"type":"string"}}}"#.to_string(),
        },
        ToolDef {
            name: "StrReplace".to_string(),
            description: "Replace a string in a file.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path","old_string","new_string"],"properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}}}"#.to_string(),
        },
        ToolDef {
            name: "Delete".to_string(),
            description: "Delete a file from the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#.to_string(),
        },
        ToolDef {
            name: "Grep".to_string(),
            description: "Search for a pattern in files.".to_string(),
            input_schema_json: r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string"},"path":{"type":"string"},"include":{"type":"string"}}}"#.to_string(),
        },
        ToolDef {
            name: "Glob".to_string(),
            description: "Find files matching a glob pattern.".to_string(),
            input_schema_json: r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string"}}}"#.to_string(),
        },
        ToolDef {
            name: "Shell".to_string(),
            description: "Run a shell command in the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","required":["command"],"properties":{"command":{"type":"string"},"block_until_ms":{"type":"integer"}}}"#.to_string(),
        },
        ToolDef {
            name: "Await".to_string(),
            description: "Wait for a background shell job to complete.".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"job_id":{"type":"string"},"task_id":{"type":"string"},"timeout_ms":{"type":"integer"},"block_until_ms":{"type":"integer"}}}"#.to_string(),
        },
        ToolDef {
            name: "ReadLints".to_string(),
            description: "Read linting diagnostics for the workspace.".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#.to_string(),
        },
        ToolDef {
            name: "SemanticSearch".to_string(),
            description: "Search the codebase semantically.".to_string(),
            input_schema_json: r#"{"type":"object","required":["query"],"properties":{"query":{"type":"string"},"path":{"type":"string"}}}"#.to_string(),
        },
    ]
}
