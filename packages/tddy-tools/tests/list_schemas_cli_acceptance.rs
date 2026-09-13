//! `list-schemas` dispatch: the `tddy-tools` CLI must advertise every registered workflow goal.
//!
//! The schema library moved to `tddy-workflow-recipes`; what the CLI does with it did not, so this
//! test stays with the binary it drives.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

use tddy_workflow_recipes::schema::{get_schema, validate_output};

/// PRD: `list-schemas` / `get-schema` include **branch-review**; minimal payload validates (post-green-review-style fields).
#[test]
fn tddy_tools_lists_branch_review_goal() {
    fn tddy_tools_bin() -> Command {
        let mut cmd = cargo_bin_cmd!("tddy-tools");
        cmd.env_remove("TDDY_SOCKET");
        cmd
    }

    // When
    let mut cmd = tddy_tools_bin();
    cmd.args(["list-schemas"]);
    let out = cmd.output().expect("list-schemas");

    // Then
    assert!(
        out.status.success(),
        "list-schemas must exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("list-schemas stdout must be JSON");
    let goals = v
        .get("goals")
        .and_then(|g| g.as_array())
        .expect("goals array");
    assert!(
        goals
            .iter()
            .filter_map(|x| x.as_str())
            .any(|n| n == "branch-review"),
        "list-schemas must include branch-review; got {:?}",
        goals
    );

    let schema = get_schema("branch-review").expect("branch-review schema must be embedded");
    let snippet: String = schema.chars().take(220).collect();
    assert!(
        schema.contains("branch-review") || schema.contains("Branch"),
        "branch-review schema should describe the goal; got snippet: {}",
        snippet
    );
    let minimal = serde_json::json!({
        "goal": "branch-review",
        "summary": "Branch review complete.",
        "validity_assessment": "ok",
        "review_body_markdown": "# Branch review\n\n## Findings\n- Observed issue in module X."
    })
    .to_string();
    assert!(
        validate_output("branch-review", &minimal).is_ok(),
        "minimal branch-review payload must validate; implement schema aligned with post-green-review-style conventions"
    );
}
