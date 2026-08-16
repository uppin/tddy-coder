//! A spawned `tddy-coder` resolves `--agent` against the builtins and `<tddyhome>/agents` only —
//! it cannot read this daemon's model registry. So when the daemon resolved the name there, the
//! def travels with the spawn as `--agent-def <json>`; without it the child would come up as a
//! different agent entirely.
//!
//! `supervisor_spawn_delegation.rs` pins the argv of an ordinary session; this suite pins only the
//! def hand-over.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC9).

use std::path::{Path, PathBuf};

use tddy_daemon::spawner::{self, LiveKitCreds, SpawnOptions};
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to resolve the target account")
}

fn a_livekit() -> LiveKitCreds {
    LiveKitCreds {
        url: "ws://127.0.0.1:7880".to_string(),
        api_key: "test-key".to_string(),
        api_secret: "test-secret".to_string(),
        common_room: None,
        daemon_instance_id: None,
    }
}

/// A long-running stand-in for `tddy-coder`, so a child plan can be computed against a real
/// executable without starting a session.
fn a_tool_that_stays_alive(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-tddy-coder.sh");
    std::fs::write(&path, "#!/bin/sh\nsleep 5\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The def a registry assistant projects onto.
fn a_repo_explorer_def() -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "repo-explorer".to_string(),
        label: Some("Repo explorer".to_string()),
        model: "qwen3:32b".to_string(),
        base_url: "http://127.0.0.1:11434".to_string(),
        system_prompt: Some("You explore repositories.".to_string()),
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Grep],
        max_turns: 10,
        replaces: Vec::new(),
    }
}

/// The argv the child would be started with for `opts`.
fn planned_argv(opts: SpawnOptions<'_>) -> Vec<String> {
    let repo = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = a_tool_that_stays_alive(tools.path());
    spawner::plan_session_child(
        &current_username(),
        tool.to_str().unwrap(),
        data_dir.path(),
        repo.path(),
        &a_livekit(),
        opts,
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
    )
    .expect("plan the session child")
    .args
}

/// The value the child receives for `flag`, or `None` when the flag is absent.
fn flag_value<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.iter()
        .position(|a| a == flag)
        .map(|i| argv[i + 1].as_str())
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

#[test]
fn a_resolved_agent_def_travels_with_the_spawn_so_the_child_can_build_its_backend() {
    // Given
    let def = a_repo_explorer_def();
    let def_json = serde_json::to_string(&def).expect("serialize the def");

    // When
    let argv = planned_argv(SpawnOptions {
        new_session_id: Some("session-a"),
        agent: Some("repo-explorer"),
        agent_def_json: Some(&def_json),
        ..Default::default()
    });

    // Then — the child is told both the name and the def that name means here
    assert_eq!(flag_value(&argv, "--agent"), Some("repo-explorer"));
    let carried: SpecializedAgentDef =
        serde_json::from_str(flag_value(&argv, "--agent-def").expect("--agent-def must be passed"))
            .expect("the carried def must round-trip");
    assert_eq!(carried, def);
}

#[test]
fn an_agent_the_child_resolves_for_itself_carries_no_def() {
    // Given / When — a config-allowlist coding backend
    let argv = planned_argv(SpawnOptions {
        new_session_id: Some("session-a"),
        agent: Some("claude"),
        ..Default::default()
    });

    // Then
    assert_eq!(flag_value(&argv, "--agent"), Some("claude"));
    assert_eq!(flag_value(&argv, "--agent-def"), None);
}
