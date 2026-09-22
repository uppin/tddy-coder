//! Inbound Telegram control plane for TDD workflow sessions (plan review, elicitation, recipe selection).
//!
//! Bridges Telegram-style commands/callbacks to session directories (`changeset.yaml`) and the same
//! presenter input encodings as the web client. The daemon binary's [`crate::telegram_bot`] module
//! dispatches inbound updates to [`TelegramSessionControlHarness`]; integration tests use [`InMemoryTelegramSender`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use chrono::Utc;
use serde::Deserialize;
use tddy_core::backend::BackendModel;
use tddy_core::changeset::{read_changeset, write_changeset, BranchWorktreeIntent, Changeset};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::{
    integrate_chain_base_into_session_worktree_bootstrap, list_recent_remote_branches_skip,
    read_session_metadata, resolve_chain_integration_base_ref_from_parent_session,
    validate_chain_pr_integration_base_ref, write_initial_tool_session_metadata,
    write_session_metadata, InitialToolSessionMetadataOpts, WorkflowError,
    SESSION_METADATA_FILENAME,
};
use uuid::Uuid;

use crate::telegram_multi_select_shortcuts::{CHOOSE_NONE_CB_PREFIX, CHOOSE_RECOMMENDED_CB_PREFIX};
use crate::telegram_notifier::{
    session_telegram_label, ElicitationMultiSelectMetaCache, ElicitationSelectOptionsCache,
    InMemoryTelegramSender, InlineKeyboardRows, TelegramSender,
};
use crate::telegram_session_subscriber::TelegramDaemonHooks;
use tddy_daemon_kernel::presenter_observer::SharedPresenterEventSink;
use tddy_session_lifecycle::config::DaemonConfig;
use tddy_session_lifecycle::presenter_intent_client;
use tddy_session_lifecycle::project_storage::{
    self, effective_integration_base_ref_for_project, effective_remote_name_for_project,
    ProjectData,
};
use tddy_session_lifecycle::session_list_enrichment::SessionListStatusDisplay;
use tddy_session_lifecycle::user_sessions_path::projects_path_for_user;
use tddy_spawn::spawn_worker;
use tddy_spawn::spawner::{self, SpawnOptions};
use tddy_telegram::active_elicitation::{
    ActiveElicitationCoordinator, SharedActiveElicitationCoordinator,
};
use tddy_telegram::telegram_github_link::TelegramGithubMappingStore;
use tddy_telegram::telegram_tracked_session::{
    SharedTelegramTrackedSessionCoordinator, TelegramTrackedSessionCoordinator,
};

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

mod callbacks;
mod chaining_and_listing;
mod elicitation;
mod pickers;
mod session_start;
mod workflow_spawn;

