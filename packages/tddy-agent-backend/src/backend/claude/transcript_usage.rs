//! Token usage read back from Claude Code's own session transcripts.

use std::path::Path;

/// Fold every assistant message's `message.usage` input/output counts in one transcript's
/// contents (Claude's separate `cache_*` counters are intentionally not folded into input),
/// returning the summed usage, the assistant-turn count, and the last model seen on those lines.
fn sum_assistant_usage(
    contents: &str,
) -> (crate::token_accounting::TokenUsage, u32, Option<String>) {
    use crate::token_accounting::TokenUsage;

    let mut usage = TokenUsage::default();
    let mut turns = 0u32;
    let mut model: Option<String> = None;
    for line in contents.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let message = &value["message"];
        usage.input_tokens += message["usage"]["input_tokens"].as_u64().unwrap_or(0);
        usage.output_tokens += message["usage"]["output_tokens"].as_u64().unwrap_or(0);
        if let Some(m) = message["model"].as_str() {
            model = Some(m.to_string());
        }
        turns += 1;
    }
    (usage, turns, model)
}

/// Sum the Claude Code agent's *main-thread* token usage from its own session transcript.
///
/// Claude Code writes the main transcript to
/// `<claude_home>/.claude/projects/<encoded-cwd>/<session_id>.jsonl`. Because the runner spawns
/// `claude --session-id <session_id>`, the file name is deterministic, so we locate it by session
/// id (unique) rather than reconstructing the cwd encoding. Nested Task-tool subagents are NOT in
/// this file (they live under `<session_id>/subagents/` — see [`read_claude_subagent_usages`]).
///
/// This Claude-Code-specific transcript layout lives with the Claude backend on purpose — other
/// agents store their transcripts elsewhere. When no transcript exists, the record reports zero
/// tokens with `fallback_model` — never an error, so a caller can still render a main-agent row.
pub fn read_claude_transcript_usage(
    claude_home: &Path,
    session_id: &str,
    fallback_model: &str,
) -> crate::token_accounting::ConversationRecord {
    use crate::token_accounting::{ConversationRecord, TokenUsage};

    let mut usage = TokenUsage::default();
    let mut turns = 0u32;
    let mut model: Option<String> = None;

    let projects_dir = claude_home.join(".claude").join("projects");
    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let transcript = entry.path().join(format!("{session_id}.jsonl"));
            let Ok(contents) = std::fs::read_to_string(&transcript) else {
                continue;
            };
            let (turn_usage, turn_count, turn_model) = sum_assistant_usage(&contents);
            usage = usage + turn_usage;
            turns += turn_count;
            if turn_model.is_some() {
                model = turn_model;
            }
        }
    }

    ConversationRecord {
        agent: "claude".to_string(),
        id: session_id.to_string(),
        model: model.unwrap_or_else(|| fallback_model.to_string()),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total(),
        turns,
    }
}

/// One [`ConversationRecord`](crate::token_accounting::ConversationRecord) per Claude Code Task-tool
/// subagent the session spawned.
///
/// Claude Code records each nested subagent in its own transcript at
/// `<claude_home>/.claude/projects/<encoded-cwd>/<session_id>/subagents/agent-<id>.jsonl`, with a
/// sibling `agent-<id>.meta.json` carrying its `agentType` (e.g. `"Explore"`). Each record uses the
/// agent type as `agent`, the `agent-<id>` file stem as the conversation `id`, and sums the
/// subagent's own assistant `message.usage`. Results are sorted by id for a stable summary. Returns
/// an empty vec when the session spawned no subagents.
pub fn read_claude_subagent_usages(
    claude_home: &Path,
    session_id: &str,
    fallback_model: &str,
) -> Vec<crate::token_accounting::ConversationRecord> {
    use crate::token_accounting::ConversationRecord;

    let mut records = Vec::new();
    let projects_dir = claude_home.join(".claude").join("projects");
    let Ok(projects) = std::fs::read_dir(&projects_dir) else {
        return records;
    };
    for project in projects.flatten() {
        let subagents_dir = project.path().join(session_id).join("subagents");
        let Ok(entries) = std::fs::read_dir(&subagents_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("subagent")
                .to_string();
            let (usage, turns, model) = sum_assistant_usage(&contents);
            let agent = std::fs::read_to_string(subagents_dir.join(format!("{stem}.meta.json")))
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| {
                    v.get("agentType")
                        .and_then(|a| a.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "subagent".to_string());
            records.push(ConversationRecord {
                agent,
                id: stem,
                model: model.unwrap_or_else(|| fallback_model.to_string()),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total(),
                turns,
            });
        }
    }
    records.sort_by(|a, b| a.id.cmp(&b.id));
    records
}
