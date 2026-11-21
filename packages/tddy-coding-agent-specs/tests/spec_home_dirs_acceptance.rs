//! Every shipped coding agent names the directories under its home that hold its settings and chat
//! files (`coding_agent.home_dirs`, `#orchestra` 1/9). An orchestrator's read reach includes them,
//! under both the OS user's home and the agent's persistent tddy home.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolError;
use tddy_coding_agent_specs::{embedded_spec, load_agent_registry};

#[test]
fn every_shipped_agent_declares_its_home_dirs() {
    // Given the agents an orchestrator starts sessions with
    let agents = ["claude", "cursor", "muse", "codex", "opencode"];

    // When each one's declared home directories are read
    let undeclared: Vec<&str> = agents
        .into_iter()
        .filter(|id| {
            embedded_spec(id)
                .unwrap_or_else(|e| panic!("{id} is built in: {e}"))
                .home_dirs()
                .is_empty()
        })
        .collect();

    // Then none is missing
    assert_eq!(undeclared, Vec::<&str>::new());
}

#[test]
fn claudes_home_dirs_hold_its_chat_transcripts() {
    // Given
    let claude = embedded_spec("claude").expect("claude is built in");

    // When / Then
    assert_eq!(claude.home_dirs(), [".claude".to_string()]);
}

/// An operator's agent `id` whose `coding_agent` section declares `home_dirs`.
fn an_agent_with_home_dirs(dir: &tempfile::TempDir, id: &str, home_dirs: &str) {
    std::fs::write(
        dir.path().join(format!("{id}.yaml")),
        format!(
            r#"schema_version: 1
id: {id}
summary: a coding agent
binary:
  command: {id}
deps: []
run:
  transport: pty
  argv: ["{{binary}}"]
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: opus, curated: [opus] }}
    home_dirs: {home_dirs}
"#
        ),
    )
    .expect("a tool file");
}

#[test]
fn an_absolute_home_dir_is_refused_at_load() {
    // Given an operator's agent that names /etc as a home directory
    let tools = tempfile::tempdir().unwrap();
    an_agent_with_home_dirs(&tools, "greedy", "[/etc]");

    // When the registry is loaded
    let refused = load_agent_registry(tools.path()).unwrap_err();

    // Then it is refused, naming the slot and the directory
    let ToolError::ExtensionRejected { file, message, .. } = refused else {
        panic!("expected a coding_agent refusal, got {refused:?}");
    };
    assert_eq!(file, tools.path().join("greedy.yaml"));
    assert!(
        message.contains("home_dirs") && message.contains("/etc"),
        "{message}"
    );
}

#[test]
fn a_home_dir_that_climbs_out_of_the_home_is_refused_at_load() {
    // Given an operator's agent whose home dir climbs out with `..`
    let tools = tempfile::tempdir().unwrap();
    an_agent_with_home_dirs(&tools, "climber", "[\"../other-user\"]");

    // When the registry is loaded
    let refused = load_agent_registry(tools.path()).unwrap_err();

    // Then it is refused
    let ToolError::ExtensionRejected { message, .. } = refused else {
        panic!("expected a coding_agent refusal, got {refused:?}");
    };
    assert!(message.contains("home_dirs"), "{message}");
}
