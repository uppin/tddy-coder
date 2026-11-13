//! An agent's spec declares how its agent reports a fresh conversation inside a running session —
//! `/clear` in Claude Code (`coding_agent.conversation.rotation`, `#orchestrators` 6/11). The daemon
//! follows the session to the new conversation by this slot alone, so no agent's rotation is named
//! in code. A rotation on a hook the agent's hooks adapter never installs could never be reported,
//! and is refused at load naming the file and the hook.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-conversation-rotation.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolError;
use tddy_coding_agent_specs::slots::conversation::{RotationHook, RotationSlot};
use tddy_coding_agent_specs::{embedded_spec, load_agent_registry, CODING_AGENT_EXTENSION};

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
    /// An operator's agent `id` with Claude Code's hooks adapter, whose conversation is
    /// `conversation_line`.
    fn with_agent(self, id: &str, conversation_line: &str) -> Self {
        std::fs::write(
            self.dir.path().join(format!("{id}.yaml")),
            an_agent_file(id, conversation_line),
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

/// An operator's agent `id` wired through `claude-settings-local`; `conversation_line` is appended
/// to its `coding_agent` section verbatim.
fn an_agent_file(id: &str, conversation_line: &str) -> String {
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
    {conversation_line}
"#
    )
}

/// The rejection `load_agent_registry` answers for `tools`, which must be a `coding_agent` one.
fn the_refusal_of(tools: &AToolsDir) -> (PathBuf, String) {
    let refused =
        load_agent_registry(tools.path()).expect_err("the registry refuses the agent file");
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

/// One report of a rotation: `hook` when the event's `field` is `value`, its conversation in
/// `session_id`.
fn a_report(hook: &str, (field, value): (&str, &str)) -> RotationHook {
    RotationHook {
        hook: hook.to_string(),
        when: BTreeMap::from([(field.to_string(), value.to_string())]),
        id_field: "session_id".to_string(),
    }
}

#[test]
fn claude_declares_a_clear_rotation_on_session_start() {
    // When
    let claude = embedded_spec("claude").expect("claude is built in");

    // Then Claude Code's `/clear` is its SessionEnd with `reason: clear` for the conversation it
    // leaves, followed by its SessionStart with `source: clear` for the one it begins, each id in
    // `session_id`
    assert_eq!(
        claude.conversation().rotation(),
        Some(&RotationSlot {
            ends: a_report("SessionEnd", ("reason", "clear")),
            begins: a_report("SessionStart", ("source", "clear")),
        })
    );
}

#[test]
fn a_rotation_whose_ending_hook_the_adapter_does_not_install_is_refused_at_load() {
    // Given an operator's agent whose rotation ends on a hook claude-settings-local never installs
    let tools = a_tools_dir().with_agent(
        "acme",
        "conversation: { kind: daemon_session_id, rotation: { ends: { hook: PreCompact, when: { trigger: manual }, id_field: session_id }, begins: { hook: SessionStart, when: { source: clear }, id_field: session_id } } }",
    );

    // When the registry is loaded
    let (file, message) = the_refusal_of(&tools);

    // Then it is refused, naming the file and the hook
    assert_eq!(file, tools.file("acme.yaml"));
    assert!(message.contains("PreCompact"), "message: {message}");
}

#[test]
fn a_rotation_naming_a_hook_the_adapter_does_not_install_is_refused_at_load() {
    // Given an operator's agent whose rotation begins on a hook claude-settings-local never installs
    let tools = a_tools_dir().with_agent(
        "acme",
        "conversation: { kind: daemon_session_id, rotation: { ends: { hook: SessionEnd, when: { reason: clear }, id_field: session_id }, begins: { hook: PreCompact, when: { trigger: manual }, id_field: session_id } } }",
    );

    // When the registry is loaded
    let (file, message) = the_refusal_of(&tools);

    // Then it is refused, naming the file and the hook
    assert_eq!(file, tools.file("acme.yaml"));
    assert!(message.contains("PreCompact"), "message: {message}");
}
