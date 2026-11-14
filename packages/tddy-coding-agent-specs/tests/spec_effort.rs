//! An agent's spec declares the effort levels a session of it can be started at, and how its launch
//! carries the chosen one (`coding_agent.effort`, `#orchestrators` 3/11). A slot that contradicts
//! itself is refused at load, naming the file; a spec without one loads as it did.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-start-effort.md`.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ToolDef, ToolError};
use tddy_coding_agent_specs::{
    embedded_spec, load_agent_registry, CodingAgentSpec, EffortCarry, EffortSlot, SpecError,
    CODING_AGENT_EXTENSION,
};

/// The levels `claude --effort` takes (Claude Code 2.1.311: `--effort <level>  Effort level for
/// the current session (low, medium, high, xhigh, max)`).
const CLAUDE_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

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
    /// An operator's agent `id` whose `coding_agent` section ends with `effort_line` (empty for
    /// none).
    fn with_agent(self, id: &str, effort_line: &str) -> Self {
        std::fs::write(
            self.dir.path().join(format!("{id}.yaml")),
            an_agent_file(id, effort_line),
        )
        .expect("a tool file");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn file(&self, file: &str) -> PathBuf {
        self.dir.path().join(file)
    }
}

/// An operator's agent `id`; `effort_line` is appended to its `coding_agent` section verbatim.
fn an_agent_file(id: &str, effort_line: &str) -> String {
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
    {effort_line}
"#
    )
}

fn an_agent_spec(id: &str, effort_line: &str) -> Result<CodingAgentSpec, SpecError> {
    let def: ToolDef =
        serde_yaml::from_str(&an_agent_file(id, effort_line)).expect("the tool file deserializes");
    CodingAgentSpec::of(&def)
}

/// The rejection `load_agent_registry` answers for `tools`, which must be a `coding_agent` one.
fn the_refusal_of(tools: &AToolsDir) -> (PathBuf, String) {
    let refused = load_agent_registry(tools.path()).unwrap_err();
    let ToolError::ExtensionRejected {
        file,
        owner,
        message,
    } = refused
    else {
        panic!("expected a coding_agent refusal, got {refused:?}");
    };
    assert_eq!(owner, CODING_AGENT_EXTENSION);
    (file, message)
}

fn levels(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[test]
fn claude_declares_its_effort_levels() {
    // When
    let claude = embedded_spec("claude").expect("claude is built in");

    // Then — the levels its CLI takes, carried by its `--effort` flag; no default of its own, so
    // an empty effort leaves Claude Code's
    assert_eq!(
        claude.effort(),
        Some(&EffortSlot {
            levels: levels(&CLAUDE_LEVELS),
            default: None,
            carry: EffortCarry::Flag("--effort".to_string()),
        })
    );
}

#[test]
fn a_spec_whose_default_is_not_one_of_its_levels_is_refused_at_load() {
    // Given
    let tools = a_tools_dir().with_agent(
        "lark",
        "effort: { levels: [low, high], default: medium, flag: --effort }",
    );

    // When
    let (file, message) = the_refusal_of(&tools);

    // Then — the file is named, and so are the default and the levels it is not among
    assert_eq!(
        (file, message),
        (
            tools.file("lark.yaml"),
            "effort: default medium is not one of its levels (low, high)".to_string()
        )
    );
}

#[test]
fn a_spec_without_an_effort_slot_loads_as_before() {
    // When
    let spec = an_agent_spec("lark", "").expect("the spec is valid");

    // Then — guard
    assert_eq!(spec.effort(), None);
}
