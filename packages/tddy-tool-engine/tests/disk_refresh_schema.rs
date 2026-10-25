//! `RefreshWorktreeDiskUsage` says what it refreshes and where to read the progress
//! (`#start-parity` 8/9).

use tddy_tool_engine::project_home_disk_refresh::{
    refresh_worktree_disk_usage_tool_def, REFRESH_WORKTREE_DISK_USAGE,
};

#[test]
fn it_refreshes_one_worktree_or_all_and_says_where_progress_is_read() {
    // When
    let def = refresh_worktree_disk_usage_tool_def().expect("RefreshWorktreeDiskUsage is defined");
    let schema: serde_json::Value = serde_json::from_str(&def.input_schema_json).unwrap();

    // Then
    assert_eq!(def.name, REFRESH_WORKTREE_DISK_USAGE);
    assert!(
        def.description.contains("ListWorktrees"),
        "{}",
        def.description
    );
    for argument in ["path", "daemon_instance_id"] {
        assert!(
            !schema["properties"][argument]["description"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "{argument} is not described"
        );
    }
    assert!(schema["required"].as_array().is_none_or(|r| r.is_empty()));
}
