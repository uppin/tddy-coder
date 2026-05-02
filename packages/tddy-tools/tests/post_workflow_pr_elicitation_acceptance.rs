//! Acceptance (PRD): persist post-workflow PR + worktree fields via `persist-changeset-workflow`,
//! validate against `changeset-workflow`, merge into session `Context`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tddy_core::changeset::{merge_persisted_workflow_into_context, write_changeset, Changeset};
use tddy_core::workflow::context::Context;
use tddy_tools::schema::validate_output;

/// Canonical extended payload: PR intent, conditional worktree removal, machine-readable PR status.
const POST_PR_WORKFLOW_JSON: &str = r#"{
  "run_optional_step_x": false,
  "demo_options": [],
  "tool_schema_id": "urn:tddy:tool/changeset-workflow",
  "post_workflow_open_github_pr": true,
  "post_workflow_remove_session_worktree": false,
  "github_pr_status": {
    "phase": "published",
    "url": "https://github.com/example/repo/pull/7",
    "error": null
  }
}"#;

fn temp_session_dir(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("tddy-post-pr-wf-{}-{}", label, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn persist_and_merge_post_pr_workflow_fields() {
    assert!(
        validate_output("changeset-workflow", POST_PR_WORKFLOW_JSON).is_ok(),
        "extended post-PR workflow JSON must validate against changeset-workflow schema; payload:\n{POST_PR_WORKFLOW_JSON}"
    );

    let dir = temp_session_dir("persist-merge");
    write_changeset(&dir, &Changeset::default()).expect("seed changeset");

    let status = Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .args([
            "persist-changeset-workflow",
            "--session-dir",
            dir.to_str().expect("utf8 path"),
            "--data",
            POST_PR_WORKFLOW_JSON,
        ])
        .status()
        .expect("spawn tddy-tools");
    assert!(
        status.success(),
        "persist-changeset-workflow must exit 0 for post-PR workflow JSON; code={:?}",
        status.code()
    );

    let raw = fs::read_to_string(dir.join("changeset.yaml")).expect("read changeset.yaml");
    for fragment in [
        "post_workflow_open_github_pr",
        "post_workflow_remove_session_worktree",
        "github_pr_status",
        "published",
        "pull/7",
    ] {
        assert!(
            raw.contains(fragment),
            "changeset.yaml must round-trip post-PR workflow fields; missing {fragment:?} in:\n{raw}"
        );
    }

    let ctx = Context::new();
    merge_persisted_workflow_into_context(&dir, &ctx).expect("merge workflow into context");
    assert_eq!(
        ctx.get_sync::<bool>("post_workflow_open_github_pr"),
        Some(true),
        "Context must expose post_workflow_open_github_pr after merge"
    );
    assert_eq!(
        ctx.get_sync::<bool>("post_workflow_remove_session_worktree"),
        Some(false),
        "Context must expose post_workflow_remove_session_worktree after merge"
    );
    let status_val = ctx
        .get_sync::<serde_json::Value>("github_pr_status")
        .expect("Context must expose github_pr_status object after merge");
    assert_eq!(
        status_val.get("phase").and_then(|v| v.as_str()),
        Some("published")
    );
    assert_eq!(
        status_val.get("url").and_then(|v| v.as_str()),
        Some("https://github.com/example/repo/pull/7")
    );

    let _ = fs::remove_dir_all(&dir);
}