pub use callbacks::*;
pub use workflow_spawn::*;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Optional bridge so [`TelegramSessionControlHarness::handle_enter_session`] can replay elicitation
/// through a shared [`crate::telegram_notifier::TelegramSessionWatcher`] when `workflow_spawn` has
/// no `telegram_hooks` (integration tests).
#[derive(Default)]
pub struct TelegramElicitationReplayBridge {
    pub config: Option<Arc<DaemonConfig>>,
    pub watcher: Option<Arc<tokio::sync::Mutex<crate::telegram_notifier::TelegramSessionWatcher>>>,
}

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
    /// Telegram-tracked session per chat (Enter session opt-in; shared with notifier when wired in `main`).
    telegram_tracked: SharedTelegramTrackedSessionCoordinator,
    /// When `workflow_spawn` has no [`TelegramDaemonHooks`], tests may install watcher + config here for Enter replay.
    elicitation_replay_bridge: Arc<Mutex<TelegramElicitationReplayBridge>>,
    /// The session rooms this daemon hosts, so **Delete** stops hosting the deleted session's room
    /// before its checkout is removed — the same order `DeleteSession` uses.
    ///
    /// Defaults to a registry of this harness's own, which hosts nothing: a harness built without
    /// the daemon's registry (every test fixture) has no rooms to close, and the call still happens
    /// on one code path rather than on a branch. `main` injects the daemon's own with
    /// [`Self::with_session_rooms`].
    session_rooms: Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry>,
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
            None,
        )
    }

    /// Same as [`Self::with_workflow_spawn`], but shares [`TelegramTrackedSessionCoordinator`] with the notifier.
    pub fn with_workflow_spawn_and_telegram_tracked(
        allowed_chat_ids: Vec<i64>,
        sessions_base: PathBuf,
        sender: Arc<S>,
        workflow_spawn: Option<Arc<TelegramWorkflowSpawn>>,
        shared_elicitation: Option<SharedActiveElicitationCoordinator>,
        shared_tracked: Option<SharedTelegramTrackedSessionCoordinator>,
    ) -> Self {
        Self::with_workflow_spawn_and_github_mapping(
            allowed_chat_ids,
            sessions_base,
            sender,
            workflow_spawn,
            None,
            shared_elicitation,
            shared_tracked,
        )
    }

    fn with_workflow_spawn_and_github_mapping(
        allowed_chat_ids: Vec<i64>,
        sessions_base: PathBuf,
        sender: Arc<S>,
        workflow_spawn: Option<Arc<TelegramWorkflowSpawn>>,
        telegram_github_mapping_path: Option<PathBuf>,
        shared_elicitation: Option<SharedActiveElicitationCoordinator>,
        shared_tracked: Option<SharedTelegramTrackedSessionCoordinator>,
    ) -> Self {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "TelegramSessionControlHarness::with_workflow_spawn: allowed_chats={} sessions_base={} workflow_spawn={} github_mapping={} shared_elicitation={} shared_tracked={}",
            allowed_chat_ids.len(),
            sessions_base.display(),
            workflow_spawn.is_some(),
            telegram_github_mapping_path.is_some(),
            shared_elicitation.is_some(),
            shared_tracked.is_some()
        );
        let active_elicitation = shared_elicitation
            .unwrap_or_else(|| Arc::new(Mutex::new(ActiveElicitationCoordinator::new())));
        let telegram_tracked = shared_tracked
            .unwrap_or_else(|| Arc::new(Mutex::new(TelegramTrackedSessionCoordinator::new())));
        Self {
            allowed_chat_ids,
            sessions_base,
            sender,
            workflow_spawn,
            telegram_github_mapping_path,
            active_elicitation,
            telegram_tracked,
            elicitation_replay_bridge: Arc::new(Mutex::new(
                TelegramElicitationReplayBridge::default(),
            )),
            session_rooms: Arc::new(tddy_daemon_livekit::session_room::SessionRoomRegistry::new()),
        }
    }

    /// Share the daemon's session-room registry (builder), so **Delete** closes the room of a
    /// session it deletes instead of leaving it polling a checkout that is being removed.
    pub fn with_session_rooms(
        mut self,
        rooms: Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry>,
    ) -> Self {
        self.session_rooms = rooms;
        self
    }

    /// Wire [`TelegramSessionWatcher`] + [`DaemonConfig`] for **Enter session** elicitation replay when
    /// this harness is built without [`TelegramWorkflowSpawn::telegram_hooks`] (e.g. integration tests).
    pub fn connect_telegram_elicitation_replay_bridge(
        &self,
        config: DaemonConfig,
        watcher: Arc<tokio::sync::Mutex<crate::telegram_notifier::TelegramSessionWatcher>>,
    ) {
        let mut b = self
            .elicitation_replay_bridge
            .lock()
            .expect("elicitation_replay_bridge poisoned");
        b.config = Some(Arc::new(config));
        b.watcher = Some(watcher);
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "connect_telegram_elicitation_replay_bridge: installed"
        );
    }

    async fn maybe_replay_elicitation_after_enter_session(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<()> {
        if let Some(spawn) = &self.workflow_spawn {
            if let Some(hooks) = &spawn.telegram_hooks {
                let mut w = hooks.watcher.lock().await;
                w.replay_telegram_elicitation_after_tracked_enter(
                    &hooks.config,
                    self.sender.as_ref(),
                    chat_id,
                    session_id,
                )
                .await?;
                return Ok(());
            }
        }
        let (cfg_arc, watcher_arc) = {
            let b = self
                .elicitation_replay_bridge
                .lock()
                .expect("elicitation_replay_bridge poisoned");
            (b.config.clone(), b.watcher.clone())
        };
        if let (Some(cfg), Some(w)) = (cfg_arc, watcher_arc) {
            let mut wg = w.lock().await;
            wg.replay_telegram_elicitation_after_tracked_enter(
                cfg.as_ref(),
                self.sender.as_ref(),
                chat_id,
                session_id,
            )
            .await?;
        }
        Ok(())
    }

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

    fn ensure_authorized(&self, chat_id: i64) -> anyhow::Result<()> {
        if self.allowed_chat_ids.contains(&chat_id) {
            return Ok(());
        }
        anyhow::bail!(
            "chat_id {} is not authorized for Telegram session control",
            chat_id
        )
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
