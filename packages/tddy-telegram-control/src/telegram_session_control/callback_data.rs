use crate::telegram_session_control::callbacks::BRANCH_PAGE_SIZE;
use crate::telegram_session_control::callbacks::CB_TELEGRAM_AGENT;
use crate::telegram_session_control::callbacks::CB_TELEGRAM_BRANCH;
use crate::telegram_session_control::callbacks::CB_TELEGRAM_BRANCH_CONFLICT;
use crate::telegram_session_control::callbacks::CB_TELEGRAM_BRANCH_MORE;
use crate::telegram_session_control::callbacks::CB_TELEGRAM_CHAIN_PARENT;
use crate::telegram_session_control::callbacks::CB_TELEGRAM_INTENT;
use crate::telegram_session_control::callbacks::CB_TELEGRAM_PROJECT;
use crate::telegram_session_control::*;

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

/// Decode `tcp:p:<parent_tail8>|s:<child_session_id>`.
///
/// `parent_tail8` is the last 8 characters of the parent session id — used to locate the parent
/// in the candidate page without depending on list position (stable across session churn).
/// Returns `(parent_tail8, child_session_id)`.
pub fn parse_telegram_chain_parent_callback(callback_data: &str) -> Option<(String, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_CHAIN_PARENT)?;
    let rest = rest.strip_prefix("p:")?;
    let (tail_part, sess_part) = rest.split_once("|s:")?;
    let parent_tail = tail_part.trim().to_string();
    let session_id = sess_part.trim().to_string();
    if parent_tail.is_empty() || session_id.is_empty() {
        return None;
    }
    Some((parent_tail, session_id))
}

/// Last 8 chars of `session_id` — used as the stable parent discriminator in chain callbacks.
pub(super) fn session_tail8(session_id: &str) -> &str {
    let len = session_id.len();
    &session_id[len.saturating_sub(8)..]
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

/// The operator's answer to the branch-conflict prompt — one of the three choices the PRD offers
/// when the branch a `/start-claude` session derived is already owned by another session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramBranchConflictChoice {
    /// Enter the owning session; the pending session stays un-spawned.
    SwitchToOwner,
    /// Put a second agent on the owned branch (`work_on_selected_branch`, shared worktree).
    NewAgentOnOwnedBranch,
    /// Create the first free `<branch>-<n>` instead.
    UseSuggestedName,
}

impl TelegramBranchConflictChoice {
    /// The `<choice>` segment of [`CB_TELEGRAM_BRANCH_CONFLICT`] payloads, so the keyboard and
    /// [`parse_telegram_branch_conflict_callback`] cannot drift apart.
    pub fn callback_code(self) -> &'static str {
        match self {
            Self::SwitchToOwner => "sw",
            Self::NewAgentOnOwnedBranch => "na",
            Self::UseSuggestedName => "sg",
        }
    }
}

/// Decode `tbc:<choice>:<proj_idx>:<session_id>` (branch-conflict prompt).
pub fn parse_telegram_branch_conflict_callback(
    callback_data: &str,
) -> Option<(TelegramBranchConflictChoice, usize, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_BRANCH_CONFLICT)?;
    let (choice_part, after_choice) = rest.split_once(':')?;
    let choice = match choice_part {
        "sw" => TelegramBranchConflictChoice::SwitchToOwner,
        "na" => TelegramBranchConflictChoice::NewAgentOnOwnedBranch,
        "sg" => TelegramBranchConflictChoice::UseSuggestedName,
        _ => return None,
    };
    let (proj_part, sess_part) = after_choice.split_once(':')?;
    let proj_idx: usize = proj_part.parse().ok()?;
    let session_id = sess_part.trim().to_string();
    if session_id.is_empty() {
        return None;
    }
    Some((choice, proj_idx, session_id))
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
