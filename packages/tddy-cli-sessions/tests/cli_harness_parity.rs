//! The CLI harness descriptors reproduce, argument for argument, the claude-cli and cursor-cli
//! launches the session manager has always built.
//!
//! These pin the descriptor refactor as behaviour-preserving: every pair below must stay equal for
//! the start and resume shapes those sessions use.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::cli_harness::{cli_harness, CliLaunch};

fn claude_argv_through_the_descriptor(launch: &CliLaunch<'_>) -> Vec<String> {
    (cli_harness("claude-cli").expect("claude-cli").argv)(launch)
}

fn cursor_argv_through_the_descriptor(launch: &CliLaunch<'_>) -> Vec<String> {
    (cli_harness("cursor-cli").expect("cursor-cli").argv)(launch)
}

#[test]
fn claude_cli_and_cursor_cli_argv_are_unchanged_through_the_descriptor() {
    // Claude: fresh start with a prompt and a permission mode, a resume, and the skip flag.
    for (prompt, mode, skip, resume) in [
        (Some("hello"), Some("plan"), false, false),
        (None, None, false, true),
        (Some("  "), Some("acceptEdits"), true, false),
    ] {
        let launch = CliLaunch {
            binary: "/opt/claude",
            model: "opus",
            session_id: "sess-7",
            initial_prompt: prompt,
            permission_mode: mode,
            dangerously_skip_permissions: skip,
            resume,
            ..CliLaunch::default()
        };
        assert_eq!(
            claude_argv_through_the_descriptor(&launch),
            CliSessionManager::build_claude_argv(
                "/opt/claude",
                "opus",
                "sess-7",
                prompt,
                mode,
                skip,
                resume
            ),
            "claude prompt={prompt:?} mode={mode:?} skip={skip} resume={resume}"
        );
    }

    // Cursor: a chat id or none, a model or none, a prompt or none.
    for (chat, model, prompt) in [
        (Some("chat-1"), "composer-2.5", Some("go")),
        (None, "", None),
        (Some("  "), "gpt-5.2", Some("  ")),
    ] {
        let launch = CliLaunch {
            binary: "agent",
            model,
            session_id: "sess-7",
            conversation_id: chat,
            initial_prompt: prompt,
            ..CliLaunch::default()
        };
        assert_eq!(
            cursor_argv_through_the_descriptor(&launch),
            CliSessionManager::build_cursor_argv("agent", model, chat, prompt),
            "cursor chat={chat:?} model={model:?} prompt={prompt:?}"
        );
    }
}
