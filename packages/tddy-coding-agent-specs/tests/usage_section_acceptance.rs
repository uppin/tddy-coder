//! Acceptance: a coding agent's `usage:` binding is part of its spec — the built-in Claude specs
//! declare theirs, and a binding the engine could not evaluate is refused when the spec loads,
//! naming the file (`#usage-binding 1/5`).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-usage-binding-claude-transcript.md

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolError;
use tddy_coding_agent_specs::{load_agent_registry, CodingAgentSpec, CODING_AGENT_EXTENSION};
use tddy_usage::UsageSource;

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

/// A `claude.yaml` override whose `coding_agent` section also holds `coding_agent_lines` (YAML
/// indented four spaces).
fn a_claude_override_with(coding_agent_lines: &str) -> String {
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
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: opus, curated: [opus] }}
{coding_agent_lines}"#
    )
}

/// A one-source `jsonl` binding whose main path is `main_path`.
fn a_usage_binding_reading(main_path: &str) -> String {
    format!(
        r#"    usage:
      sources:
        - format: jsonl
          main: {{ path: "{main_path}" }}
          record:
            tokens: {{ input: usage.input }}
"#
    )
}

/// The refusal `tools` loads with, as `(file, owner, message)`.
fn the_refusal(tools: &AToolsDir) -> (PathBuf, String, String) {
    let refused = load_agent_registry(tools.path()).unwrap_err();
    let ToolError::ExtensionRejected {
        file,
        owner,
        message,
    } = refused
    else {
        panic!("expected an extension refusal, got {refused:?}");
    };
    (file, owner, message)
}

#[test]
fn the_builtin_claude_specs_bind_their_transcripts() {
    // Given — a fresh `<tddyhome>` with no overrides
    let home = tempfile::tempdir().unwrap();

    // When
    let registry = load_agent_registry(&home.path().join("tools")).expect("the built-ins load");

    // Then
    for id in ["claude", "claude-acp"] {
        let spec =
            CodingAgentSpec::of(registry.get(id).expect("built in")).expect("a coding agent");
        let usage = spec
            .agent
            .usage
            .unwrap_or_else(|| panic!("{id} declares no usage"));
        let [UsageSource::Jsonl(source)] = usage.sources.as_slice() else {
            panic!("{id}: expected one jsonl source, got {:?}", usage.sources);
        };
        assert_eq!(
            source.main.path.as_str(),
            "{agent_home}/.claude/projects/*/{conversation_id}.jsonl",
            "{id}"
        );
    }
}

#[test]
fn an_unknown_usage_format_is_refused_at_load_naming_the_file() {
    // Given
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with(
            r#"    usage:
      sources:
        - format: parquet
          main: { path: "{agent_home}/usage.parquet" }
"#,
        ),
    );

    // When
    let (file, owner, message) = the_refusal(&tools);

    // Then
    assert_eq!(
        (file, owner),
        (
            tools.file("claude.yaml"),
            CODING_AGENT_EXTENSION.to_string()
        )
    );
    assert!(
        message.contains("parquet"),
        "the refusal must name the format: {message}"
    );
}

#[test]
fn an_unknown_template_variable_is_refused_at_load() {
    // Given
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with(&a_usage_binding_reading(
            "{agent_home}/{project}/{conversation_id}.jsonl",
        )),
    );

    // When
    let (file, _, message) = the_refusal(&tools);

    // Then
    assert_eq!(file, tools.file("claude.yaml"));
    assert!(
        message.contains("usage:") && message.contains("{project}"),
        "the refusal must name the section and the variable: {message}"
    );
}

#[test]
fn a_leftover_transcript_usage_flag_is_refused_at_load() {
    // Given — an override written before `usage:` replaced the flag
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with("    transcript_usage: true\n"),
    );

    // When
    let (file, _, message) = the_refusal(&tools);

    // Then
    assert_eq!(file, tools.file("claude.yaml"));
    assert!(
        message.contains("transcript_usage"),
        "the refusal must name the field: {message}"
    );
}

#[test]
fn a_binding_with_no_sources_is_refused_at_load() {
    // Given
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with("    usage: { sources: [] }\n"),
    );

    // When
    let (file, _, message) = the_refusal(&tools);

    // Then
    assert_eq!(file, tools.file("claude.yaml"));
    assert!(
        message.contains("usage:") && message.contains("source"),
        "the refusal must say the binding has no source: {message}"
    );
}

#[test]
fn a_source_without_a_main_path_is_refused_at_load() {
    // Given
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with(
            r#"    usage:
      sources:
        - format: jsonl
          record: { tokens: { input: usage.input } }
"#,
        ),
    );

    // When
    let (file, _, message) = the_refusal(&tools);

    // Then
    assert_eq!(file, tools.file("claude.yaml"));
    assert!(
        message.contains("main"),
        "the refusal must name the missing field: {message}"
    );
}
