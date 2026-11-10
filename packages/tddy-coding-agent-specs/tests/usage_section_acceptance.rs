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
        // The transcript is the first source; `#usage-binding 3/5` adds Claude's tapped headless
        // results after it.
        let Some(UsageSource::Jsonl(source)) = usage.sources.first() else {
            panic!(
                "{id}: expected a jsonl source first, got {:?}",
                usage.sources
            );
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

#[test]
fn two_sources_mapping_the_same_counter_are_refused_at_load() {
    // Given — a transcript and a tapped stream both claiming input tokens
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with(
            r#"    usage:
      sources:
        - format: jsonl
          main: { path: "{agent_home}/{conversation_id}.jsonl" }
          record: { tokens: { input: usage.input } }
        - format: jsonl
          main: { path: "{session_dir}/usage/{conversation_id}.jsonl" }
          record: { tokens: { input: usage.input_tokens } }
"#,
        ),
    );

    // When
    let (file, _, message) = the_refusal(&tools);

    // Then
    assert_eq!(file, tools.file("claude.yaml"));
    assert!(
        message.contains("usage:") && message.contains("input"),
        "the refusal must name the counter mapped twice: {message}"
    );
}

#[test]
fn the_builtin_claude_spec_taps_its_headless_results_for_their_cost() {
    // Given
    let home = tempfile::tempdir().unwrap();

    // When
    let registry = load_agent_registry(&home.path().join("tools")).expect("the built-ins load");
    let spec =
        CodingAgentSpec::of(registry.get("claude").expect("built in")).expect("a coding agent");

    // Then — the headless driver taps `result` events, and a second source reads their cost
    let headless = spec.agent.headless.expect("claude runs headless");
    assert_eq!(
        serde_json::to_value(&headless.usage_events).unwrap(),
        serde_json::json!([{ "field": "type", "equals": "result" }])
    );
    let usage = spec.agent.usage.expect("claude declares usage");
    let Some(UsageSource::Jsonl(tapped)) = usage.sources.get(1) else {
        panic!("expected a second jsonl source, got {:?}", usage.sources);
    };
    assert_eq!(
        (
            tapped.main.path.as_str(),
            tapped.record.cost.as_ref().map(|c| c.as_str()),
            tapped.record.tokens.clone(),
        ),
        (
            "{session_dir}/usage/{conversation_id}.jsonl",
            Some("total_cost_usd"),
            tddy_usage::CounterMap::default(),
        )
    );
}

#[test]
fn a_malformed_counter_expression_is_refused_at_load() {
    // Given
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with(
            r#"    usage:
      sources:
        - format: jsonl
          main: { path: "{agent_home}/{conversation_id}.jsonl" }
          record: { tokens: { input: "usage.input -" } }
"#,
        ),
    );

    // When
    let (file, _, message) = the_refusal(&tools);

    // Then
    assert_eq!(file, tools.file("claude.yaml"));
    assert!(
        message.contains("usage.input -"),
        "the refusal must quote the expression: {message}"
    );
}

#[test]
fn children_by_parent_field_without_an_identity_is_refused_at_load() {
    // Given
    let tools = a_tools_dir().with_file(
        "claude.yaml",
        &a_claude_override_with(
            r#"    usage:
      sources:
        - format: jsonl
          main: { path: "{agent_home}/*.jsonl" }
          children_by: parent_field
          record: { tokens: { input: usage.input } }
"#,
        ),
    );

    // When
    let (file, _, message) = the_refusal(&tools);

    // Then
    assert_eq!(file, tools.file("claude.yaml"));
    assert!(
        message.contains("usage:") && message.contains("identity"),
        "the refusal must say children_by needs an identity: {message}"
    );
}

#[test]
fn the_builtin_codex_specs_bind_their_rollouts() {
    // Given
    let home = tempfile::tempdir().unwrap();

    // When
    let registry = load_agent_registry(&home.path().join("tools")).expect("the built-ins load");

    // Then
    for id in ["codex", "codex-acp"] {
        let spec =
            CodingAgentSpec::of(registry.get(id).expect("built in")).expect("a coding agent");
        let usage = spec
            .agent
            .usage
            .unwrap_or_else(|| panic!("{id} declares no usage"));
        let Some(UsageSource::Jsonl(source)) = usage.sources.first() else {
            panic!("{id}: expected a jsonl source, got {:?}", usage.sources);
        };
        assert_eq!(
            (source.main.path.as_str(), source.record.aggregate),
            (
                "{agent_home}/.codex/sessions/*/*/*/rollout-*.jsonl",
                tddy_usage::Aggregate::Last
            ),
            "{id}"
        );
    }
}
