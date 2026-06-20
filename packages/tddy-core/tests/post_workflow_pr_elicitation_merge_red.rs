//! Integration: `merge_persisted_workflow_into_context` mirrors post-workflow fields into [`Context`] (no CLI).

use serde_json::json;
use tddy_core::changeset::{
    merge_persisted_workflow_into_context, write_changeset, Changeset, ChangesetWorkflow,
    GithubPrStatus,
};
use tddy_core::workflow::context::Context;
use tddy_testing_commons::fs::temp_session_dir;

#[test]
fn merge_persisted_workflow_writes_post_github_pr_fields_to_context() {
    // Given
    let dir = temp_session_dir("post-pr-merge-red");
    let cs = Changeset {
        workflow: Some(ChangesetWorkflow {
            run_optional_step_x: Some(false),
            demo_options: vec![],
            tool_schema_id: Some("urn:tddy:tool/changeset-workflow".into()),
            post_workflow_open_github_pr: Some(true),
            post_workflow_remove_session_worktree: Some(false),
            github_pr_status: Some(GithubPrStatus {
                phase: "published".into(),
                url: Some("https://github.com/example/repo/pull/7".into()),
                error: None,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    write_changeset(&dir, &cs).unwrap();
    let ctx = Context::new();

    // When
    merge_persisted_workflow_into_context(&dir, &ctx).expect("merge");

    // Then
    assert_eq!(
        ctx.get_sync::<bool>("post_workflow_open_github_pr"),
        Some(true),
        "Context must expose post_workflow_open_github_pr"
    );
    assert_eq!(
        ctx.get_sync::<bool>("post_workflow_remove_session_worktree"),
        Some(false),
        "Context must expose post_workflow_remove_session_worktree"
    );
    let v = ctx
        .get_sync::<serde_json::Value>("github_pr_status")
        .expect("github_pr_status JSON on context");
    assert_eq!(
        v,
        json!({"phase":"published","url":"https://github.com/example/repo/pull/7","error": null})
    );
}
