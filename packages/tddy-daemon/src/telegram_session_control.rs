//! Inbound Telegram control plane for TDD workflow sessions (plan review, elicitation, recipe selection).
//!
//! Bridges Telegram-style commands/callbacks to session directories (`changeset.yaml`) and the same
//! presenter input encodings as the web client. The daemon binary's [`crate::telegram_bot`] module
//! dispatches inbound updates to [`TelegramSessionControlHarness`]; integration tests use [`InMemoryTelegramSender`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tddy_core::changeset::{read_changeset, write_changeset, BranchWorktreeIntent, Changeset};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{
    list_recent_remote_branches_skip, read_session_metadata,
    validate_chain_pr_integration_base_ref, WorkflowError, SESSION_METADATA_FILENAME,
};
use uuid::Uuid;

use crate::active_elicitation::{ActiveElicitationCoordinator, SharedActiveElicitationCoordinator};
use crate::config::DaemonConfig;
use crate::presenter_intent_client;
use crate::project_storage::{self, effective_integration_base_ref_for_project, ProjectData};
use crate::session_list_enrichment::SessionListStatusDisplay;
use crate::spawn_worker;
use crate::spawner::{self, SpawnOptions};
use crate::telegram_github_link::TelegramGithubMappingStore;
use crate::telegram_multi_select_shortcuts::{CHOOSE_NONE_CB_PREFIX, CHOOSE_RECOMMENDED_CB_PREFIX};
use crate::telegram_notifier::{
    session_telegram_label, ElicitationMultiSelectMetaCache, ElicitationSelectOptionsCache,
    InMemoryTelegramSender, InlineKeyboardRows, TelegramSender,
};
use crate::telegram_session_subscriber::TelegramDaemonHooks;
use crate::user_sessions_path::projects_path_for_user;

// ---------------------------------------------------------------------------
// Public types (contract under test)
// ---------------------------------------------------------------------------

/// Simulated Telegram `/start-workflow` or reply payload.
#[derive(Debug, Clone)]
pub struct StartWorkflowCommand {
    pub chat_id: i64,
    pub user_id: u64,
    /// Full text after `/start-workflow` (trimmed).
    pub prompt: String,
}

/// Inline keyboard callback for recipe / demo / elicitation (opaque payload string).
#[derive(Debug, Clone)]
pub struct TelegramCallback {
    pub chat_id: i64,
    pub user_id: u64,
    pub callback_data: String,
}

/// Outbound Telegram message captured by the harness (text + optional inline keyboard).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedTelegramMessage {
    pub chat_id: i64,
    pub text: String,
    /// Row-major inline keyboard: `(button label, callback_data)` per button.
    pub inline_keyboard: InlineKeyboardRows,
}

/// Result of handling `/start-workflow`: session id + anything sent to Telegram.
#[derive(Debug, Clone)]
pub struct StartWorkflowOutcome {
    pub session_id: String,
    pub messages: Vec<CapturedTelegramMessage>,
}

/// Parsed `changeset.yaml` fields relevant to Telegram-driven routing (subset).
#[derive(Debug, Deserialize, PartialEq)]
pub struct ChangesetRoutingSnapshot {
    pub recipe: Option<String>,
    #[serde(default)]
    pub initial_prompt: Option<String>,
    #[serde(default)]
    pub demo_options: Option<serde_yaml::Value>,
    #[serde(default)]
    pub run_optional_step_x: Option<bool>,
    #[serde(default)]
    pub workflow: Option<WorkflowRoutingSnapshot>,
}

/// Subset of `workflow` from `changeset.yaml` for tests and snapshots.
#[derive(Debug, Deserialize, PartialEq)]
pub struct WorkflowRoutingSnapshot {
    #[serde(default)]
    pub branch_worktree_intent: Option<BranchWorktreeIntent>,
}

/// Bytes sent to the presenter / workflow input layer (must match web RPC encoding for the same UI action).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenterInputPayload {
    pub bytes: Vec<u8>,
}

/// Workflow step identifier used only in tests to compare with web approval transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowTransitionKind {
    PlanReviewApproved,
    ElicitationSubmitted,
}

/// A single session entry formatted for Telegram display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramSessionEntry {
    pub session_id: String,
    pub label: String,
    pub status: String,
    pub workflow_state: String,
    pub elapsed_display: String,
    pub is_active: bool,
}

/// Result of `/sessions` command: one page of session entries + whether more pages exist.
#[derive(Debug, Clone)]
pub struct SessionListPage {
    pub entries: Vec<TelegramSessionEntry>,
    pub has_more: bool,
    pub next_offset: usize,
}

/// Result of `/delete <session_id>` command.
#[derive(Debug, Clone)]
pub struct DeleteSessionOutcome {
    pub session_id: String,
    pub confirmation_message: CapturedTelegramMessage,
}

/// Result of entering a session workflow from the session list.
#[derive(Debug, Clone)]
pub struct EnterSessionOutcome {
    pub session_id: String,
    pub messages: Vec<CapturedTelegramMessage>,
}

// ---------------------------------------------------------------------------
// Parsing & chunking
// ---------------------------------------------------------------------------

const START_WORKFLOW_CMD: &str = "/start-workflow";
/// Submit feature text to a running child `tddy-coder` presenter: `/submit-feature <session_id_or_prefix> <description…>`
pub const SUBMIT_FEATURE_CMD: &str = "/submit-feature";
const SESSIONS_CMD: &str = "/sessions";
const DELETE_CMD: &str = "/delete";
const TELEGRAM_CONTINUATION: &str = "\n(continued)";

/// Number of sessions shown per Telegram page.
pub const SESSIONS_PAGE_SIZE: usize = 10;

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

fn resolve_child_grpc_port(
    map: &HashMap<String, u16>,
    session_key: &str,
) -> anyhow::Result<(String, u16)> {
    if let Some(&p) = map.get(session_key) {
        return Ok((session_key.to_string(), p));
    }
    let mut hits: Vec<(String, u16)> = map
        .iter()
        .filter(|(sid, _)| sid.as_str().starts_with(session_key))
        .map(|(s, &p)| (s.clone(), p))
        .collect();
    match hits.len() {
        0 => anyhow::bail!(
            "No active workflow found for session {:?}. \
Start a workflow from Telegram (recipe → project → agent), or use the full session id.",
            session_key
        ),
        1 => Ok(hits.pop().expect("len 1")),
        _ => anyhow::bail!(
            "Ambiguous session prefix {:?}; use more hex characters or the full session id.",
            session_key
        ),
    }
}

/// Format a single session entry as a Telegram-friendly text line.
pub fn format_session_list_entry(entry: &TelegramSessionEntry) -> String {
    format!(
        "{} · {} · {} · {}",
        entry.label, entry.status, entry.workflow_state, entry.elapsed_display
    )
}

fn telegram_label_for_session_id(session_id: &str) -> String {
    session_telegram_label(session_id).unwrap_or_else(|| session_id.to_string())
}

fn session_list_status_or_placeholders(session_dir: &Path) -> SessionListStatusDisplay {
    match crate::session_list_enrichment::session_list_status_from_session_dir(session_dir) {
        Ok(d) => d,
        Err(_) => SessionListStatusDisplay {
            workflow_goal: "—".to_string(),
            workflow_state: "—".to_string(),
            elapsed_display: "—".to_string(),
            agent: "—".to_string(),
            model: "—".to_string(),
        },
    }
}

