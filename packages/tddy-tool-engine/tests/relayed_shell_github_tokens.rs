//! A shell command the daemon runs on a session's behalf — a sandboxed agent's commands are relayed
//! here and run in the daemon's own process — inherits none of the GitHub token variables the
//! daemon was started with. `gh` prefers them over its own login, so one exported where the daemon
//! runs would act as whoever exported it.
//!
//! It sets process-wide variables, so it is the only test in this binary.

use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool_with_env;

#[tokio::test]
async fn a_relayed_shell_command_inherits_none_of_the_daemons_github_tokens() {
    // Given a daemon environment carrying every GitHub token variable gh honours
    for variable in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
    ] {
        std::env::set_var(variable, "from_the_daemons_environment");
    }
    let worktree = tempfile::tempdir().unwrap();

    // When a session's shell command prints its environment
    let outcome = execute_tool_with_env(
        worktree.path(),
        "Shell",
        &serde_json::json!({ "command": "env" }).to_string(),
        &TaskRegistry::new(),
        "relayed-shell-session",
        &[],
    )
    .await;

    // Then the command ran, and was started with none of them
    assert!(
        outcome.result_json.contains("HOME="),
        "the command must have printed its environment; got: {} / {}",
        outcome.result_json,
        outcome.error_message
    );
    assert_eq!(
        outcome.result_json.contains("from_the_daemons_environment"),
        false,
        "a GitHub token variable reached the command: {}",
        outcome.result_json
    );
}
