//! An agent's spec declares where a running session of it reports the model it is on now
//! (`coding_agent.model_report`), so a model its operator picked inside the agent — Cursor's or
//! Claude Code's `/model` — is the one its session resumes on. No agent's report is named in code.
//!
//! Evidence (2026-11-24): every Cursor Agent hook event carries `model` (`glm-5p3-flash-high` after
//! `/model` → GLM 5.3 Flash); Claude Code's hooks carry `model` on `SessionStart` only, and its
//! transcript names each assistant message's `message.model`.
//!
//! Feature doc: docs/ft/daemon/cursor-cli-session.md.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolError;
use tddy_coding_agent_specs::{embedded_spec, load_agent_registry, ModelReportSource};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// An operator's agent `id` in `dir`, whose `coding_agent` section ends with `lines` verbatim.
fn an_agent_file(dir: &Path, id: &str, lines: &str) {
    std::fs::write(
        dir.join(format!("{id}.yaml")),
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
{lines}
"#
        ),
    )
    .expect("a tool file");
}

fn the_refusal_message(dir: &Path) -> String {
    match load_agent_registry(dir).unwrap_err() {
        ToolError::ExtensionRejected { message, .. } => message,
        other => panic!("expected a coding_agent refusal, got {other:?}"),
    }
}

#[test]
fn cursor_reports_its_model_on_every_hook_event() {
    // When
    let cursor = embedded_spec("cursor").expect("cursor is built in");

    // Then every hook event's `model` field is the model the session is on
    assert_eq!(
        cursor.model_report(),
        &[ModelReportSource::HookField {
            field: "model".to_string(),
            events: Vec::new(),
            ignore: Vec::new(),
        }]
    );
}

#[test]
fn claude_reports_its_model_at_start_and_from_its_transcript_after_each_turn() {
    // When
    let claude = embedded_spec("claude").expect("claude is built in");

    // Then SessionStart's `model` names it, and each turn's end reads the transcript's last model,
    // which is not a model when Claude Code wrote the message itself
    assert_eq!(
        claude.model_report(),
        &[
            ModelReportSource::HookField {
                field: "model".to_string(),
                events: strings(&["SessionStart"]),
                ignore: Vec::new(),
            },
            ModelReportSource::Usage {
                events: strings(&["Stop"]),
                ignore: strings(&["<synthetic>"]),
            },
        ]
    );
}

#[test]
fn a_source_reads_on_the_events_it_names_or_on_every_event_when_it_names_none() {
    // Given a source for SessionStart and a source for every event
    let at_start = ModelReportSource::HookField {
        field: "model".to_string(),
        events: strings(&["SessionStart"]),
        ignore: Vec::new(),
    };
    let always = ModelReportSource::HookField {
        field: "model".to_string(),
        events: Vec::new(),
        ignore: Vec::new(),
    };

    // Then each applies where it says
    assert!(at_start.applies_to("SessionStart"));
    assert!(!at_start.applies_to("Stop"));
    assert!(always.applies_to("beforeSubmitPrompt"));
}

#[test]
fn a_blank_or_ignored_value_is_no_model() {
    // Given a source that ignores Claude Code's own messages
    let usage = ModelReportSource::Usage {
        events: Vec::new(),
        ignore: strings(&["<synthetic>"]),
    };

    // Then only a real model name is accepted
    assert_eq!(
        usage.accepts("claude-sonnet-5-5"),
        Some("claude-sonnet-5-5")
    );
    assert_eq!(usage.accepts("<synthetic>"), None);
    assert_eq!(usage.accepts("  "), None);
}

#[test]
fn a_usage_source_on_an_agent_without_a_usage_binding_is_refused_at_load() {
    // Given an operator's agent that reads its model off a usage binding it does not declare
    let tools = tempfile::tempdir().expect("a tools dir");
    an_agent_file(
        tools.path(),
        "ghost",
        "    model_report: [ { kind: usage, events: [Stop] } ]",
    );

    // When the registry is loaded
    let message = the_refusal_message(tools.path());

    // Then it is refused, naming the slot and the missing binding
    assert!(
        message.contains("model_report") && message.contains("usage"),
        "message: {message}"
    );
}

#[test]
fn a_hook_field_source_without_a_field_name_is_refused_at_load() {
    // Given an operator's agent whose hook source names an empty field
    let tools = tempfile::tempdir().expect("a tools dir");
    an_agent_file(
        tools.path(),
        "blank",
        r#"    model_report: [ { kind: hook_field, field: "" } ]"#,
    );

    // When the registry is loaded
    let message = the_refusal_message(tools.path());

    // Then it is refused, naming the slot
    assert!(message.contains("model_report"), "message: {message}");
}
