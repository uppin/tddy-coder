//! An agent's spec declares where its session's activity comes from (`coding_agent.activity`): the
//! user's prompts and the agent's replies (`messages`) and its tool calls (`tool_calls`), each from
//! its hooks, from its own transcript file, or from both. The transcript's shape — which line is a
//! prompt, a reply, a tool call or a turn's end, and where its text, tool name and input live — is
//! declared in YAML and read by one generic mapper: no agent's transcript is parsed in code.
//!
//! Evidence (2026-11-24, local Cursor probe): Cursor's hooks report its tool calls but none of its
//! text (`afterAgentResponse` duplicates rows and stays uninstalled); its
//! `~/.cursor/projects/<worktree slug>/agent-transcripts/<chat>/<chat>.jsonl` holds every prompt and
//! every reply, the intermediate text between tool calls included, but no tool results and no tool
//! ids. `tests/fixtures/cursor-agent-transcript.jsonl` is that probe's transcript, home anonymised.
//!
//! Feature doc: docs/ft/daemon/cursor-cli-session.md (§ Activity from the transcript).

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolError;
use tddy_coding_agent_specs::{
    embedded_spec, load_agent_registry, worktree_slug, ActivitySource, HookMessageSource,
    SpokenRole, TranscriptEntry, TranscriptPathVars,
};

/// The probe's transcript, one JSON value per line.
fn the_cursor_probe_transcript() -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cursor-agent-transcript.jsonl"),
    )
    .expect("the fixture");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect()
}

fn user(text: &str) -> TranscriptEntry {
    TranscriptEntry::UserMessage {
        text: text.to_string(),
    }
}

fn agent(text: &str) -> TranscriptEntry {
    TranscriptEntry::AgentMessage {
        text: text.to_string(),
    }
}

fn tool(name: &str, input: serde_json::Value) -> TranscriptEntry {
    TranscriptEntry::ToolUse {
        name: name.to_string(),
        input,
        id: None,
    }
}

const PROBE_WORKTREE: &str = "/Users/dev/Code/tddy-coder/.worktrees/cursor-cli-01a20f1b";

#[test]
fn cursor_takes_its_messages_from_its_transcript_and_its_tool_calls_from_its_hooks() {
    // When
    let cursor = embedded_spec("cursor").expect("cursor is built in");

    // Then
    let activity = cursor.activity();
    assert_eq!(
        (
            activity.messages,
            activity.tool_calls,
            activity.transcript.is_some()
        ),
        (ActivitySource::Transcript, ActivitySource::Hooks, true)
    );
}

#[test]
fn claude_and_muse_keep_their_spoken_messages_on_their_hooks() {
    // Given the prompt and the turn's last reply Claude Code's hooks carry
    let spoken = vec![
        HookMessageSource {
            event: "UserPromptSubmit".to_string(),
            field: "prompt".to_string(),
            role: SpokenRole::User,
        },
        HookMessageSource {
            event: "Stop".to_string(),
            field: "last_assistant_message".to_string(),
            role: SpokenRole::Agent,
        },
    ];

    // When
    let claude = embedded_spec("claude").expect("claude is built in");
    let muse = embedded_spec("muse").expect("muse is built in");

    // Then both read them where they always were, now named in YAML rather than in code
    assert_eq!(
        (
            claude.activity().messages,
            &claude.activity().hook_messages,
            muse.activity().messages,
            &muse.activity().hook_messages,
        ),
        (
            ActivitySource::Hooks,
            &spoken,
            ActivitySource::Hooks,
            &spoken
        )
    );
}

#[test]
fn cursors_transcript_maps_to_the_prompt_each_reply_and_each_tool_call_in_order() {
    // Given
    let cursor = embedded_spec("cursor").expect("cursor is built in");
    let transcript = cursor.activity().transcript.clone().expect("a transcript");

    // When every line of the probe's transcript is mapped
    let entries: Vec<TranscriptEntry> = the_cursor_probe_transcript()
        .iter()
        .flat_map(|line| transcript.map_line(line))
        .collect();

    // Then the prompt is unwrapped from its `<timestamp>` and `<user_query>`, the text between the
    // tool calls is kept, and the turn's end closes it
    assert_eq!(
        entries,
        vec![
            user(
                "This is a hooks/activity probe; do not change any files. Step by step: 1) read \
                 README.md (first 20 lines), 2) run the shell command `git log --oneline -3`, 3) \
                 grep for \"CodingBackend\" in packages/, 4) then reply with a one-line summary \
                 and stop."
            ),
            agent("Running the probe steps without modifying any files."),
            tool(
                "Read",
                serde_json::json!({"path": format!("{PROBE_WORKTREE}/README.md"), "limit": 20})
            ),
            tool(
                "Shell",
                serde_json::json!({
                    "command": "git log --oneline -3",
                    "working_directory": PROBE_WORKTREE,
                    "description": "Show last 3 commits oneline"
                })
            ),
            tool(
                "Grep",
                serde_json::json!({
                    "pattern": "CodingBackend",
                    "path": format!("{PROBE_WORKTREE}/packages"),
                    "output_mode": "count"
                })
            ),
            agent(
                "**Probe complete:** `tddy-coder` is a TDD workflow monorepo whose README \
                 highlights `CodingBackend` in `tddy-core`; the last three commits are \
                 session-drawer close (#1032), work-queue watch (#1030), and hooks log naming \
                 (#1031); and `CodingBackend` appears in dozens of files under `packages/` (core, \
                 workflow-engine, agent-backend, integration tests)."
            ),
            TranscriptEntry::TurnEnd,
        ]
    );
}

