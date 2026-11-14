//! Command and callback-payload vocabulary: the slash commands, the `callback_data` prefixes,
//! and the pure parsers, formatters and chunkers that decode and encode them.

use super::*;

// ---------------------------------------------------------------------------
// Parsing & chunking
// ---------------------------------------------------------------------------

const START_WORKFLOW_CMD: &str = "/start-workflow";
const CHAIN_WORKFLOW_CMD: &str = "/chain-workflow";
/// Start a Claude Code CLI session with the first user prompt: `/start-claude <prompt>`.
/// Follows project → branch → model keyboard flow; skips recipe (tddy-coder only).
pub const START_CLAUDE_CMD: &str = "/start-claude";
/// Start a Cursor Agent CLI session with the first user prompt: `/start-cursor <prompt>`.
pub const START_CURSOR_CMD: &str = "/start-cursor";
/// Submit feature text to a running child `tddy-coder` presenter: `/submit-feature <session_id_or_prefix> <description…>`
pub const SUBMIT_FEATURE_CMD: &str = "/submit-feature";
const SESSIONS_CMD: &str = "/sessions";
const DELETE_CMD: &str = "/delete";
pub(super) const TELEGRAM_CONTINUATION: &str = "\n(continued)";

/// Number of sessions shown per Telegram page.
pub const SESSIONS_PAGE_SIZE: usize = 10;

/// Parent-picker page rows for chain workflow: newest first, excluding one session id (typically the child).
pub fn parent_candidates_page_for_chain_picker(
    sessions_base: &Path,
    exclude_session_id: &str,
) -> anyhow::Result<Vec<tddy_session_lifecycle::session_reader::SessionEntry>> {
    let mut parent_candidates =
        tddy_session_lifecycle::session_reader::list_sessions_in_dir(sessions_base)?;
    parent_candidates.retain(|s| s.session_id != exclude_session_id);
    parent_candidates.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(parent_candidates
        .into_iter()
        .take(SESSIONS_PAGE_SIZE)
        .collect())
}

/// Remote branches listed per Telegram page (plus one row for project default integration base).
pub const BRANCH_PAGE_SIZE: usize = 10;

/// Callback prefixes for session list inline buttons (must fit Telegram `callback_data` byte limit).
pub const CB_ENTER: &str = "enter:";
pub const CB_DELETE: &str = "delete:";
pub const CB_MORE: &str = "more:";
/// Pick project for Telegram workflow: `tp:<proj_idx>|s:<session_id>`.
pub const CB_TELEGRAM_PROJECT: &str = "tp:";
/// Pick agent: `ta:<agent_idx>|p:<proj_idx>|s:<session_id>`.
pub const CB_TELEGRAM_AGENT: &str = "ta:";
/// Pick integration base (`branch_idx` 0 = project default; 1..=N = recent remote on this page):
/// `tb:<branch_idx>|p:<proj_idx>|s:<session_id>` or `tb:<branch_idx>|o:<list_offset>|p:<proj_idx>|s:<session_id>`.
pub const CB_TELEGRAM_BRANCH: &str = "tb:";
/// Next page of remote branches: `tbm:<next_list_offset>|p:<proj_idx>|s:<session_id>`.
pub const CB_TELEGRAM_BRANCH_MORE: &str = "tbm:";
/// Branch/worktree intent (`nb` / `ws` — must fit Telegram `callback_data` byte limit with `|s:<uuid>`).
pub const CB_TELEGRAM_INTENT: &str = "intent:";
/// Chain workflow: parent session row for stacked work (`tcp:p:<parent_tail8>|s:<child_session_id>`).
///
/// `parent_tail8` = last 8 chars of the parent session id — stable regardless of list order.
/// The child session id is validated by [`validate_session_id_segment`] on callback dispatch.
pub const CB_TELEGRAM_CHAIN_PARENT: &str = "tcp:";
/// Branch-conflict prompt: `tbc:<choice>:<proj_idx>:<session_id>`, where `<choice>` is the
/// [`TelegramBranchConflictChoice`] code.
///
/// The contested branch name deliberately does **not** ride in the payload — Telegram caps
/// `callback_data` at 64 bytes — it is re-derived from the changeset when the callback arrives.
/// Per `docs/ft/daemon/session-branch-conflict.md` § Telegram.
pub const CB_TELEGRAM_BRANCH_CONFLICT: &str = "tbc:";
/// Parsed [`CallbackQuery::data`](https://core.telegram.org/bots/api#callbackquery) for session list actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionControlCallback {
    Enter { session_id: String },
    Delete { session_id: String },
    More { offset: usize },
}

