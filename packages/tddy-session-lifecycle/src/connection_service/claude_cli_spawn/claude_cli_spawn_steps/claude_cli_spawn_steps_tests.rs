//! Unit tests: what a claude-cli start launches its agent with, beyond the managed workflow's own.

use super::*;

fn the_accounts_git_pairs() -> Vec<(String, String)> {
    vec![
        ("GIT_AUTHOR_NAME".to_string(), "ada".to_string()),
        ("GIT_COMMITTER_NAME".to_string(), "ada".to_string()),
    ]
}

/// A start with no recipe and no index, so the only environment is the one under test.
async fn the_launch_environment_with(
    git_environment: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let progress = crate::connection_service::AttachmentProgressSink::discarding();
    let registry = TaskRegistry::new();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (_, _, env_extra) = managed_claude_cli_launch(ManagedClaudeCliLaunch {
        tddy_data_dir: dir.path(),
        session_id: "s1",
        managed_recipe: &None,
        child_spawn_handler: None,
        conversation_spawn_handler: None,
        semantic_index: false,
        progress: &progress,
        task_registry: &registry,
        session_dir: dir.path(),
        worktree_path: dir.path(),
        tddy_tools_path: String::new(),
        git_environment,
    })
    .await
    .expect("a launch with nothing to prepare succeeds");
    env_extra
}

#[tokio::test]
async fn the_resolved_accounts_pairs_are_in_the_agents_environment() {
    // Given a start whose project resolved to an account
    // When the launch environment is built
    let env = the_launch_environment_with(the_accounts_git_pairs()).await;

    // Then the agent is launched with exactly those pairs
    assert_eq!(env, the_accounts_git_pairs());
}

#[tokio::test]
async fn a_start_that_resolved_nothing_adds_no_git_variables() {
    // Given a start whose project resolved to no account
    // When the launch environment is built
    let env = the_launch_environment_with(Vec::new()).await;

    // Then the agent inherits the checkout's identity: nothing is added
    assert!(env.is_empty());
}
