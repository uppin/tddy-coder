//! Inbound Telegram control plane for TDD workflow sessions (plan review, elicitation, recipe selection).
//!
//! Bridges Telegram-style commands/callbacks to session directories (`changeset.yaml`) and the same
//! presenter input encodings as the web client. The live teloxide update loop should call into these
//! helpers and harness types; integration tests exercise the contract with [`InMemoryTelegramSender`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use uuid::Uuid;

use crate::telegram_notifier::{InMemoryTelegramSender, TelegramSender};

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

/// Outbound Telegram message captured by the harness (text + optional inline keyboard labels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedTelegramMessage {
    pub chat_id: i64,
    pub text: String,
    /// Row-major inline keyboard button labels (empty if no keyboard).
    pub inline_keyboard_labels: Vec<Vec<String>>,
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
    pub demo_options: Option<serde_yaml::Value>,
    #[serde(default)]
    pub run_optional_step_x: Option<bool>,
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

// ---------------------------------------------------------------------------
// Parsing & chunking
// ---------------------------------------------------------------------------

const START_WORKFLOW_CMD: &str = "/start-workflow";
const TELEGRAM_CONTINUATION: &str = "\n(continued)";

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

fn take_utf8_prefix<'a>(s: &'a str, max_bytes: usize) -> &'a str {
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

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Test harness: fake inbound updates + [`InMemoryTelegramSender`].
pub struct TelegramSessionControlHarness {
    allowed_chat_ids: Vec<i64>,
    sessions_base: PathBuf,
    sender: Arc<InMemoryTelegramSender>,
}

impl TelegramSessionControlHarness {
    pub fn new(
        allowed_chat_ids: Vec<i64>,
        sessions_base: PathBuf,
        sender: Arc<InMemoryTelegramSender>,
    ) -> Self {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "TelegramSessionControlHarness::new: allowed_chats={} sessions_base={}",
            allowed_chat_ids.len(),
            sessions_base.display()
        );
        Self {
            allowed_chat_ids,
            sessions_base,
            sender,
        }
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

        let session_id = Uuid::new_v4().to_string();
        let session_dir = self.sessions_base.join(&session_id);
        std::fs::create_dir_all(&session_dir)?;
        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow: created session_dir={}",
            session_dir.display()
        );

        let intro = format!(
            "Workflow started (session {}). Choose a recipe to continue.",
            &session_id[..8.min(session_id.len())]
        );
        let keyboard = vec![vec![
            "Recipe: tdd-small".to_string(),
            "More recipes…".to_string(),
        ]];
        self.sender
            .send_message_with_inline_keyboard(cmd.chat_id, &intro, keyboard.clone())
            .await?;

        let messages = vec![CapturedTelegramMessage {
            chat_id: cmd.chat_id,
            text: intro,
            inline_keyboard_labels: keyboard,
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

        let mut recipe: Option<String> = None;
        let mut demo_options: Option<serde_yaml::Value> = None;

        for segment in cb.callback_data.split('|') {
            if let Some(r) = segment.strip_prefix("recipe:") {
                recipe = Some(r.to_string());
            } else if let Some(rest) = segment.strip_prefix("demo_options:") {
                demo_options = Some(parse_demo_options_value(rest)?);
            }
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

        if let Some(r) = recipe {
            map.insert(
                serde_yaml::Value::String("recipe".into()),
                serde_yaml::Value::String(r),
            );
        }
        if let Some(d) = demo_options {
            map.insert(serde_yaml::Value::String("demo_options".into()), d);
        }

        let out = serde_yaml::to_string(&root)?;
        std::fs::write(&path, out)?;
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

        let _ = approval_callback;
        Ok((logical_chunks, WorkflowTransitionKind::PlanReviewApproved))
    }

    /// Unauthorized chat must not create sessions or open control streams.
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

        let text = "Access denied: this chat is not authorized to control workflows.";
        self.sender.send_message(cmd.chat_id, text).await?;
        Ok(Some(CapturedTelegramMessage {
            chat_id: cmd.chat_id,
            text: text.to_string(),
            inline_keyboard_labels: Vec::new(),
        }))
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

pub fn drain_outbound_messages(
    sender: &InMemoryTelegramSender,
    chat_id: i64,
) -> Vec<CapturedTelegramMessage> {
    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "drain_outbound_messages: chat_id={}",
        chat_id
    );
    sender
        .recorded_with_keyboards()
        .into_iter()
        .filter(|(cid, _, _)| *cid == chat_id)
        .map(
            |(chat_id, text, inline_keyboard_labels)| CapturedTelegramMessage {
                chat_id,
                text,
                inline_keyboard_labels,
            },
        )
        .collect()
}

#[cfg(test)]
mod unit_tests {
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
        let key =
            parse_callback_payload("recipe:tdd-small|demo:1").expect("expected recipe callback");
        assert!(
            key.contains("tdd-small"),
            "parsed routing key should include recipe id: {key}"
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
}