/// Decode `enter:…`, `delete:…`, or `more:…` session-list callback payloads.
pub fn parse_session_control_callback(callback_data: &str) -> Option<SessionControlCallback> {
    let data = callback_data.trim();
    if let Some(rest) = data.strip_prefix(CB_ENTER) {
        let id = rest.trim();
        if !id.is_empty() {
            return Some(SessionControlCallback::Enter {
                session_id: id.to_string(),
            });
        }
    }
    if let Some(rest) = data.strip_prefix(CB_DELETE) {
        let id = rest.trim();
        if !id.is_empty() {
            return Some(SessionControlCallback::Delete {
                session_id: id.to_string(),
            });
        }
    }
    if let Some(rest) = data.strip_prefix(CB_MORE) {
        let offset = rest.trim().parse().ok()?;
        return Some(SessionControlCallback::More { offset });
    }
    None
}

/// Parse `/chain-workflow <prompt>` from a message body (prompt only; chat/user come from the update envelope).
pub fn parse_chain_workflow_prompt(message_text: &str) -> Option<String> {
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(CHAIN_WORKFLOW_CMD)?;
    Some(rest.trim().to_string())
}

/// Parse `/start-workflow <prompt>` from a message body (prompt only; chat/user come from the update envelope).
pub fn parse_start_workflow_prompt(message_text: &str) -> Option<String> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "parse_start_workflow_prompt: len={}",
        message_text.len()
    );
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(START_WORKFLOW_CMD)?;
    Some(rest.trim().to_string())
}

/// Parse `/start-claude <prompt>` from a message body (prompt only; chat/user come from the update envelope).
pub fn parse_start_claude_prompt(message_text: &str) -> Option<String> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "parse_start_claude_prompt: len={}",
        message_text.len()
    );
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(START_CLAUDE_CMD)?;
    Some(rest.trim().to_string())
}

/// Parse `/start-cursor <prompt>` from a message body.
pub fn parse_start_cursor_prompt(message_text: &str) -> Option<String> {
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(START_CURSOR_CMD)?;
    Some(rest.trim().to_string())
}

/// Parse `/sessions` command (with optional offset from callback data).
pub fn parse_sessions_command(message_text: &str) -> Option<usize> {
    let trimmed = message_text.trim();
    if !trimmed.starts_with(SESSIONS_CMD) {
        return None;
    }
    let rest = trimmed[SESSIONS_CMD.len()..].trim();
    if rest.is_empty() {
        return Some(0);
    }
    rest.parse::<usize>().ok()
}

/// Parse `/delete <session_id>` command and return the session id.
pub fn parse_delete_command(message_text: &str) -> Option<String> {
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(DELETE_CMD)?;
    let session_id = rest.trim();
    if session_id.is_empty() {
        return None;
    }
    Some(session_id.to_string())
}

/// Parse `/submit-feature <session_id_or_prefix> <text…>` (multi-word body after the session key).
pub fn parse_submit_feature_command(message_text: &str) -> Option<(String, String)> {
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(SUBMIT_FEATURE_CMD)?;
    let rest = rest.trim();
    let mut it = rest.splitn(2, |c: char| c.is_whitespace());
    let session_key = it.next()?.trim();
    let body = it.next()?.trim();
    if session_key.is_empty() || body.is_empty() {
        return None;
    }
    Some((session_key.to_string(), body.to_string()))
}

/// `doc:<action>:<session_id>` from document-review inline keyboards (see Telegram notifier).
/// `action` is `v` view, `a` approve, `r` refine, `d` back (dismiss viewer), `j` reject.
pub fn parse_document_review_callback(callback_data: &str) -> Option<(char, String)> {
    let rest = callback_data.strip_prefix("doc:")?;
    let (action_s, session_id) = rest.split_once(':')?;
    let action = action_s.chars().next()?;
    if !matches!(action, 'a' | 'r' | 'v' | 'd' | 'j') {
        return None;
    }
    let sid = session_id.trim();
    if sid.is_empty() {
        return None;
    }
    Some((action, sid.to_string()))
}

