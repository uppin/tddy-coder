//! A command an agent runs through the exec tools sees no `TDDY_*` variable (`#agent-tools 15/19`,
//! the user's rule: the agent process sees none). The process hosting the engine may carry them — a
//! `tddy-tools` adopts the session it resolved into its own environment, a daemon may have been
//! started with some — and the engine reads one from its `env` slice (`TDDY_SEMANTIC_INDEX_DB`);
//! none of them reaches the command. A `tddy-tools` the command runs finds its session through the
//! `.tddy/session.json` pointer in its cwd instead.
//!
//! It sets process-wide variables, so it is the only test in this binary.

use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool_with_env;

#[tokio::test]
async fn an_exec_tool_command_starts_with_no_tddy_variable_inherited_or_passed() {
    // Given a host process that adopted a session into its environment
    for (variable, value) in [
        ("TDDY_SOCKET", "/adopted/session.sock"),
        ("TDDY_SESSION_DIR", "/adopted/session-dir"),
        ("TDDY_REPO_DIR", "/adopted/repo"),
        ("TDDY_REMOTE_HOST", "adopted-remote-host"),
    ] {
        std::env::set_var(variable, value);
    }
    let worktree = tempfile::tempdir().unwrap();

    // When a session's shell command prints its environment, called with a TDDY_* variable of the
    // engine's own and an ordinary one
    let outcome = execute_tool_with_env(
        worktree.path(),
        "Shell",
        &serde_json::json!({ "command": "env" }).to_string(),
        &TaskRegistry::new(),
        "exec-env-session",
        &[
            (
                "TDDY_SEMANTIC_INDEX_DB".to_string(),
                "/engine/index.db".to_string(),
            ),
            ("PASSED_THROUGH".to_string(), "kept".to_string()),
        ],
    )
    .await;

    // Then the command ran with the ordinary variable and with no TDDY_* variable at all
    assert!(
        outcome.result_json.contains("PASSED_THROUGH=kept"),
        "the command must have printed its environment with the passed variable; got: {} / {}",
        outcome.result_json,
        outcome.error_message
    );
    assert!(
        !outcome.result_json.contains("TDDY_"),
        "a TDDY_* variable reached the command: {}",
        outcome.result_json
    );
}