/// Extra recipes shown after **More recipes…** (compact `mr:` callbacks — see [`parse_recipe_mr_callback`]).
/// Names must match `tddy-coder --recipe` / [`normalize_recipe_name_for_tddy_coder_cli`].
pub const RECIPE_MORE_PAGE: &[&str] = &["tdd", "bugfix", "free-prompting", "grill-me", "merge-pr"];

/// Default recipe on the first keyboard row; must be a valid `tddy-coder --recipe` value.
pub const TELEGRAM_DEFAULT_RECIPE_CLI: &str = "tdd";

/// Maps Telegram / legacy ids to names accepted by `tddy-coder --recipe` (see `tddy-coder` `validate_recipe_cli`).
pub fn normalize_recipe_name_for_tddy_coder_cli(name: &str) -> String {
    match name.trim() {
        "tdd-small" => "tdd".to_string(),
        s => s.to_string(),
    }
}

/// Compact recipe selection: `mr:<idx>|<session_uuid>` (fits Telegram `callback_data` byte limit for long recipe names).
pub fn parse_recipe_mr_callback(callback_data: &str) -> Option<(usize, String)> {
    let rest = callback_data.strip_prefix("mr:")?;
    let (idx_part, session_id) = rest.split_once('|')?;
    let idx: usize = idx_part.parse().ok()?;
    let session_id = session_id.trim().to_string();
    if session_id.is_empty() || idx >= RECIPE_MORE_PAGE.len() {
        return None;
    }
    Some((idx, session_id))
}

/// Session id from `recipe:…|session:<id>` (and `recipe:more|session:<id>`).
pub fn parse_session_id_from_recipe_callback(callback_data: &str) -> Option<String> {
    for segment in callback_data.split('|') {
        if let Some(id) = segment.strip_prefix("session:") {
            let id = id.trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// Resolve session directory for a start-workflow recipe callback (`recipe:…|session:<id>` or `mr:…`).
pub fn parse_recipe_callback_session_dir(
    callback_data: &str,
    sessions_base: &Path,
) -> Option<PathBuf> {
    if let Some((_, sid)) = parse_recipe_mr_callback(callback_data) {
        return Some(unified_session_dir_path(sessions_base, &sid));
    }
    for segment in callback_data.split('|') {
        if let Some(id) = segment.strip_prefix("session:") {
            let id = id.trim();
            if !id.is_empty() {
                return Some(unified_session_dir_path(sessions_base, id));
            }
        }
    }
    None
}

/// Decode `intent:nb|s:<session_id>` or `intent:ws|s:<session_id>` (and full snake_case slugs).
pub fn parse_telegram_intent_callback(
    callback_data: &str,
) -> Option<(BranchWorktreeIntent, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_INTENT)?;
    let (intent_part, sess_part) = rest.split_once("|s:")?;
    let intent = match intent_part {
        "nb" | "new_branch_from_base" => BranchWorktreeIntent::NewBranchFromBase,
        "ws" | "work_on_selected_branch" => BranchWorktreeIntent::WorkOnSelectedBranch,
        _ => return None,
    };
    let session_id = sess_part.trim().to_string();
    if session_id.is_empty() {
        return None;
    }
    Some((intent, session_id))
}

/// Decode `tp:<proj_idx>|s:<session_id>` (project pick after recipe).
pub fn parse_telegram_project_callback(callback_data: &str) -> Option<(usize, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_PROJECT)?;
    let (idx_part, sess_part) = rest.split_once("|s:")?;
    let proj_idx: usize = idx_part.parse().ok()?;
    let session_id = sess_part.trim().to_string();
    if session_id.is_empty() {
        return None;
    }
    Some((proj_idx, session_id))
}

/// Decode `ta:<agent_idx>|p:<proj_idx>|s:<session_id>`.
pub fn parse_telegram_agent_callback(callback_data: &str) -> Option<(usize, usize, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_AGENT)?;
    let (agent_part, tail) = rest.split_once('|')?;
    let agent_idx: usize = agent_part.parse().ok()?;
    let tail = tail.strip_prefix("p:")?;
    let (proj_part, sess_part) = tail.split_once("|s:")?;
    let proj_idx: usize = proj_part.parse().ok()?;
    let session_id = sess_part.trim().to_string();
    if session_id.is_empty() {
        return None;
    }
    Some((agent_idx, proj_idx, session_id))
}

/// Decode `tb:…|p:…|s:…` (branch pick after project). Optional `|o:<list_offset>` scopes button rows
/// to a page of [`list_recent_remote_branches_skip`] results.
pub fn parse_telegram_branch_callback(
    callback_data: &str,
) -> Option<(usize, usize, usize, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_BRANCH)?;
    let (before_p, after_p) = rest.split_once("|p:")?;
    let (branch_idx, list_offset) = if let Some((idx, off)) = before_p.split_once("|o:") {
        (idx.parse().ok()?, off.parse().ok()?)
    } else {
        (before_p.parse().ok()?, 0usize)
    };
    let (proj_part, sess_part) = after_p.split_once("|s:")?;
    let proj_idx: usize = proj_part.parse().ok()?;
    let session_id = sess_part.trim().to_string();
    if session_id.is_empty() {
        return None;
    }
    if branch_idx > BRANCH_PAGE_SIZE {
        return None;
    }
    Some((branch_idx, list_offset, proj_idx, session_id))
}

/// Decode `tbm:<next_list_offset>|p:<proj_idx>|s:<session_id>` (more remote branches).
pub fn parse_telegram_branch_more_callback(callback_data: &str) -> Option<(usize, usize, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_BRANCH_MORE)?;
    let (off_part, after_p) = rest.split_once("|p:")?;
    let next_offset: usize = off_part.parse().ok()?;
    let (proj_part, sess_part) = after_p.split_once("|s:")?;
    let proj_idx: usize = proj_part.parse().ok()?;
    let session_id = sess_part.trim().to_string();
    if session_id.is_empty() {
        return None;
    }
    Some((next_offset, proj_idx, session_id))
}

/// Parse callback payload strings into internal routing keys (recipe id, demo flags, elicitation ids).
pub fn parse_callback_payload(callback_data: &str) -> Option<String> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "parse_callback_payload: len={}",
        callback_data.len()
    );
    if callback_data.contains("recipe:") {
        return Some(callback_data.to_string());
    }
    None
}

fn parse_demo_options_value(raw: &str) -> anyhow::Result<serde_yaml::Value> {
    let s = raw.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        return serde_yaml::to_value(&v).map_err(Into::into);
    }
    // Accept compact YAML/JSON-like maps from Telegram payloads, e.g. `{run:true}`.
    let normalized = s.replace(":true", ": true").replace(":false", ": false");
    serde_yaml::from_str(&normalized).map_err(Into::into)
}

