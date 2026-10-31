//! Acceptance: coding-agent specs load through the declared-tool registry, and a bad agent file
//! is refused at load, naming what is wrong.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md` (`#agent-tools 2/12`).

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ToolError, Transport};
use tddy_coding_agent_specs::{load_agent_registry, CodingAgentSpec, CODING_AGENT_EXTENSION};

/// A `<tddyhome>/tools` directory.
struct AToolsDir {
    dir: tempfile::TempDir,
}

fn a_tools_dir() -> AToolsDir {
    AToolsDir {
        dir: tempfile::tempdir().expect("a tools dir"),
    }
}

impl AToolsDir {
    fn with_file(self, file: &str, yaml: &str) -> Self {
        std::fs::write(self.dir.path().join(file), yaml).expect("a tool file");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn file(&self, file: &str) -> PathBuf {
        self.dir.path().join(file)
    }
}

/// A `claude.yaml` override whose `coding_agent.mcp` is `mcp` and whose `extensions:` also hold
/// `extra_extensions` (YAML lines indented two spaces).
fn a_claude_override(mcp: &str, extra_extensions: &str) -> String {
    format!(
        r#"schema_version: 1
id: claude
summary: Claude Code CLI
binary:
  command: claude
deps: [tddy-tools]
run:
  transport: pty
  argv: ["{{binary}}"]
extensions:
  coding_agent:
    mcp: {mcp}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: opus, curated: [opus] }}
{extra_extensions}"#
    )
}

const A_FILE_MCP_SLOT: &str = "{ kind: file, flag: --mcp-config, format: claude-mcp-json }";

#[test]
fn the_builtin_claude_spec_loads_and_depends_on_tddy_tools() {
    // Given — a fresh `<tddyhome>` with no overrides
    let home = tempfile::tempdir().unwrap();

    // When
    let registry = load_agent_registry(&home.path().join("tools")).expect("the built-ins load");

    // Then
    let claude = registry.get("claude").expect("claude is built in");
    assert_eq!(claude.deps, vec!["tddy-tools".to_string()]);
    assert_eq!(
        registry
            .get("tddy-tools")
            .map(|t| t.binary.command.as_str()),
        Some("tddy-tools")
    );
    let spec = CodingAgentSpec::of(claude).expect("claude is a coding agent");
    assert_eq!(spec.run.transport, Transport::Pty);
    assert_eq!(spec.agent.models.default, "opus");
    assert_eq!(spec.agent.hooks.adapter, "claude-settings-local");
}

#[test]
fn an_unknown_slot_kind_is_refused_naming_the_file() {
    // Given — an `mcp` slot of a kind no renderer knows
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override("{ kind: socket, path: /tmp/mcp.sock }", ""),
    );

    // When
    let refused = load_agent_registry(tools.path()).unwrap_err();

    // Then
    let ToolError::ExtensionRejected {
        file,
        owner,
        message,
    } = refused
    else {
        panic!("expected an extension refusal, got {refused:?}");
    };
    assert_eq!(
        (file, owner),
        (
            tools.file("claude.yaml"),
            CODING_AGENT_EXTENSION.to_string()
        )
    );
    // serde_yaml words the rest; the contract is that the offending kind is named.
    assert!(
        message.contains("socket"),
        "the refusal must name the slot kind: {message}"
    );
}

#[test]
fn an_unknown_extension_owner_is_refused_naming_the_owner() {
    // Given — a valid `coding_agent` section beside a key nothing owns
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override(
            A_FILE_MCP_SLOT,
            "  telemetry: { endpoint: https://example.test }\n",
        ),
    );

    // When
    let refused = load_agent_registry(tools.path());

    // Then
    assert_eq!(
        refused.map(|registry| registry.ids().count()),
        Err(ToolError::UnknownExtension {
            file: tools.file("claude.yaml"),
            owner: "telemetry".to_string(),
        })
    );
}

#[test]
fn the_same_override_without_the_unowned_key_loads() {
    // Given — the positive control for the refusal above
    let tools = a_tools_dir().with_file("claude.yaml", &a_claude_override(A_FILE_MCP_SLOT, ""));

    // When
    let registry = load_agent_registry(tools.path()).expect("the override loads");

    // Then
    assert_eq!(
        registry.origin("claude"),
        Some(tools.file("claude.yaml").as_path())
    );
}
