//! What `session-hook --await-answer` prints when an orchestrator answered the `AskUserQuestion` a
//! `PreToolUse` hook is holding (`#home-inbox` 8/8).
//!
//! # The mechanism, verified against Claude Code 2.1.312 (2026-10-28)
//!
//! A `PreToolUse` hook returns `permissionDecision: "allow"` with `updatedInput` — the tool's input
//! with an `answers` map added (question text → answer string) — and Claude **does not open the
//! dialog**: it runs the tool as if the person had answered, and the transcript shows
//! `User answered Claude's questions: · <question> → <answer>`. Checked interactively, not headless:
//! `claude -p` does not offer `AskUserQuestion` at all. The alternative, a deny whose reason states
//! the answers, was not needed; it would leave a denied call in the transcript.
//!
//! ```json
//! {"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow",
//!   "updatedInput":{"questions":[…as asked…],"answers":{"Which language?":"Rust"}}}}
//! ```
//!
//! - `updatedInput` replaces the whole input, so `questions` is the call's own, unchanged.
//! - An answer string is the chosen labels joined with `", "` (Claude's schema: multi-select answers
//!   are comma-separated), then `other` when given. An `other` alone is the text itself.
//! - A question the answers leave out is absent from `answers`; Claude then asks it.

use serde_json::{json, Value};

use crate::session_hook::QuestionAnswer;

/// The decision JSON that hands `answers` to Claude for the call whose input is `tool_input`, or
/// `None` when nothing can be handed over: `answers` is empty, `tool_input` has no `questions`, or
/// an answer names a question index it does not have. `None` prints nothing, and Claude shows its own dialog.
#[must_use]
pub fn encode_question_decision(tool_input: &Value, answers: &[QuestionAnswer]) -> Option<String> {
    if answers.is_empty() {
        return None;
    }
    let questions = tool_input.get("questions")?.as_array()?;
    let mut by_question = serde_json::Map::new();
    for answer in answers {
        let question = questions.get(answer.question_index as usize)?;
        let text = question.get("question")?.as_str()?;
        let free_text = answer.other.as_deref().filter(|text| !text.is_empty());
        let parts: Vec<&str> = answer
            .selected
            .iter()
            .map(String::as_str)
            .chain(free_text)
            .collect();
        by_question.insert(text.to_string(), Value::String(parts.join(", ")));
    }
    let mut updated_input = tool_input.as_object()?.clone();
    updated_input.insert("answers".to_string(), Value::Object(by_question));
    Some(
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": updated_input,
            }
        })
        .to_string(),
    )
}