/// `eli:s:<session_id>:<option_index>` from clarification single-select inline keyboards.
pub fn parse_elicitation_select_callback(callback_data: &str) -> Option<(String, usize)> {
    let rest = callback_data.strip_prefix("eli:s:")?;
    let (session_id, idx_s) = rest.rsplit_once(':')?;
    let idx: usize = idx_s.parse().ok()?;
    let sid = session_id.trim();
    if sid.is_empty() {
        return None;
    }
    Some((sid.to_string(), idx))
}

/// `eli:o:<session_id>` — user chose "Other"; next plain chat message is the custom answer.
pub fn parse_elicitation_other_callback(callback_data: &str) -> Option<String> {
    let sid = callback_data.strip_prefix("eli:o:")?.trim();
    if sid.is_empty() {
        return None;
    }
    Some(sid.to_string())
}

/// Inbound multi-select Telegram shortcut taps (`eli:mn:` / `eli:mr:`), parallel to `/answer-multi`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElicitationMultiSelectShortcutKind {
    ChooseNone,
    ChooseRecommended,
}

/// Parse `eli:mn:<session_id>:<question_index>` or `eli:mr:<session_id>:<question_index>`.
///
/// `session_id` is the opaque tail before the final `:index` (hyphenated UUID-shaped ids are supported).
pub fn parse_elicitation_multi_select_shortcut(
    callback_data: &str,
) -> Option<(String, i32, ElicitationMultiSelectShortcutKind)> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "parse_elicitation_multi_select_shortcut: len={}",
        callback_data.len()
    );
    let (kind, rest) = if let Some(rest) = callback_data.strip_prefix(CHOOSE_NONE_CB_PREFIX) {
        (ElicitationMultiSelectShortcutKind::ChooseNone, rest)
    } else if let Some(rest) = callback_data.strip_prefix(CHOOSE_RECOMMENDED_CB_PREFIX) {
        (ElicitationMultiSelectShortcutKind::ChooseRecommended, rest)
    } else {
        return None;
    };

    let (session_id_raw, idx_s) = rest.rsplit_once(':')?;
    let session_id = session_id_raw.trim();
    if session_id.is_empty() {
        return None;
    }
    let qi: i32 = idx_s.trim().parse().ok()?;

    Some((session_id.to_string(), qi, kind))
}

/// Free-text answer for clarification / text-input mode: `/answer-text <session> <text…>`
pub const ANSWER_TEXT_CMD: &str = "/answer-text";

/// Multi-select clarification: `/answer-multi <session> <comma-separated indices>`
pub const ANSWER_MULTI_CMD: &str = "/answer-multi";

/// Parse `/answer-text <session_key> <body…>` (body may contain spaces).
pub fn parse_answer_text_command(message_text: &str) -> Option<(String, String)> {
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(ANSWER_TEXT_CMD)?;
    let rest = rest.trim();
    let mut it = rest.splitn(2, |c: char| c.is_whitespace());
    let session_key = it.next()?.trim();
    let body = it.next()?.trim();
    if session_key.is_empty() || body.is_empty() {
        return None;
    }
    Some((session_key.to_string(), body.to_string()))
}

/// Parse `/answer-multi <session_key> i,j,k` (0-based indices).
pub fn parse_answer_multi_command(message_text: &str) -> Option<(String, Vec<usize>)> {
    let trimmed = message_text.trim();
    let rest = trimmed.strip_prefix(ANSWER_MULTI_CMD)?;
    let rest = rest.trim();
    let mut it = rest.splitn(2, |c: char| c.is_whitespace());
    let session_key = it.next()?.trim();
    let indices_s = it.next()?.trim();
    if session_key.is_empty() {
        return None;
    }
    let mut indices = Vec::new();
    for part in indices_s.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        indices.push(p.parse().ok()?);
    }
    if indices.is_empty() {
        return None;
    }
    Some((session_key.to_string(), indices))
}

