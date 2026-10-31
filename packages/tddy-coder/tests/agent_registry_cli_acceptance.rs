//! Acceptance: `tddy-coder --agent` accepts any coding-agent registry id that can take a turn
//! without a terminal, and the interactive selection lists the registry (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md` § Acceptance Criteria.
//!
//! The registry is loaded from a `<tddyhome>/tools` of the test's own holding one new agent,
//! `acme`, declared in YAML alone: a headless turn read by the existing `claude-stream-json`
//! adapter.

use pretty_assertions::assert_eq;
use tddy_coder::agent_registry_cli::{backend_agent_ids, backend_selection_question_from};
use tddy_coding_agent_specs::CodingAgentRegistry;

/// An agent no Rust names: its turn is one `acme -p PROMPT`, read as Claude's stream JSON.
const ACME_YAML: &str = r#"schema_version: 1
id: acme
summary: Acme Code — an agent added as a tool file alone
binary:
  command: acme
  env: TDDY_ACME_CLI
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv:
    - "{binary}"
    - { if: model, then: ["--model", "{model}"] }
  env: {}
extensions:
  coding_agent:
    mcp: { kind: unsupported }
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: { adapter: unsupported }
    capabilities: [requires_model]
    models:
      default: acme-1
      curated: [acme-1]
    headless:
      argv: ["{binary}", -p, "{prompt}", --output-format, stream-json]
      adapter: claude-stream-json
"#;

#[test]
fn tddy_coder_agent_accepts_any_registry_id_and_the_interactive_selection_lists_the_registry() {
    // Given
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("acme.yaml"), ACME_YAML).unwrap();
    let registry = CodingAgentRegistry::load(&tools).expect("the registry loads");

    // When
    let accepted = backend_agent_ids(&registry);
    let offered: Vec<String> = backend_selection_question_from(&registry)
        .options
        .into_iter()
        .map(|option| option.label)
        .collect();

    // Then — every agent with a headless or ACP mode, `acme` among them, then the stub; the
    // PTY-only `muse` and `warp` are not workflow backends
    let expected: Vec<String> = [
        "acme",
        "claude",
        "claude-acp",
        "codex",
        "codex-acp",
        "cursor",
        "opencode",
        "oz",
        "stub",
    ]
    .iter()
    .map(|id| id.to_string())
    .collect();
    assert_eq!((accepted, offered), (expected.clone(), expected));
}