#[test]
fn a_worktree_slug_is_the_path_with_each_run_of_other_characters_made_one_dash() {
    // Then, as Cursor names its project directories (checked against ~/.cursor/projects)
    assert_eq!(
        (
            worktree_slug("/Users/dev/Code/tddy-coder/.worktrees/cursor-cli-01a20f1b"),
            worktree_slug(
                "/Users/dev/Library/Application Support/Cursor/Workspaces/1/workspace.json"
            ),
            worktree_slug("/home/coder/.tddy/repos/qape-hq/.worktrees/01a20f05-97d8"),
        ),
        (
            "Users-dev-Code-tddy-coder-worktrees-cursor-cli-01a20f1b".to_string(),
            "Users-dev-Library-Application-Support-Cursor-Workspaces-1-workspace-json".to_string(),
            "home-coder-tddy-repos-qape-hq-worktrees-01a20f05-97d8".to_string(),
        )
    );
}

#[test]
fn on_a_target_the_transcript_path_is_the_targets_home_and_the_targets_worktree() {
    // Given a session on a Coder target: its home and its worktree are the target's
    let cursor = embedded_spec("cursor").expect("cursor is built in");
    let transcript = cursor.activity().transcript.clone().expect("a transcript");
    let on_the_target = TranscriptPathVars {
        agent_home: "/home/coder".to_string(),
        conversation_id: "6a42ca72-f90d-4d19-9b8e-618ece0956b5".to_string(),
        worktree: "/home/coder/.tddy/repos/qape-hq/.worktrees/01a20f05-97d8-7221-85bc-d879ececfb62"
            .to_string(),
    };

    // When
    let path = transcript.render_path(&on_the_target);

    // Then
    assert_eq!(
        path,
        Ok("/home/coder/.cursor/projects/\
            home-coder-tddy-repos-qape-hq-worktrees-01a20f05-97d8-7221-85bc-d879ececfb62/\
            agent-transcripts/6a42ca72-f90d-4d19-9b8e-618ece0956b5/\
            6a42ca72-f90d-4d19-9b8e-618ece0956b5.jsonl"
            .to_string())
    );
}

#[test]
fn a_part_no_emit_rule_matches_maps_to_nothing() {
    // Given a reply part of a kind the transcript declares nothing for
    let cursor = embedded_spec("cursor").expect("cursor is built in");
    let transcript = cursor.activity().transcript.clone().expect("a transcript");
    let line = serde_json::json!({
        "role": "assistant",
        "message": {"content": [{"type": "thinking", "text": "hmm"}, {"type": "text", "text": "Done."}]}
    });

    // When
    let entries = transcript.map_line(&line);

    // Then
    assert_eq!(entries, vec![agent("Done.")]);
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
fn messages_from_a_transcript_the_spec_does_not_declare_are_refused() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    an_agent_file(dir.path(), "acme", "    activity: { messages: transcript }");

    // Then
    assert!(
        the_refusal_message(dir.path()).contains("activity: messages are read from a transcript"),
        "{}",
        the_refusal_message(dir.path())
    );
}

#[test]
fn a_transcript_path_naming_an_unknown_variable_is_refused() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    an_agent_file(
        dir.path(),
        "acme",
        r#"    activity:
      messages: transcript
      transcript:
        path: "{agent_home}/{nope}.jsonl"
        lines: [{ emit: [{ kind: turn_end }] }]"#,
    );

    // Then
    assert!(
        the_refusal_message(dir.path()).contains("'{nope}'"),
        "{}",
        the_refusal_message(dir.path())
    );
}

#[test]
fn a_message_rule_without_its_text_is_refused() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    an_agent_file(
        dir.path(),
        "acme",
        r#"    activity:
      messages: transcript
      transcript:
        path: "{agent_home}/{conversation_id}.jsonl"
        lines: [{ emit: [{ kind: agent_message }] }]"#,
    );

    // Then
    assert!(
        the_refusal_message(dir.path()).contains("agent_message names no text"),
        "{}",
        the_refusal_message(dir.path())
    );
}
