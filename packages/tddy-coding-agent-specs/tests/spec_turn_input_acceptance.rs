//! An agent's spec declares how a new turn reaches a running session of it (`coding_agent.turn_input`,
//! `#orchestrators` 4/11): a terminal paste followed by a submit key, or an ACP prompt.
//! `SendSessionMessage` delivers by this slot alone, so no agent's delivery is named in code. A slot
//! that cannot be delivered by is refused at load, naming the file; a spec without one loads as it
//! did, and its sessions are refused a message.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-session-turn-control.md`.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolError;
use tddy_coding_agent_specs::{
    embedded_spec, load_agent_registry, TurnInputSlot, CODING_AGENT_EXTENSION,
};

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
    /// An operator's agent `id` whose `coding_agent` section ends with `turn_input_line`.
    fn with_agent(self, id: &str, turn_input_line: &str) -> Self {
        std::fs::write(
            self.dir.path().join(format!("{id}.yaml")),
            an_agent_file(id, turn_input_line),
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

/// An operator's agent `id`; `turn_input_line` is appended to its `coding_agent` section verbatim.
fn an_agent_file(id: &str, turn_input_line: &str) -> String {
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
    {turn_input_line}
"#
    )
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

#[test]
fn claude_declares_terminal_paste_turn_input() {
    // When
    let claude = embedded_spec("claude").expect("claude is built in");

    // Then a turn reaches Claude Code's TUI as a paste, submitted with Enter
    assert_eq!(
        claude.turn_input(),
        Some(&TurnInputSlot::TerminalPaste {
            submit_key: "Enter".to_string(),
        })
    );
}

#[test]
fn an_unknown_turn_input_kind_is_refused_at_load() {
    // Given an operator's agent whose turn_input names a kind nothing delivers by
    let tools = a_tools_dir().with_agent("pigeon", "turn_input: { kind: carrier_pigeon }");

    // When the registry is loaded
    let (file, message) = the_refusal_of(&tools);

    // Then it is refused, naming the file and the kind
    assert_eq!(file, tools.file("pigeon.yaml"));
    assert!(message.contains("carrier_pigeon"), "message: {message}");
}

#[test]
fn a_terminal_paste_whose_submit_key_is_not_a_key_is_refused_at_load() {
    // Given an operator's agent whose terminal paste is submitted with a key that does not exist
    let tools = a_tools_dir().with_agent(
        "hyper",
        "turn_input: { kind: terminal_paste, submit_key: Hyper }",
    );

    // When the registry is loaded
    let (file, message) = the_refusal_of(&tools);

    // Then it is refused, naming the file, the slot and the key
    assert_eq!(file, tools.file("hyper.yaml"));
    assert!(
        message.contains("turn_input") && message.contains("Hyper"),
        "message: {message}"
    );
}