fn take_utf8_prefix(s: &str, max_bytes: usize) -> &str {
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

fn default_tool_path_for_spawn(config: &DaemonConfig) -> String {
    config
        .allowed_tools()
        .first()
        .map(|t| t.path.clone())
        .unwrap_or_else(|| "tddy-coder".to_string())
}

fn projects_dir_for_telegram_workflow_spawn(
    deps: &TelegramWorkflowSpawn,
) -> anyhow::Result<PathBuf> {
    match &deps.projects_dir_override {
        Some(p) => Ok(p.clone()),
        None => projects_path_for_user(&deps.os_user)
            .ok_or_else(|| anyhow::anyhow!("could not resolve projects path")),
    }
}

fn sorted_projects_for_workflow_spawn(
    deps: &TelegramWorkflowSpawn,
) -> anyhow::Result<Vec<ProjectData>> {
    let projects_dir = projects_dir_for_telegram_workflow_spawn(deps)?;
    let mut projects = project_storage::read_projects(&projects_dir)?;
    projects.sort_by(|a, b| a.project_id.cmp(&b.project_id));
    Ok(projects)
}

fn read_recipe_from_changeset(session_dir: &Path) -> anyhow::Result<Option<String>> {
    let path = session_dir.join("changeset.yaml");
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let snap: ChangesetRoutingSnapshot = serde_yaml::from_str(&raw)?;
    Ok(snap.recipe)
}

/// Spawn + LiveKit configuration for Telegram-driven workflow start (same invariants as web `StartSession`).
#[derive(Clone)]
pub struct TelegramWorkflowSpawn {
    pub config: Arc<DaemonConfig>,
    pub spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    pub os_user: String,
    /// When set (e.g. integration tests), read `projects.yaml` from this directory instead of `~/.tddy/projects`.
    pub projects_dir_override: Option<PathBuf>,
    pub telegram_hooks: Option<Arc<TelegramDaemonHooks>>,
    /// Full session id → child gRPC port (for [`crate::presenter_intent_client`]).
    pub child_grpc_by_session: Arc<Mutex<HashMap<String, u16>>>,
    /// Same cache as [`crate::telegram_notifier::TelegramSessionWatcher`] — full strings for select confirmations.
    pub elicitation_select_options: ElicitationSelectOptionsCache,
    /// Multi-select **Choose recommended** metadata (recommended_other keyed by presenter session id).
    pub elicitation_multi_select_meta: ElicitationMultiSelectMetaCache,
    /// Chat id → session id (full) when the user tapped "Other" and we await a free-text follow-up message.
    pub pending_elicitation_other: Arc<Mutex<HashMap<i64, String>>>,
}

impl TelegramWorkflowSpawn {
    /// Blocking spawn (call from [`tokio::task::spawn_blocking`]).
    pub fn spawn_blocking(
        &self,
        project_id: &str,
        agent: Option<&str>,
        recipe: Option<&str>,
        new_session_id: &str,
    ) -> anyhow::Result<spawner::SpawnResult> {
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| anyhow::anyhow!("LiveKit not configured"))?;
        let projects_dir = projects_path_for_user(&self.os_user)
            .ok_or_else(|| anyhow::anyhow!("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)?
            .ok_or_else(|| anyhow::anyhow!("project not found"))?;
        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            anyhow::bail!("project main repo path does not exist");
        }
        if let Some(a) = agent {
            let a = a.trim();
            if !a.is_empty() {
                let allowed = self.config.allowed_agents();
                if !allowed.is_empty() && !allowed.iter().any(|x| x.id == a) {
                    anyhow::bail!(
                        "agent id {:?} is not listed in allowed_agents (configure daemon YAML)",
                        a
                    );
                }
            }
        }
        let tool_path = default_tool_path_for_spawn(&self.config);
        let spawn_mouse = self.config.spawn_mouse;
        let agent_for_spawn = agent.and_then(|a| {
            let t = a.trim();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        });
        let recipe_for_spawn: Option<String> = recipe
            .map(normalize_recipe_name_for_tddy_coder_cli)
            .and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            });
        let opts = SpawnOptions {
            resume_session_id: None,
            new_session_id: Some(new_session_id),
            project_id: Some(project_id),
            agent: agent_for_spawn,
            mouse: spawn_mouse,
            recipe: recipe_for_spawn.as_deref(),
        };
        if let Some(ref client) = self.spawn_client {
            let req = spawn_worker::build_spawn_request(
                &self.os_user,
                &tool_path,
                repo_path,
                &livekit,
                opts,
                self.config.log.as_ref(),
            );
            client.spawn(req)
        } else {
            let (child_log_level, child_log_format) =
                spawner::child_log_yaml_tuning(self.config.log.as_ref());
            spawner::spawn_as_user(
                &self.os_user,
                &tool_path,
                repo_path,
                &livekit,
                opts,
                child_log_level.as_str(),
                child_log_format.as_str(),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Session control plane: authorized chats, sessions root, and a [`TelegramSender`] (in-memory in tests, teloxide in production).
pub struct TelegramSessionControlHarness<S: TelegramSender + Send + Sync> {
    allowed_chat_ids: Vec<i64>,
    sessions_base: PathBuf,
    sender: Arc<S>,
    workflow_spawn: Option<Arc<TelegramWorkflowSpawn>>,
    /// When set, [`Self::handle_start_workflow`] requires a linked GitHub identity for `user_id`.
    telegram_github_mapping_path: Option<PathBuf>,
    /// Single active elicitation token per Telegram chat (shared with [`crate::telegram_notifier::TelegramSessionWatcher`] when wired in `main`).
    active_elicitation: SharedActiveElicitationCoordinator,
}

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    pub fn new(allowed_chat_ids: Vec<i64>, sessions_base: PathBuf, sender: Arc<S>) -> Self {
        Self::with_workflow_spawn(allowed_chat_ids, sessions_base, sender, None, None)
    }

    /// Same as [`Self::new`], but workflow start checks [`TelegramGithubMappingStore`] at `path`
    /// so unlinked Telegram users receive an explicit error (PRD).
    pub fn with_telegram_github_link(
        allowed_chat_ids: Vec<i64>,
        sessions_base: PathBuf,
        sender: Arc<S>,
        github_mapping_path: PathBuf,
    ) -> Self {
        Self::with_workflow_spawn_and_github_mapping(
            allowed_chat_ids,
            sessions_base,
            sender,
            None,
            Some(github_mapping_path),
            None,
        )
    }

    pub fn with_workflow_spawn(
        allowed_chat_ids: Vec<i64>,
        sessions_base: PathBuf,
        sender: Arc<S>,
        workflow_spawn: Option<Arc<TelegramWorkflowSpawn>>,
        shared_elicitation: Option<SharedActiveElicitationCoordinator>,
    ) -> Self {
        Self::with_workflow_spawn_and_github_mapping(
            allowed_chat_ids,
            sessions_base,
            sender,
            workflow_spawn,
            None,
            shared_elicitation,
        )
    }

    fn with_workflow_spawn_and_github_mapping(
        allowed_chat_ids: Vec<i64>,
        sessions_base: PathBuf,
        sender: Arc<S>,
        workflow_spawn: Option<Arc<TelegramWorkflowSpawn>>,
        telegram_github_mapping_path: Option<PathBuf>,
        shared_elicitation: Option<SharedActiveElicitationCoordinator>,
    ) -> Self {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "TelegramSessionControlHarness::with_workflow_spawn: allowed_chats={} sessions_base={} workflow_spawn={} github_mapping={} shared_elicitation={}",
            allowed_chat_ids.len(),
            sessions_base.display(),
            workflow_spawn.is_some(),
            telegram_github_mapping_path.is_some(),
            shared_elicitation.is_some()
        );
        let active_elicitation = shared_elicitation
            .unwrap_or_else(|| Arc::new(Mutex::new(ActiveElicitationCoordinator::new())));
        Self {
            allowed_chat_ids,
            sessions_base,
            sender,
            workflow_spawn,
            telegram_github_mapping_path,
            active_elicitation,
        }
    }

    /// Root passed at construction (`~/.tddy`); session dirs for listing live under [`SESSIONS_SUBDIR`].
    pub fn sessions_base(&self) -> &Path {
        &self.sessions_base
    }

    /// Whether `chat_id` is allowed to use session control (matches configured `chat_ids`).
    pub fn is_authorized(&self, chat_id: i64) -> bool {
        self.allowed_chat_ids.contains(&chat_id)
    }

    // -------------------------------------------------------------------------
    // Concurrent elicitation (single active token per Telegram chat) — public contract for tests
    // and future inbound/outbound wiring. Implementations live with the per-chat lease/queue.
    // -------------------------------------------------------------------------

    /// Session id that currently owns the **active** elicitation token for this chat, if any.
    ///
    /// Plain-text follow-ups and commands that target the active session (without an explicit
    /// session key) must resolve through this value.
    pub fn active_elicitation_session_for_chat(&self, chat_id: i64) -> Option<String> {
        match self.active_elicitation.lock() {
            Ok(g) => g.active_session_for_chat(chat_id),
            Err(e) => {
                log::error!(
                    target: "tddy_daemon::telegram_session_control",
                    "active_elicitation_session_for_chat: mutex poisoned: {e}"
                );
                None
            }
        }
    }

    /// Register demand for elicitation UI (same entry point as outbound notifier; used when tests
    /// or future inbound paths seed the queue).
    pub fn register_elicitation_surface_request(&self, chat_id: i64, session_id: String) {
        match self.active_elicitation.lock() {
            Ok(mut g) => g.register_elicitation_surface_request(chat_id, session_id),
            Err(e) => log::error!(
                target: "tddy_daemon::telegram_session_control",
                "register_elicitation_surface_request: mutex poisoned: {e}"
            ),
        }
    }

    /// Whether an inbound `eli:s:` / `eli:o:` callback for `session_id` may be applied under the
    /// single-active elicitation policy for this chat.
    pub fn elicitation_callback_permitted(&self, chat_id: i64, session_id: &str) -> bool {
        match self.active_elicitation.lock() {
            Ok(g) => g.elicitation_callback_permitted(chat_id, session_id),
            Err(e) => {
                log::error!(
                    target: "tddy_daemon::telegram_session_control",
                    "elicitation_callback_permitted: mutex poisoned: {e}"
                );
                false
            }
        }
    }

    /// When `completed_session_id` finishes its elicitation step, advance the queue and return the
    /// next session id that becomes active for `chat_id`, if any.
    pub fn advance_after_elicitation_completion(
        &mut self,
        chat_id: i64,
        completed_session_id: &str,
    ) -> Option<String> {
        match self.active_elicitation.lock() {
            Ok(mut g) => g.advance_after_elicitation_completion(chat_id, completed_session_id),
            Err(e) => {
                log::error!(
                    target: "tddy_daemon::telegram_session_control",
                    "advance_after_elicitation_completion: mutex poisoned: {e}"
                );
                None
            }
        }
    }

    fn try_advance_elicitation_after_step(
        &self,
        chat_id: i64,
        completed_session_id: &str,
        context: &'static str,
    ) {
        let next = match self.active_elicitation.lock() {
            Ok(mut g) => g.advance_after_elicitation_completion(chat_id, completed_session_id),
            Err(e) => {
                log::error!(
                    target: "tddy_daemon::telegram_session_control",
                    "{context}: active elicitation mutex poisoned: {e}"
                );
                None
            }
        };
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "{context}: elicitation queue advanced chat_id={} completed_session_id={} next_active_session_id={:?}",
            chat_id,
            completed_session_id,
            next
        );
    }

    fn ensure_authorized(&self, chat_id: i64) -> anyhow::Result<()> {
        if self.allowed_chat_ids.contains(&chat_id) {
            return Ok(());
        }
        anyhow::bail!(
            "chat_id {} is not authorized for Telegram session control",
            chat_id
        )
    }

    /// After recipe selection: ask whether to fork a new branch from the integration base or work on an existing branch.
    pub async fn send_intent_pick_keyboard(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let data_nb = format!("{CB_TELEGRAM_INTENT}nb|s:{session_id}");
        let data_ws = format!("{CB_TELEGRAM_INTENT}ws|s:{session_id}");
        debug_assert!(
            data_nb.len() <= 64 && data_ws.len() <= 64,
            "Telegram callback_data exceeds 64 bytes: nb_len={} ws_len={} session_id_len={}",
            data_nb.len(),
            data_ws.len(),
            session_id.len()
        );
        let rows: InlineKeyboardRows = vec![vec![
            ("New branch + worktree".to_string(), data_nb),
            ("Work on existing branch".to_string(), data_ws),
        ]];
        self.sender
            .send_message_with_keyboard(chat_id, "Choose branch/worktree intent:", rows)
            .await?;
        Ok(())
    }

    /// Persist [`BranchWorktreeIntent`] from Telegram and continue to project selection.
    pub async fn handle_telegram_intent_callback(
        &self,
        chat_id: i64,
        intent: BranchWorktreeIntent,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let mut cs = match read_changeset(&session_dir) {
            Ok(c) => c,
            Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
            Err(e) => anyhow::bail!("read changeset: {e}"),
        };
        cs.workflow
            .get_or_insert_with(Default::default)
            .branch_worktree_intent = Some(intent);
        write_changeset(&session_dir, &cs).map_err(|e| anyhow::anyhow!("write changeset: {e}"))?;
        self.send_project_pick_keyboard(chat_id, session_id).await
    }

    /// After branch/worktree intent is chosen: prompt for a project (then branch, then agent) like the web UI.
    pub async fn send_project_pick_keyboard(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            return Ok(());
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        if projects.is_empty() {
            self.sender
                .send_message(
                    chat_id,
                    "Recipe saved. No projects found for this user — add one via the web UI, then run /start-workflow again.",
                )
                .await?;
            return Ok(());
        }
        let mut rows: InlineKeyboardRows = Vec::new();
        for (i, p) in projects.iter().enumerate() {
            let label = format!("{} ({})", p.name, p.project_id);
            let data = format!("{CB_TELEGRAM_PROJECT}{i}|s:{session_id}");
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        self.sender
            .send_message_with_keyboard(chat_id, "Choose a project for this workflow:", rows)
            .await?;
        Ok(())
    }

    /// After project pick: show default integration base + recent `origin/*` branches (paginated).
    async fn send_branch_pick_keyboard(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
        project: &ProjectData,
        list_offset: usize,
    ) -> anyhow::Result<()> {
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects_dir = projects_dir_for_telegram_workflow_spawn(deps)?;
        let default_ref =
            effective_integration_base_ref_for_project(&projects_dir, &project.project_id)?;
        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            anyhow::bail!("project main repo path does not exist");
        }
        let page_peek =
            match list_recent_remote_branches_skip(repo_path, list_offset, BRANCH_PAGE_SIZE + 1) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::telegram_session_control",
                        "list_recent_remote_branches_skip: {}",
                        e
                    );
                    Vec::new()
                }
            };
        let has_more = page_peek.len() > BRANCH_PAGE_SIZE;
        let branches: Vec<String> = page_peek.into_iter().take(BRANCH_PAGE_SIZE).collect();
        let short_default = default_ref
            .strip_prefix("origin/")
            .unwrap_or(default_ref.as_str());
        let default_label = format!("Default ({short_default})");
        let mut rows: InlineKeyboardRows = Vec::new();
        let data0 = if list_offset == 0 {
            format!("{CB_TELEGRAM_BRANCH}0|p:{proj_idx}|s:{session_id}")
        } else {
            format!("{CB_TELEGRAM_BRANCH}0|o:{list_offset}|p:{proj_idx}|s:{session_id}")
        };
        debug_assert!(
            data0.len() <= 64,
            "Telegram callback_data exceeds 64 bytes: len={} data={data0:?}",
            data0.len()
        );
        rows.push(vec![(default_label, data0)]);
        for (i, br) in branches.iter().enumerate() {
            let idx = i + 1;
            let label = take_utf8_prefix(br, 52).to_string();
            let data = if list_offset == 0 {
                format!("{CB_TELEGRAM_BRANCH}{idx}|p:{proj_idx}|s:{session_id}")
            } else {
                format!("{CB_TELEGRAM_BRANCH}{idx}|o:{list_offset}|p:{proj_idx}|s:{session_id}")
            };
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        if has_more {
            let next_off = list_offset + branches.len();
            let more_data =
                format!("{CB_TELEGRAM_BRANCH_MORE}{next_off}|p:{proj_idx}|s:{session_id}");
            debug_assert!(
                more_data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={more_data:?}",
                more_data.len()
            );
            rows.push(vec![("More…".to_string(), more_data)]);
        }
        let intro = format!(
            "Project `{}` selected. Choose integration base (remote branch):",
            project.project_id
        );
        self.sender
            .send_message_with_keyboard(chat_id, &intro, rows)
            .await?;
        Ok(())
    }

    pub async fn handle_telegram_project_callback(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        self.send_branch_pick_keyboard(chat_id, proj_idx, session_id, project, 0)
            .await
    }

    /// Show another page of remote branches after **More…** (`tbm:…` callback).
    pub async fn handle_telegram_branch_more_callback(
        &self,
        chat_id: i64,
        next_list_offset: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        self.send_branch_pick_keyboard(chat_id, proj_idx, session_id, project, next_list_offset)
            .await
    }

    pub async fn handle_telegram_branch_callback(
        &self,
        chat_id: i64,
        branch_idx: usize,
        list_offset: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            anyhow::bail!("project main repo path does not exist");
        }
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let mut cs = match read_changeset(&session_dir) {
            Ok(c) => c,
            Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
            Err(e) => anyhow::bail!("read changeset: {e}"),
        };
        let intent = cs.workflow.as_ref().and_then(|w| w.branch_worktree_intent);
        let projects_dir = projects_dir_for_telegram_workflow_spawn(deps)?;
        if branch_idx == 0 {
            cs.worktree_integration_base_ref = None;
            if intent == Some(BranchWorktreeIntent::WorkOnSelectedBranch) {
                cs.workflow
                    .get_or_insert_with(Default::default)
                    .selected_branch_to_work_on = None;
            }
            if intent == Some(BranchWorktreeIntent::NewBranchFromBase) {
                let default_ref =
                    effective_integration_base_ref_for_project(&projects_dir, &project.project_id)?;
                let wf = cs.workflow.get_or_insert_with(Default::default);
                wf.selected_integration_base_ref = Some(default_ref);
                wf.selected_branch_to_work_on = None;
            }
        } else {
            let global_idx = list_offset
                .checked_add(branch_idx)
                .and_then(|n| n.checked_sub(1))
                .ok_or_else(|| anyhow::anyhow!("invalid branch index"))?;
            let picked = list_recent_remote_branches_skip(repo_path, global_idx, 1)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let chain = picked
                .first()
                .ok_or_else(|| anyhow::anyhow!("invalid branch index (list may have changed)"))?;
            validate_chain_pr_integration_base_ref(chain).map_err(|e| anyhow::anyhow!(e))?;
            if intent == Some(BranchWorktreeIntent::WorkOnSelectedBranch) {
                cs.workflow
                    .get_or_insert_with(Default::default)
                    .selected_branch_to_work_on = Some(chain.clone());
            } else {
                cs.worktree_integration_base_ref = Some(chain.clone());
            }
            if intent == Some(BranchWorktreeIntent::NewBranchFromBase) {
                let wf = cs.workflow.get_or_insert_with(Default::default);
                wf.selected_integration_base_ref = Some(chain.clone());
                wf.selected_branch_to_work_on = None;
            }
        }
        write_changeset(&session_dir, &cs).map_err(|e| anyhow::anyhow!("write changeset: {e}"))?;
        let allowed = deps.config.allowed_agents();
        if allowed.is_empty() {
            self.spawn_telegram_workflow(chat_id, session_id, &project.project_id, None)
                .await?;
            return Ok(());
        }
        let mut rows: InlineKeyboardRows = Vec::new();
        for (i, a) in allowed.iter().enumerate() {
            let label = a
                .label
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| a.id.clone());
            let data = format!("{CB_TELEGRAM_AGENT}{i}|p:{proj_idx}|s:{session_id}");
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        let intro = format!(
            "Branch saved for `{}`. Choose an agent:",
            project.project_id
        );
        self.sender
            .send_message_with_keyboard(chat_id, &intro, rows)
            .await?;
        Ok(())
    }

    pub async fn handle_telegram_agent_callback(
        &self,
        chat_id: i64,
        agent_idx: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        let allowed = deps.config.allowed_agents();
        let agent = allowed
            .get(agent_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid agent index"))?;
        self.spawn_telegram_workflow(
            chat_id,
            session_id,
            &project.project_id,
            Some(agent.id.as_str()),
        )
        .await
    }

    async fn spawn_telegram_workflow(
        &self,
        chat_id: i64,
        session_id: &str,
        project_id: &str,
        agent: Option<&str>,
    ) -> anyhow::Result<()> {
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?
            .clone();
        let presenter_hooks = deps.telegram_hooks.clone();
        let child_grpc_registry = deps.child_grpc_by_session.clone();
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let recipe = read_recipe_from_changeset(&session_dir)?;
        let timeout = deps.config.spawn_worker_request_timeout();
        let session_id_owned = session_id.to_string();
        let project_id_owned = project_id.to_string();
        let agent_owned = agent.map(|s| s.to_string());
        let recipe_owned = recipe;
        let deps_for_spawn = deps.clone();
        let join = tokio::task::spawn_blocking(move || {
            deps_for_spawn.spawn_blocking(
                &project_id_owned,
                agent_owned.as_deref(),
                recipe_owned.as_deref(),
                &session_id_owned,
            )
        });
        let result = tokio::time::timeout(timeout, join).await;
        let result = match result {
            Err(_elapsed) => anyhow::bail!("spawn timed out after {:?}", timeout),
            Ok(Ok(Ok(r))) => r,
            Ok(Ok(Err(e))) => return Err(e),
            Ok(Err(join_e)) => anyhow::bail!("spawn task join: {join_e}"),
        };
        {
            let mut g = child_grpc_registry
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            g.insert(result.session_id.clone(), result.grpc_port);
        }
        if let Some(ref hooks) = presenter_hooks {
            hooks.spawn_presenter_observer_task(&result.session_id, result.grpc_port);
        }
        let sid_short = {
            let s = result.session_id.as_str();
            &s[..8.min(s.len())]
        };
        let done = format!(
            "Workflow started (session {sid_short}…). gRPC {} LiveKit room `{}`.",
            result.grpc_port, result.livekit_room
        );
        self.sender.send_message(chat_id, &done).await?;
        Ok(())
    }

    /// Deliver feature text to the running child `tddy-coder` via [`PresenterIntent`] (see [`SUBMIT_FEATURE_CMD`]).
    pub async fn handle_submit_feature(
        &self,
        chat_id: i64,
        session_key: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        presenter_intent_client::submit_feature_text_localhost(port, text).await?;
        let short = &session_id[..8.min(session_id.len())];
        self.sender
            .send_message(
                chat_id,
                &format!("Feature text submitted for session {short}…"),
            )
            .await?;
        Ok(())
    }

    /// Forward document-review actions to the child presenter ([`PresenterIntent`]).
    pub async fn handle_document_review_action(
        &self,
        chat_id: i64,
        action: char,
        session_key: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        match action {
            'a' => presenter_intent_client::approve_session_document_localhost(port).await,
            'r' => presenter_intent_client::refine_session_document_localhost(port).await,
            'v' => presenter_intent_client::view_session_document_localhost(port).await,
            'd' => presenter_intent_client::dismiss_viewer_localhost(port).await,
            'j' => presenter_intent_client::reject_session_document_localhost(port).await,
            _ => anyhow::bail!("unknown document action {action:?}"),
        }?;
        // Rotate queue only when the document-review gate is decisively completed (approve/reject).
        if matches!(action, 'a' | 'j') {
            self.try_advance_elicitation_after_step(
                chat_id,
                &session_id,
                "handle_document_review_action",
            );
        }
        Ok(())
    }

    /// Single-select clarification answer ([`PresenterIntent::AnswerClarificationSelect`]).
    pub async fn handle_elicitation_select(
        &self,
        chat_id: i64,
        session_key: &str,
        option_index: usize,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        if let Ok(mut g) = deps.pending_elicitation_other.lock() {
            g.remove(&chat_id);
        }
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        presenter_intent_client::answer_clarification_select_localhost(port, option_index as u32)
            .await?;
        self.try_advance_elicitation_after_step(chat_id, &session_id, "handle_elicitation_select");
        let confirmation = {
            let guard = deps
                .elicitation_select_options
                .lock()
                .map_err(|e| anyhow::anyhow!("elicitation options cache lock: {e}"))?;
            guard
                .get(&session_id)
                .and_then(|v| v.get(option_index))
                .cloned()
        };
        let text = match confirmation {
            Some(full) => format!("You selected:\n{full}"),
            None => format!("You selected option {}.", option_index + 1),
        };
        self.sender.send_message(chat_id, &text).await?;
        Ok(())
    }

    /// User tapped **Other** on a single-select clarification keyboard — next plain message is the answer.
    pub async fn handle_elicitation_other(
        &self,
        chat_id: i64,
        session_key: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (_session_id, _port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        deps.pending_elicitation_other
            .lock()
            .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?
            .insert(chat_id, session_key.to_string());
        self.sender
            .send_message(
                chat_id,
                "Send your custom answer as your next message in this chat.",
            )
            .await?;
        Ok(())
    }

    /// If this chat is awaiting an "Other" free-text answer, consume `body` and forward to the presenter.
    ///
    /// Returns `Ok(true)` when the message was handled (including presenter errors surfaced to the user).
    pub async fn handle_elicitation_other_followup_plain_message(
        &self,
        chat_id: i64,
        body: &str,
    ) -> anyhow::Result<bool> {
        if !self.is_authorized(chat_id) {
            return Ok(false);
        }
        let Some(ref deps) = self.workflow_spawn else {
            return Ok(false);
        };
        let session_key = {
            let g = deps
                .pending_elicitation_other
                .lock()
                .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?;
            match g.get(&chat_id).cloned() {
                Some(s) => s,
                None => return Ok(false),
            }
        };
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, &session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That follow-up does not match the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }
        let trimmed = body.trim();
        if trimmed.is_empty() {
            self.sender
                .send_message(
                    chat_id,
                    "That message was empty — send your custom answer as text, or pick an option on the question.",
                )
                .await?;
            return Ok(true);
        }
        {
            let mut g = deps
                .pending_elicitation_other
                .lock()
                .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?;
            g.remove(&chat_id);
        }
        if let Err(e) =
            presenter_intent_client::answer_clarification_text_localhost(port, trimmed).await
        {
            deps.pending_elicitation_other
                .lock()
                .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?
                .insert(chat_id, session_key);
            return Err(e);
        }
        self.try_advance_elicitation_after_step(
            chat_id,
            &session_id,
            "handle_elicitation_other_followup_plain_message",
        );
        let text = format!("You selected:\n{trimmed}");
        self.sender.send_message(chat_id, &text).await?;
        Ok(true)
    }

    /// Free-text clarification ([`PresenterIntent::AnswerClarificationText`]).
    pub async fn handle_answer_text_command(
        &self,
        chat_id: i64,
        session_key: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        if let Ok(mut g) = deps.pending_elicitation_other.lock() {
            g.remove(&chat_id);
        }
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That session is not the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }
        presenter_intent_client::answer_clarification_text_localhost(port, text).await?;
        self.try_advance_elicitation_after_step(chat_id, &session_id, "handle_answer_text_command");
        Ok(())
    }

    /// Multi-select clarification ([`PresenterIntent::AnswerClarificationMultiSelect`]).
    pub async fn handle_answer_multi_command(
        &self,
        chat_id: i64,
        session_key: &str,
        indices: &[usize],
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That session is not the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }
        let u32s: Vec<u32> = indices.iter().map(|&i| i as u32).collect();
        presenter_intent_client::answer_clarification_multi_select_localhost(
            port,
            u32s,
            String::new(),
        )
        .await?;
        self.try_advance_elicitation_after_step(
            chat_id,
            &session_id,
            "handle_answer_multi_command",
        );
        Ok(())
    }

    /// Inline **Choose none** / **Choose recommended** (`eli:mn:` / `eli:mr:`).
    pub async fn handle_elicitation_multi_select_shortcut(
        &self,
        chat_id: i64,
        session_key: &str,
        question_index: i32,
        kind: ElicitationMultiSelectShortcutKind,
    ) -> anyhow::Result<()> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_elicitation_multi_select_shortcut: chat_id={} kind={:?} question_index={}",
            chat_id,
            kind,
            question_index,
        );
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        if let Ok(mut g) = deps.pending_elicitation_other.lock() {
            g.remove(&chat_id);
        }

        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That session is not the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }

        match kind {
            ElicitationMultiSelectShortcutKind::ChooseNone => {
                log::debug!(
                    target: "tddy_daemon::telegram_session_control",
                    "shortcut ChooseNone → presenter empty indices session_id={}",
                    session_id,
                );
                presenter_intent_client::answer_clarification_multi_select_localhost(
                    port,
                    Vec::new(),
                    String::new(),
                )
                .await?;
            }
            ElicitationMultiSelectShortcutKind::ChooseRecommended => {
                let meta = {
                    let guard = deps
                        .elicitation_multi_select_meta
                        .lock()
                        .map_err(|e| anyhow::anyhow!("elicitation_multi_select_meta lock: {e}"))?;
                    guard.get(&session_id).cloned()
                };

                let Some(meta) = meta else {
                    anyhow::bail!(
                        "No shortcut metadata for this session — use /answer-multi or the web UI."
                    );
                };
                if meta.question_index != question_index {
                    log::warn!(
                        target: "tddy_daemon::telegram_session_control",
                        "shortcut question_index mismatch cached={} callback={}",
                        meta.question_index,
                        question_index
                    );
                    anyhow::bail!("That recommendation shortcut is stale for this question.");
                }

                let trimmed = meta.recommended_other.trim();
                if trimmed.is_empty() {
                    anyhow::bail!("No recommended answer is configured for this step.");
                }

                log::debug!(
                    target: "tddy_daemon::telegram_session_control",
                    "shortcut ChooseRecommended → presenter Other len {}",
                    trimmed.len(),
                );

                presenter_intent_client::answer_clarification_multi_select_localhost(
                    port,
                    Vec::new(),
                    trimmed.to_string(),
                )
                .await?;
            }
        }

        self.try_advance_elicitation_after_step(
            chat_id,
            &session_id,
            "handle_elicitation_multi_select_shortcut",
        );

        let confirm = match kind {
            ElicitationMultiSelectShortcutKind::ChooseNone => {
                "You submitted an empty multi-select (Choose none)."
            }
            ElicitationMultiSelectShortcutKind::ChooseRecommended => {
                "You submitted the recommended answer."
            }
        };
        self.sender.send_message(chat_id, confirm).await?;
        Ok(())
    }

    /// Store `/start-workflow` text as [`Changeset::initial_prompt`] so `tddy-coder` does not block on
    /// [`WorkflowEvent::AwaitingFeatureInput`] when only Telegram drove session creation (no web prompt).
    async fn persist_initial_prompt_to_changeset(
        &self,
        session_dir: &Path,
        prompt: &str,
    ) -> anyhow::Result<()> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let mut cs = match read_changeset(session_dir) {
            Ok(c) => c,
            Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
            Err(e) => anyhow::bail!("read changeset: {e}"),
        };
        cs.initial_prompt = Some(trimmed.to_string());
        write_changeset(session_dir, &cs).map_err(|e| anyhow::anyhow!("write changeset: {e}"))
    }

    async fn persist_recipe_to_changeset(
        &self,
        session_dir: &Path,
        recipe_name: &str,
        demo_options: Option<serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        if recipe_name.is_empty() {
            anyhow::bail!("empty recipe name");
        }
        let path = session_dir.join("changeset.yaml");
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        let mut root: serde_yaml::Value = if raw.trim().is_empty() {
            serde_yaml::Mapping::new().into()
        } else {
            serde_yaml::from_str(&raw)?
        };

        let map = root
            .as_mapping_mut()
            .ok_or_else(|| anyhow::anyhow!("changeset.yaml root must be a mapping"))?;

        let stored = normalize_recipe_name_for_tddy_coder_cli(recipe_name);
        map.insert(
            serde_yaml::Value::String("recipe".into()),
            serde_yaml::Value::String(stored),
        );
        if let Some(d) = demo_options {
            map.insert(serde_yaml::Value::String("demo_options".into()), d);
        }

        let out = serde_yaml::to_string(&root)?;
        std::fs::write(&path, out)?;
        Ok(())
    }

    /// Second page of recipe buttons after **More recipes…** (`recipe:more|session:…`).
    async fn send_more_recipes_keyboard(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<()> {
        let mut rows: InlineKeyboardRows = Vec::with_capacity(RECIPE_MORE_PAGE.len());
        for (i, name) in RECIPE_MORE_PAGE.iter().enumerate() {
            let label = format!("Recipe: {name}");
            let data = format!("mr:{i}|{session_id}");
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        self.sender
            .send_message_with_keyboard(chat_id, "More recipes — choose one:", rows)
            .await?;
        Ok(())
    }

    /// Authorized chat: create session, emit recipe keyboard.
    pub async fn handle_start_workflow(
        &mut self,
        cmd: StartWorkflowCommand,
    ) -> anyhow::Result<StartWorkflowOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow: chat_id={} user_id={} prompt_len={}",
            cmd.chat_id,
            cmd.user_id,
            cmd.prompt.len()
        );
        self.ensure_authorized(cmd.chat_id)?;

        if let Some(ref map_path) = self.telegram_github_mapping_path {
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_start_workflow: github link required; mapping_path={}",
                map_path.display()
            );
            let store = TelegramGithubMappingStore::open(map_path)?;
            if store.get_github_login(cmd.user_id).is_none() {
                log::info!(
                    target: "tddy_daemon::telegram_session_control",
                    "handle_start_workflow: rejected start-workflow — telegram user_id={} has no linked GitHub identity",
                    cmd.user_id
                );
                anyhow::bail!(
                    "Telegram account is not linked to GitHub. Use the bot's /link-github flow (or web OAuth) to connect your GitHub identity before starting a workflow."
                );
            }
        }

        let session_id = Uuid::new_v4().to_string();
        let session_dir = unified_session_dir_path(&self.sessions_base, &session_id);
        std::fs::create_dir_all(&session_dir)?;
        self.persist_initial_prompt_to_changeset(&session_dir, &cmd.prompt)
            .await?;
        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow: created session_dir={}",
            session_dir.display()
        );

        let intro = format!(
            "Workflow started (session {}). Choose a recipe to continue.",
            &session_id[..8.min(session_id.len())]
        );
        let keyboard: InlineKeyboardRows = vec![vec![
            (
                format!("Recipe: {TELEGRAM_DEFAULT_RECIPE_CLI}"),
                format!("recipe:{TELEGRAM_DEFAULT_RECIPE_CLI}|session:{session_id}"),
            ),
            (
                "More recipes…".to_string(),
                format!("recipe:more|session:{session_id}"),
            ),
        ]];
        self.sender
            .send_message_with_keyboard(cmd.chat_id, &intro, keyboard.clone())
            .await?;

        let messages = vec![CapturedTelegramMessage {
            chat_id: cmd.chat_id,
            text: intro,
            inline_keyboard: keyboard,
        }];

        Ok(StartWorkflowOutcome {
            session_id,
            messages,
        })
    }

    /// After recipe selection: persist `changeset.yaml` and continue workflow.
    pub async fn handle_recipe_callback(
        &mut self,
        session_dir: &Path,
        cb: TelegramCallback,
    ) -> anyhow::Result<()> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_recipe_callback: chat_id={} session_dir={}",
            cb.chat_id,
            session_dir.display()
        );
        self.ensure_authorized(cb.chat_id)?;

        if let Some((idx, _sid)) = parse_recipe_mr_callback(&cb.callback_data) {
            let recipe_name = RECIPE_MORE_PAGE[idx];
            self.persist_recipe_to_changeset(session_dir, recipe_name, None)
                .await?;
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_recipe_callback: mr: idx={} recipe={}",
                idx,
                recipe_name
            );
            return Ok(());
        }

        let mut recipe: Option<String> = None;
        let mut demo_options: Option<serde_yaml::Value> = None;

        for segment in cb.callback_data.split('|') {
            if let Some(r) = segment.strip_prefix("recipe:") {
                recipe = Some(r.to_string());
            } else if let Some(rest) = segment.strip_prefix("demo_options:") {
                demo_options = Some(parse_demo_options_value(rest)?);
            }
        }

        if recipe.as_deref() == Some("more") {
            let Some(session_id) = parse_session_id_from_recipe_callback(&cb.callback_data) else {
                anyhow::bail!("recipe:more callback missing session: segment");
            };
            self.send_more_recipes_keyboard(cb.chat_id, &session_id)
                .await?;
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_recipe_callback: sent more recipes keyboard session_id={}",
                session_id
            );
            return Ok(());
        }

        let Some(recipe_name) = recipe else {
            anyhow::bail!("recipe callback missing recipe: segment");
        };

        self.persist_recipe_to_changeset(session_dir, &recipe_name, demo_options)
            .await?;
        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_recipe_callback: wrote changeset.yaml"
        );
        Ok(())
    }

    /// Deliver full plan text (chunked) and record approval callback handling.
    pub async fn handle_plan_review_phase(
        &mut self,
        _session_id: &str,
        golden_plan_text: &str,
        approval_callback: TelegramCallback,
    ) -> anyhow::Result<(Vec<String>, WorkflowTransitionKind)> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_plan_review_phase: chat_id={} plan_len={} approval_data_len={}",
            approval_callback.chat_id,
            golden_plan_text.len(),
            approval_callback.callback_data.len()
        );
        self.ensure_authorized(approval_callback.chat_id)?;

        // Chunk small enough to force continuation markers for typical plans (integration test).
        const CHUNK_MAX: usize = 24;
        let display_chunks = chunk_telegram_text(golden_plan_text, CHUNK_MAX);
        for chunk in &display_chunks {
            self.sender
                .send_message(approval_callback.chat_id, chunk)
                .await?;
        }

        let logical_chunks: Vec<String> = display_chunks
            .iter()
            .map(|c| {
                c.strip_suffix(TELEGRAM_CONTINUATION)
                    .unwrap_or(c.as_str())
                    .to_string()
            })
            .collect();

        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_plan_review_phase: sent_chunks={} transition=PlanReviewApproved",
            display_chunks.len()
        );

        // FIXME: parse approval_callback.callback_data to branch on approve/reject when wired to live teloxide path
        let _ = approval_callback;
        Ok((logical_chunks, WorkflowTransitionKind::PlanReviewApproved))
    }

    /// List sessions under `sessions_base`, paginated `SESSIONS_PAGE_SIZE` at a time.
    /// Sends session entries with inline keyboards ("Enter" + "Delete" per row) and a "More"
    /// button when additional pages exist.
    pub async fn handle_list_sessions(
        &self,
        chat_id: i64,
        offset: usize,
    ) -> anyhow::Result<SessionListPage> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_list_sessions: chat_id={} offset={}",
            chat_id,
            offset
        );
        self.ensure_authorized(chat_id)?;

        let mut sessions = crate::session_reader::list_sessions_in_dir(&self.sessions_base)?;
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total = sessions.len();
        let page_slice: Vec<_> = sessions
            .into_iter()
            .skip(offset)
            .take(SESSIONS_PAGE_SIZE)
            .collect();
        let has_more = offset + page_slice.len() < total;
        let next_offset = offset + page_slice.len();

        let sessions_root = self.sessions_base.join(SESSIONS_SUBDIR);
        let mut entries: Vec<TelegramSessionEntry> = Vec::with_capacity(page_slice.len());

        if page_slice.is_empty() {
            self.sender
                .send_message(chat_id, "No sessions found.")
                .await?;
            return Ok(SessionListPage {
                entries,
                has_more: false,
                next_offset,
            });
        }

        for se in page_slice {
            let session_dir = sessions_root.join(&se.session_id);
            let enrich = session_list_status_or_placeholders(&session_dir);
            let label = telegram_label_for_session_id(&se.session_id);
            let entry = TelegramSessionEntry {
                session_id: se.session_id.clone(),
                label,
                status: se.status.clone(),
                workflow_state: enrich.workflow_state,
                elapsed_display: enrich.elapsed_display,
                is_active: se.is_active,
            };
            let text = format_session_list_entry(&entry);
            let enter_data = format!("{CB_ENTER}{}", se.session_id);
            let delete_data = format!("{CB_DELETE}{}", se.session_id);
            let keyboard: InlineKeyboardRows = vec![vec![
                ("Enter".to_string(), enter_data),
                ("Delete".to_string(), delete_data),
            ]];
            self.sender
                .send_message_with_keyboard(chat_id, &text, keyboard)
                .await?;
            entries.push(entry);
        }

        if has_more {
            let more_data = format!("{CB_MORE}{next_offset}");
            self.sender
                .send_message_with_keyboard(
                    chat_id,
                    "More sessions…",
                    vec![vec![("More".to_string(), more_data)]],
                )
                .await?;
        }

        Ok(SessionListPage {
            entries,
            has_more,
            next_offset,
        })
    }

    /// Delete a session by id. Delegates to `session_deletion::delete_session_directory`.
    /// Sends a confirmation message on success.
    pub async fn handle_delete_session(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<DeleteSessionOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_delete_session: chat_id={} session_id={}",
            chat_id,
            session_id
        );
        self.ensure_authorized(chat_id)?;

        crate::session_deletion::delete_session_directory(&self.sessions_base, session_id)
            .map_err(|s| anyhow::anyhow!("{}", s.message))?;

        let text = format!("Session {} deleted.", session_id);
        self.sender.send_message(chat_id, &text).await?;

        Ok(DeleteSessionOutcome {
            session_id: session_id.to_string(),
            confirmation_message: CapturedTelegramMessage {
                chat_id,
                text: text.clone(),
                inline_keyboard: Vec::new(),
            },
        })
    }

    /// Enter an existing session's workflow. Sends current workflow state and available actions.
    pub async fn handle_enter_session(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<EnterSessionOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_enter_session: chat_id={} session_id={}",
            chat_id,
            session_id
        );
        self.ensure_authorized(chat_id)?;

        let session_dir = self
            .sessions_base
            .join(SESSIONS_SUBDIR)
            .join(session_id.trim());
        let meta_path = session_dir.join(SESSION_METADATA_FILENAME);
        if !session_dir.is_dir() || !meta_path.is_file() {
            anyhow::bail!("session not found: {}", session_id);
        }

        let metadata = read_session_metadata(&session_dir).map_err(|e| anyhow::anyhow!("{e}"))?;
        let enrich = session_list_status_or_placeholders(&session_dir);
        let label = telegram_label_for_session_id(session_id);
        let text = format!(
            "Session {} ({}): status {} · workflow {} · elapsed {}",
            session_id, label, metadata.status, enrich.workflow_state, enrich.elapsed_display
        );
        self.sender.send_message(chat_id, &text).await?;

        let captured = CapturedTelegramMessage {
            chat_id,
            text: text.clone(),
            inline_keyboard: Vec::new(),
        };

        Ok(EnterSessionOutcome {
            session_id: session_id.to_string(),
            messages: vec![captured],
        })
    }

    /// Unauthorized chat: no session creation and no outbound message.
    ///
    /// The inbound [`crate::telegram_bot`] path does not call this for disallowed chats (silent
    /// ignore for multi-daemon deployments). Kept for tests and any caller that needs the same
    /// contract without sending Telegram noise to unrelated channels.
    pub async fn handle_start_workflow_unauthorized(
        &self,
        cmd: StartWorkflowCommand,
    ) -> anyhow::Result<Option<CapturedTelegramMessage>> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow_unauthorized: chat_id={}",
            cmd.chat_id
        );
        if self.allowed_chat_ids.contains(&cmd.chat_id) {
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_start_workflow_unauthorized: chat is authorized — caller should use handle_start_workflow"
            );
            return Ok(None);
        }

        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow_unauthorized: ignoring chat not in allowlist"
        );
        Ok(None)
    }
}

/// Read `changeset.yaml` after Telegram-driven updates (for test assertions).
pub fn read_changeset_routing_snapshot(
    session_dir: &Path,
) -> anyhow::Result<ChangesetRoutingSnapshot> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "read_changeset_routing_snapshot: {}",
        session_dir.display()
    );
    let path = session_dir.join("changeset.yaml");
    let raw = std::fs::read_to_string(&path)?;
    let snap: ChangesetRoutingSnapshot = serde_yaml::from_str(&raw)?;
    Ok(snap)
}

pub fn collect_outbound_messages(
    sender: &InMemoryTelegramSender,
    chat_id: i64,
) -> Vec<CapturedTelegramMessage> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "collect_outbound_messages: chat_id={}",
        chat_id
    );
    sender
        .recorded_with_keyboards()
        .into_iter()
        .filter(|(cid, _, _)| *cid == chat_id)
        .map(|(chat_id, text, inline_keyboard)| CapturedTelegramMessage {
            chat_id,
            text,
            inline_keyboard,
        })
        .collect()
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
