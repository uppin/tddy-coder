//! PRD: `changeset-workflow` JSON Schema still accepts representative payloads after GitHub tools metadata lands.

use tddy_tools::schema::validate_output;

/// Representative persist payload including optional GitHub PR tool routing metadata (PRD §6).
const CHANGESET_WORKFLOW_WITH_GITHUB_TOOLS: &str = r#"{
  "run_optional_step_x": true,
  "demo_options": ["smoke"],
  "tool_schema_id": "urn:tddy:tool/changeset-workflow",
  "github_pr_tools_metadata": {
    "mcp_tool_names": ["github_create_pull_request", "github_update_pull_request"]
  }
}"#;

#[test]
fn persist_changeset_workflow_still_validates_after_github_tools_change() {
    validate_output("changeset-workflow", CHANGESET_WORKFLOW_WITH_GITHUB_TOOLS).unwrap_or_else(
        |e| {
            panic!(
                "persist-changeset-workflow must accept github_pr_tools_metadata alongside existing fields: {e:?}"
            )
        },
    );
}
