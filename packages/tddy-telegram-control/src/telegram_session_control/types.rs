use crate::telegram_notifier::InlineKeyboardRows;
use serde::Deserialize;
use tddy_core::changeset::BranchWorktreeIntent;

/// Simulated Telegram `/start-workflow` or reply payload.
#[derive(Debug, Clone)]
pub struct StartWorkflowCommand {
    pub chat_id: i64,
    pub user_id: u64,
    /// Full text after `/start-workflow` (trimmed).
    pub prompt: String,
}

/// Simulated Telegram `/start-claude <prompt>` command to launch a Claude Code CLI session.
#[derive(Debug, Clone)]
pub struct StartClaudeCommand {
    pub chat_id: i64,
    pub user_id: u64,
    /// Full text after `/start-claude` (trimmed) — seeds the first user prompt for the CLI.
    pub prompt: String,
}

/// Simulated Telegram `/start-cursor <prompt>` command to launch a Cursor Agent CLI session.
#[derive(Debug, Clone)]
pub struct StartCursorCommand {
    pub chat_id: i64,
    pub user_id: u64,
    pub prompt: String,
}

/// Simulated Telegram `/chain-workflow` (stacked session) with feature text; parent session is
/// selected in a first step before project / integration base / agent (PRD).
#[derive(Debug, Clone)]
pub struct ChainWorkflowCommand {
    pub chat_id: i64,
    pub user_id: u64,
    /// Full text after `/chain-workflow` (trimmed) — same role as [`StartWorkflowCommand::prompt`].
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

/// Result of handling `/start-claude`: session id + anything sent to Telegram.
#[derive(Debug, Clone)]
pub struct StartClaudeOutcome {
    pub session_id: String,
    pub messages: Vec<CapturedTelegramMessage>,
}

/// Result of handling `/start-cursor`.
#[derive(Debug, Clone)]
pub struct StartCursorOutcome {
    pub session_id: String,
    pub messages: Vec<CapturedTelegramMessage>,
}

/// Parsed `changeset.yaml` fields relevant to Telegram-driven routing (subset).
#[derive(Debug, Default, Deserialize, PartialEq)]
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
    /// Set to `"claude-cli"` by [`TelegramSessionControlHarness::handle_start_claude`] so the
    /// branch → model callbacks can route to the claude-cli spawn path instead of tddy-coder.
    #[serde(default)]
    pub session_type: Option<String>,
    /// Chosen Claude model for claude-cli sessions, written by the model-picker callback.
    #[serde(default)]
    pub model: Option<String>,
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