pub use crate::telegram_session_control::session_list_entries::*;

pub use crate::telegram_session_control::callback_data::*;

pub(super) fn parse_demo_options_value(raw: &str) -> anyhow::Result<serde_yaml::Value> {
    let s = raw.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        return serde_yaml::to_value(&v).map_err(Into::into);
    }
    // Accept compact YAML/JSON-like maps from Telegram payloads, e.g. `{run:true}`.
    let normalized = s.replace(":true", ": true").replace(":false", ": false");
    serde_yaml::from_str(&normalized).map_err(Into::into)
}

pub(super) fn take_utf8_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Split plan or elicitation text into Telegram-sized chunks with continuation markers.
///
/// Non-final chunks append [`TELEGRAM_CONTINUATION`] so operators see continuation in-chat. When
/// `max_utf8_bytes` is too small to fit that suffix, chunks are split on byte boundaries only (no marker).
pub fn chunk_telegram_text(full_text: &str, max_utf8_bytes: usize) -> Vec<String> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "chunk_telegram_text: len={} max_utf8_bytes={}",
        full_text.len(),
        max_utf8_bytes
    );
    if full_text.is_empty() {
        return vec![String::new()];
    }
    if max_utf8_bytes == 0 {
        return vec![full_text.to_string()];
    }

    if max_utf8_bytes > TELEGRAM_CONTINUATION.len() {
        let max_content = max_utf8_bytes - TELEGRAM_CONTINUATION.len();
        let mut rest = full_text;
        let mut out = Vec::new();
        while !rest.is_empty() {
            if rest.len() <= max_utf8_bytes {
                out.push(rest.to_string());
                break;
            }
            let piece = take_utf8_prefix(rest, max_content);
            if piece.is_empty() {
                out.push(rest.to_string());
                break;
            }
            out.push(format!("{}{}", piece, TELEGRAM_CONTINUATION));
            rest = &rest[piece.len()..];
        }
        return out;
    }

    let mut rest = full_text;
    let mut out = Vec::new();
    while !rest.is_empty() {
        let piece = take_utf8_prefix(rest, max_utf8_bytes);
        out.push(piece.to_string());
        rest = &rest[piece.len()..];
    }
    out
}

/// Map elicitation callback data to the same structured input bytes the web client would send.
///
/// Encoding: `0x01` = multi-select, `0x00` = single-select; then UTF-8 mode, `NUL`, then options separated by `NUL`.
pub fn map_elicitation_callback_to_presenter_input(callback_data: &str) -> PresenterInputPayload {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "map_elicitation_callback_to_presenter_input: len={}",
        callback_data.len()
    );
    let rest = callback_data
        .strip_prefix("elicitation:")
        .unwrap_or(callback_data);
    let mut parts = rest.splitn(2, '|');
    let mode = parts.next().unwrap_or("");
    let tail = parts.next().unwrap_or("");

    let mut bytes = Vec::new();
    if mode == "multi" {
        bytes.push(1u8);
    } else {
        bytes.push(0u8);
    }
    bytes.extend_from_slice(mode.as_bytes());
    bytes.push(0);
    for opt in tail.split('|').filter(|s| !s.is_empty()) {
        bytes.extend_from_slice(opt.as_bytes());
        bytes.push(0);
    }
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    PresenterInputPayload { bytes }
}

