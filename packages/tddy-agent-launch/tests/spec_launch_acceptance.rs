//! Acceptance: the spec-driven host PTY launcher runs PTY agents only.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md` (`#agent-tools 2/12`).

use std::path::PathBuf;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ResolvedTool, TemplateParams, ToolDef, Transport};
use tddy_agent_launch::spec_launch::{launch_host_pty, HostAgent, HostPtySession, SpecLaunchError};
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;

/// A valid coding-agent tool file `id` whose `run.transport` is `transport`.
fn an_agent_tool(id: &str, transport: &str) -> ToolDef {
    serde_yaml::from_str(&format!(
        r#"schema_version: 1
id: {id}
summary: a coding agent
binary:
  command: {id}
deps: []
run:
  transport: {transport}
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
"#
    ))
    .expect("the tool file deserializes")
}

fn resolved(id: &str) -> ResolvedTool {
    ResolvedTool {
        id: id.to_string(),
        command: id.to_string(),
        binary: PathBuf::from(format!("/usr/local/bin/{id}")),
        reads: Vec::new(),
    }
}

#[tokio::test]
async fn a_spec_whose_transport_is_not_pty_is_refused_by_the_host_pty_launcher() {
    // Given — a headless agent declared with `transport: stdio`
    let spec = CodingAgentSpec::of(&an_agent_tool("claude-headless", "stdio"))
        .expect("a stdio agent is a valid coding agent");
    let agent = HostAgent {
        spec,
        tool: resolved("claude-headless"),
        deps: Vec::new(),
    };
    let worktree = tempfile::tempdir().unwrap();
    let manager = Arc::new(CliSessionManager::new());

    // When
    let refused = launch_host_pty(
        &agent,
        &TemplateParams::new(),
        &RenderedSlots::default(),
        worktree.path(),
        HostPtySession {
            manager: &manager,
            session_id: "0190-headless",
            session_type: "claude-cli",
            model: "opus",
            env: Vec::new(),
            os_user: None,
        },
    )
    .await;

    // Then — refused naming the spec, and nothing was registered
    assert_eq!(
        refused.err(),
        Some(SpecLaunchError::TransportNotPty {
            spec: "claude-headless".to_string(),
            transport: Transport::Stdio,
        })
    );
    assert!(manager.get("0190-headless").await.is_none());
}