#[cfg(test)]
mod unit_tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn parse_start_workflow_extracts_prompt() {
        let prompt = parse_start_workflow_prompt("/start-workflow   build feature X  ");
        assert_eq!(
            prompt.as_deref(),
            Some("build feature X"),
            "parser must trim and capture text after /start-workflow"
        );
    }

    #[test]
    fn parse_start_claude_extracts_prompt() {
        let prompt = parse_start_claude_prompt("/start-claude   build a hello world CLI  ");
        assert_eq!(
            prompt.as_deref(),
            Some("build a hello world CLI"),
            "parser must trim and capture text after /start-claude"
        );
        // Empty prompt (no text after command)
        let empty = parse_start_claude_prompt("/start-claude");
        assert_eq!(
            empty.as_deref(),
            Some(""),
            "bare /start-claude must yield Some(empty)"
        );
        // Non-matching prefix must return None
        assert!(parse_start_claude_prompt("/start-workflow foo").is_none());
        assert!(parse_start_claude_prompt("hello").is_none());
    }

    #[test]
    fn parse_submit_feature_command_extracts_session_and_body() {
        let p = parse_submit_feature_command(
            "/submit-feature 8f9c7524-aaaa-bbbb-cccc-ddddeeeeffff implement auth",
        );
        assert_eq!(
            p,
            Some((
                "8f9c7524-aaaa-bbbb-cccc-ddddeeeeffff".to_string(),
                "implement auth".to_string()
            ))
        );
    }

    #[test]
    fn parse_submit_feature_command_accepts_prefix_and_multiline_rest() {
        let p = parse_submit_feature_command("/submit-feature 8f9c7524 line one\nline two");
        assert_eq!(
            p,
            Some(("8f9c7524".to_string(), "line one\nline two".to_string()))
        );
    }

    #[test]
    fn resolve_child_grpc_port_exact_and_prefix() {
        use std::collections::HashMap;
        let mut m = HashMap::new();
        m.insert("full-uuid-7-here".to_string(), 50051u16);
        assert_eq!(
            resolve_child_grpc_port(&m, "full-uuid-7-here").unwrap(),
            ("full-uuid-7-here".to_string(), 50051)
        );
        assert_eq!(
            resolve_child_grpc_port(&m, "full-uuid").unwrap(),
            ("full-uuid-7-here".to_string(), 50051)
        );
        assert!(resolve_child_grpc_port(&m, "nope").is_err());
    }

    #[test]
    fn parse_document_review_callback_round_trip() {
        let sid = "1a76d1a7-c703-7abc-8def-123456789abc";
        assert_eq!(
            parse_document_review_callback(&format!("doc:a:{sid}")),
            Some(('a', sid.to_string()))
        );
        assert_eq!(
            parse_document_review_callback(&format!("doc:r:{sid}")),
            Some(('r', sid.to_string()))
        );
        assert_eq!(
            parse_document_review_callback(&format!("doc:j:{sid}")),
            Some(('j', sid.to_string()))
        );
        assert_eq!(parse_document_review_callback("doc:x:uuid"), None);
    }

    #[test]
    fn parse_elicitation_select_callback_round_trip() {
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        assert_eq!(
            parse_elicitation_select_callback(&format!("eli:s:{sid}:2")),
            Some((sid.to_string(), 2))
        );
        assert_eq!(parse_elicitation_select_callback("eli:s:bad"), None);
    }

    #[test]
    fn parse_elicitation_other_callback_round_trip() {
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        assert_eq!(
            parse_elicitation_other_callback(&format!("eli:o:{sid}")),
            Some(sid.to_string())
        );
        assert_eq!(parse_elicitation_other_callback("eli:o:"), None);
        assert_eq!(parse_elicitation_other_callback("eli:s:x:1"), None);
    }

    #[test]
    fn parse_elicitation_multi_select_shortcut_round_trip_choose_none() {
        let sid = "01900000-0000-7000-8000-0000000000aa";
        let encoded = crate::telegram_multi_select_shortcuts::compose_choose_none_callback(sid, 0);
        assert_eq!(
            parse_elicitation_multi_select_shortcut(&encoded),
            Some((
                sid.to_string(),
                0i32,
                ElicitationMultiSelectShortcutKind::ChooseNone
            )),
            "`eli:mn:` GREEN must decode for presenter dispatch",
        );
    }

    #[test]
    fn parse_elicitation_multi_select_shortcut_round_trip_choose_recommended() {
        let sid = "01900000-0000-7000-8000-0000000000bb";
        let qi = 2u32;
        let encoded =
            crate::telegram_multi_select_shortcuts::compose_choose_recommended_callback(sid, qi);
        assert_eq!(
            parse_elicitation_multi_select_shortcut(&encoded),
            Some((
                sid.to_string(),
                qi as i32,
                ElicitationMultiSelectShortcutKind::ChooseRecommended,
            )),
            "`eli:mr:` GREEN must decode for presenter dispatch",
        );
    }

    #[test]
    fn parse_answer_multi_command_extracts_indices() {
        let (k, idx) =
            parse_answer_multi_command("/answer-multi 018faaaa-1111 0, 2 ,3").expect("parse");
        assert_eq!(k, "018faaaa-1111");
        assert_eq!(idx, vec![0usize, 2, 3]);
    }

    #[test]
    fn parse_answer_text_command_accepts_spaces_in_body() {
        let (k, t) =
            parse_answer_text_command("/answer-text 018fbbbb hello world test").expect("parse");
        assert_eq!(k, "018fbbbb");
        assert_eq!(t, "hello world test");
    }

    #[test]
    fn chunk_telegram_text_respects_limit_and_continuation_markers() {
        let text = "0123456789".repeat(6);
        let chunks = chunk_telegram_text(&text, 48);
        assert!(
            chunks.iter().any(|c| c.contains("(continued)")),
            "non-final chunks must include continuation marker; got {chunks:?}"
        );
        let logical: String = chunks
            .iter()
            .map(|c| c.strip_suffix(TELEGRAM_CONTINUATION).unwrap_or(c.as_str()))
            .collect();
        assert_eq!(logical, text);
    }

    #[test]
    fn parse_callback_payload_recognizes_recipe_selection() {
        let key = parse_callback_payload("recipe:tdd|demo:1").expect("expected recipe callback");
        assert!(
            key.contains("recipe:tdd"),
            "parsed routing key should include recipe id: {key}"
        );
    }

    #[test]
    fn parse_start_workflow_returns_none_for_unrecognized_command() {
        assert_eq!(parse_start_workflow_prompt("/other-command hello"), None);
        assert_eq!(parse_start_workflow_prompt("plain text"), None);
    }

    #[test]
    fn parse_callback_payload_returns_none_for_non_recipe_data() {
        assert_eq!(parse_callback_payload("elicitation:single|opt-a"), None);
        assert_eq!(parse_callback_payload("plan_review:approve"), None);
    }

    #[test]
    fn chunk_telegram_text_empty_input_returns_single_empty_chunk() {
        let chunks = chunk_telegram_text("", 100);
        assert_eq!(chunks, vec![""]);
    }

    #[test]
    fn chunk_telegram_text_zero_max_returns_full_text() {
        let chunks = chunk_telegram_text("hello world", 0);
        assert_eq!(chunks, vec!["hello world"]);
    }

    #[test]
    fn chunk_telegram_text_respects_utf8_boundaries() {
        let text = "äöü".repeat(10); // each char is 2 bytes
        let chunks = chunk_telegram_text(&text, 15);
        let rejoined: String = chunks
            .iter()
            .map(|c| c.strip_suffix("\n(continued)").unwrap_or(c.as_str()))
            .collect();
        assert_eq!(rejoined, text, "round-trip must preserve full UTF-8 text");
    }

    // -- /sessions command parsing --

    #[test]
    fn parse_sessions_command_returns_zero_offset_for_bare_command() {
        assert_eq!(parse_sessions_command("/sessions"), Some(0));
        assert_eq!(parse_sessions_command("  /sessions  "), Some(0));
    }

    #[test]
    fn parse_sessions_command_returns_offset_when_provided() {
        assert_eq!(parse_sessions_command("/sessions 10"), Some(10));
        assert_eq!(parse_sessions_command("/sessions 20"), Some(20));
    }

    #[test]
    fn parse_sessions_command_returns_none_for_unrelated_input() {
        assert_eq!(parse_sessions_command("/start-workflow hello"), None);
        assert_eq!(parse_sessions_command("plain text"), None);
        assert_eq!(parse_sessions_command("/delete abc"), None);
    }

    #[test]
    fn parse_sessions_command_returns_none_for_invalid_offset() {
        assert_eq!(parse_sessions_command("/sessions abc"), None);
        assert_eq!(parse_sessions_command("/sessions -5"), None);
    }

    // -- /delete command parsing --

    #[test]
    fn parse_delete_command_extracts_session_id() {
        assert_eq!(
            parse_delete_command("/delete abc-123"),
            Some("abc-123".to_string())
        );
        assert_eq!(
            parse_delete_command("  /delete   sess-42  "),
            Some("sess-42".to_string())
        );
    }

    #[test]
    fn parse_delete_command_returns_none_for_missing_session_id() {
        assert_eq!(parse_delete_command("/delete"), None);
        assert_eq!(parse_delete_command("/delete   "), None);
    }

    #[test]
    fn parse_delete_command_returns_none_for_unrelated_input() {
        assert_eq!(parse_delete_command("/sessions"), None);
        assert_eq!(parse_delete_command("plain text"), None);
    }

    // -- format_session_list_entry --

    #[test]
    fn format_session_list_entry_includes_label_and_status() {
        let entry = TelegramSessionEntry {
            session_id: "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2".to_string(),
            label: "019d5c8f-71b0".to_string(),
            status: "running".to_string(),
            workflow_state: "GreenImplementing".to_string(),
            elapsed_display: "3m 42s".to_string(),
            is_active: true,
        };
        let text = format_session_list_entry(&entry);
        assert!(
            text.contains("019d5c8f-71b0"),
            "formatted entry must include session label; got {text:?}"
        );
        assert!(
            text.contains("running"),
            "formatted entry must include status; got {text:?}"
        );
        assert!(
            text.contains("GreenImplementing"),
            "formatted entry must include workflow state; got {text:?}"
        );
        assert!(
            text.contains("3m 42s"),
            "formatted entry must include elapsed time; got {text:?}"
        );
    }

    #[test]
    fn map_elicitation_callback_to_presenter_input_matches_web_encoding() {
        let payload = map_elicitation_callback_to_presenter_input("elicitation:multi|a|b");
        assert_eq!(
            payload.bytes,
            b"\x01multi\x00a\x00b".to_vec(),
            "multi-select must serialize to stable bytes for presenter layer"
        );
    }

    #[test]
    fn parse_session_control_callback_enter_delete_more() {
        assert_eq!(
            parse_session_control_callback("enter:sess-0001"),
            Some(SessionControlCallback::Enter {
                session_id: "sess-0001".to_string()
            })
        );
        assert_eq!(
            parse_session_control_callback("delete:019d5c8f-71b0-79d1-8492-cfaf08fc6ab2"),
            Some(SessionControlCallback::Delete {
                session_id: "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2".to_string()
            })
        );
        assert_eq!(
            parse_session_control_callback("more:10"),
            Some(SessionControlCallback::More { offset: 10 })
        );
        assert_eq!(parse_session_control_callback("unknown"), None);
    }

    #[test]
    fn parse_recipe_callback_session_dir_finds_session_segment() {
        let base = Path::new("/home/u/.tddy");
        let p = parse_recipe_callback_session_dir("recipe:tdd|session:abc-uuid-123", base);
        assert_eq!(p, Some(unified_session_dir_path(base, "abc-uuid-123")));
    }

    #[test]
    fn parse_recipe_mr_callback_round_trip() {
        let sid = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
        let data = format!("mr:2|{sid}");
        assert!(
            data.len() <= 64,
            "callback_data must fit Telegram limit: {}",
            data.len()
        );
        assert_eq!(parse_recipe_mr_callback(&data), Some((2, sid.to_string())));
        let base = Path::new("/tmp/tddy");
        assert_eq!(
            parse_recipe_callback_session_dir(&data, base),
            Some(unified_session_dir_path(base, sid))
        );
    }

    #[test]
    fn parse_session_id_from_recipe_callback_extracts_uuid() {
        let sid = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
        assert_eq!(
            parse_session_id_from_recipe_callback(&format!("recipe:more|session:{sid}")),
            Some(sid.to_string())
        );
    }

    #[test]
    fn normalize_recipe_name_maps_legacy_tdd_small_to_tdd() {
        assert_eq!(normalize_recipe_name_for_tddy_coder_cli("tdd-small"), "tdd");
        assert_eq!(normalize_recipe_name_for_tddy_coder_cli("bugfix"), "bugfix");
    }

    #[test]
    fn parse_telegram_project_and_agent_callbacks_round_trip() {
        let sid = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
        let tp = format!("tp:2|s:{sid}");
        assert_eq!(
            parse_telegram_project_callback(&tp),
            Some((2, sid.to_string()))
        );
        let ta = format!("ta:1|p:2|s:{sid}");
        assert_eq!(
            parse_telegram_agent_callback(&ta),
            Some((1, 2, sid.to_string()))
        );
        let tb = format!("tb:3|p:2|s:{sid}");
        assert_eq!(
            parse_telegram_branch_callback(&tb),
            Some((3, 0, 2, sid.to_string()))
        );
        let tb_page = format!("tb:3|o:10|p:2|s:{sid}");
        assert_eq!(
            parse_telegram_branch_callback(&tb_page),
            Some((3, 10, 2, sid.to_string()))
        );
        let tbm = format!("tbm:10|p:2|s:{sid}");
        assert_eq!(
            parse_telegram_branch_more_callback(&tbm),
            Some((10, 2, sid.to_string()))
        );
        assert_eq!(parse_telegram_branch_callback("tb:11|p:0|s:x"), None);
    }

    #[test]
    fn parse_chain_workflow_prompt_strips_command_and_trims() {
        assert_eq!(
            parse_chain_workflow_prompt("/chain-workflow  stack on top of auth PR  ").as_deref(),
            Some("stack on top of auth PR"),
            "must strip /chain-workflow prefix and trim whitespace"
        );
        assert_eq!(
            parse_chain_workflow_prompt("/chain-workflow").as_deref(),
            Some(""),
            "command with no prompt returns empty string"
        );
        assert_eq!(
            parse_chain_workflow_prompt("/start-workflow foo"),
            None,
            "wrong command prefix must return None"
        );
    }

    #[test]
    fn parse_telegram_chain_parent_callback_round_trip() {
        let child = "018fbbba-1234-7abc-9abc-123456789abc";
        let parent_tail = "56789abc"; // last 8 chars of parent UUID
        let cb = format!("tcp:p:{parent_tail}|s:{child}");
        assert!(
            cb.len() <= 64,
            "callback_data must fit 64-byte Telegram limit: len={} data={cb:?}",
            cb.len()
        );
        assert_eq!(
            parse_telegram_chain_parent_callback(&cb),
            Some((parent_tail.to_string(), child.to_string()))
        );
    }

    #[test]
    fn parse_telegram_chain_parent_callback_rejects_old_index_format() {
        // Old `tcp:0|s:<child>` format must not parse with the new `p:` prefix scheme.
        assert_eq!(
            parse_telegram_chain_parent_callback("tcp:0|s:018fbbba-1234-7abc-9abc-123456789abc"),
            None,
            "legacy index format must return None"
        );
    }

    #[test]
    fn parse_telegram_chain_parent_callback_rejects_empty_tail() {
        assert_eq!(
            parse_telegram_chain_parent_callback("tcp:p:|s:018fbbba-1234-7abc-9abc-123456789abc"),
            None
        );
    }

    #[test]
    fn session_tail8_returns_last_8_chars() {
        assert_eq!(
            session_tail8("018fbbba-1234-7abc-9abc-123456789abc"),
            "56789abc"
        );
        assert_eq!(session_tail8("sess-0001"), "ess-0001");
        assert_eq!(session_tail8("short"), "short"); // shorter than 8 → full string
    }

    #[test]
    fn parse_telegram_intent_callback_round_trip() {
        use tddy_core::changeset::BranchWorktreeIntent;
        let sid = "019d5c8f-71b0-79d1-8492-cfaf08fc6ab2";
        let nb = format!("intent:nb|s:{sid}");
        let ws = format!("intent:ws|s:{sid}");
        assert!(
            nb.len() <= 64 && ws.len() <= 64,
            "callback_data must fit Telegram limit: nb={} ws={}",
            nb.len(),
            ws.len()
        );
        assert_eq!(
            parse_telegram_intent_callback(&nb),
            Some((BranchWorktreeIntent::NewBranchFromBase, sid.to_string()))
        );
        assert_eq!(
            parse_telegram_intent_callback(&ws),
            Some((BranchWorktreeIntent::WorkOnSelectedBranch, sid.to_string()))
        );
        let nb_long = format!("intent:new_branch_from_base|s:{sid}");
        assert_eq!(
            parse_telegram_intent_callback(&nb_long),
            Some((BranchWorktreeIntent::NewBranchFromBase, sid.to_string()))
        );
    }
}
