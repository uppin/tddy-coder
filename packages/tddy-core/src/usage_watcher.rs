//! Real-time per-session token-usage watcher.
//!
//! A running session's token usage grows as the agent (and its subagents) produce turns. To let a
//! connected Inspector see live totals — rather than only the end-of-session summary
//! `tddy-sandbox-app` prints — this module re-reads the on-disk token sources
//! ([`crate::backend::gather_session_usage`]) on a fixed interval and broadcasts each snapshot as a
//! [`PresenterEvent::TokenUsageUpdated`] on the session's presenter event channel.
//!
//! Three pieces:
//! - [`SessionUsageEmitter`] owns the broadcast + dedup contract: it broadcasts the *full*
//!   cumulative snapshot on the first gather (so a freshly-connected view sees current totals) and
//!   again whenever the snapshot changes, but never for an unchanged snapshot.
//! - [`spawn_usage_watcher`] drives the emitter from a `tokio::time::interval`, re-gathering usage
//!   from disk each tick. Polling (not `notify`/inotify) keeps this dependency-free and robust to
//!   the several files a session appends to.
//! - [`spawn_session_usage_watcher`] is the session-level entry point `tddy-coder --daemon` calls:
//!   it turns a session's agent/model + on-disk locations into a [`UsageWatchTarget`] (deriving
//!   whether to include the Claude main agent) and delegates to [`spawn_usage_watcher`].
//!
//! TODO: A View that connects mid-session does not yet receive the latest usage snapshot on
//! connect — only subsequent changes. Snapshot-on-connect replay (mirroring the
//! snapshot-then-live pattern in `tddy-service` `service.rs`) is a separate presenter-snapshot
//! change and is tracked as a follow-up.

use std::path::PathBuf;
use std::time::Duration;

use crate::token_accounting::ConversationRecord;
use crate::PresenterEvent;
use tokio::sync::broadcast;

/// Broadcasts per-session token-usage snapshots to presenter subscribers, deduplicating against the
/// last snapshot so an unchanged gather produces no event.
pub struct SessionUsageEmitter {
    event_tx: broadcast::Sender<PresenterEvent>,
    /// The most recently broadcast snapshot, for change detection. `None` until the first emit.
    last: Option<Vec<ConversationRecord>>,
}

impl SessionUsageEmitter {
    /// Create an emitter that broadcasts on `event_tx`.
    pub fn new(event_tx: broadcast::Sender<PresenterEvent>) -> Self {
        Self {
            event_tx,
            last: None,
        }
    }

    /// Broadcast `records` as a [`PresenterEvent::TokenUsageUpdated`] iff it differs from the last
    /// broadcast snapshot (always on the first call). Returns whether a snapshot was broadcast.
    pub fn emit_if_changed(&mut self, records: Vec<ConversationRecord>) -> bool {
        if self.last.as_ref() == Some(&records) {
            return false;
        }
        // Broadcast even when there are no live subscribers: a later subscriber replays the current
        // snapshot on connect, and `last` must track what has been published regardless.
        let _ = self
            .event_tx
            .send(PresenterEvent::TokenUsageUpdated(records.clone()));
        self.last = Some(records);
        true
    }
}

/// Sources a usage watcher re-reads each tick: the session's egress dir plus the Claude home and
/// session id used to locate the agent transcript(s). Mirrors the arguments
/// [`crate::backend::gather_session_usage`] takes.
#[derive(Debug, Clone)]
pub struct UsageWatchTarget {
    /// Session directory whose `egress/accounting.json` carries tddy subagent conversations.
    pub session_dir: PathBuf,
    /// Session id used to locate the Claude transcript and subagent files.
    pub session_id: String,
    /// Claude home holding `.claude/projects/**`.
    pub claude_home: PathBuf,
    /// Model reported for a main-agent row when no transcript exists yet.
    pub fallback_model: String,
    /// Whether to include the main Claude agent + its Task subagents (vs. only the accounting file).
    pub include_main_agent: bool,
}

/// Spawn a background task that polls `target`'s on-disk usage every `poll_interval` and feeds each
/// snapshot to a [`SessionUsageEmitter`], so the session broadcasts live usage on `event_tx`.
///
/// Gathering touches the filesystem, so it runs on the blocking pool. The task ends when the
/// returned handle is aborted (the session owns it for its lifetime) — dropping every subscriber
/// does not stop it, because a later subscriber must still see fresh snapshots.
pub fn spawn_usage_watcher(
    target: UsageWatchTarget,
    poll_interval: Duration,
    event_tx: broadcast::Sender<PresenterEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut emitter = SessionUsageEmitter::new(event_tx);
        let mut ticker = tokio::time::interval(poll_interval);
        loop {
            ticker.tick().await;
            let target = target.clone();
            let gathered = tokio::task::spawn_blocking(move || {
                crate::backend::gather_session_usage(
                    &target.session_dir,
                    &target.session_id,
                    &target.claude_home,
                    &target.fallback_model,
                    target.include_main_agent,
                )
            })
            .await;
            match gathered {
                Ok(records) => {
                    emitter.emit_if_changed(records);
                }
                Err(e) => {
                    log::warn!(target: "tddy_core::usage_watcher", "usage gather task failed: {e}");
                }
            }
        }
    })
}

/// Configuration for the session-level usage watcher `tddy-coder --daemon` starts for a running
/// session. It captures the session's on-disk locations plus the agent/model, from which
/// [`spawn_session_usage_watcher`] derives the [`UsageWatchTarget`].
#[derive(Debug, Clone)]
pub struct SessionUsageWatchConfig {
    /// Session directory whose `egress/accounting.json` carries tddy subagent conversations.
    pub session_dir: PathBuf,
    /// Session id used to locate the Claude transcript and subagent files.
    pub session_id: String,
    /// Child agent HOME holding `.claude/projects/**` (where Claude Code writes its transcript).
    pub claude_home: PathBuf,
    /// Agent driving the session (`claude`, `cursor`, …). Claude sessions include the main agent.
    pub agent: String,
    /// Model reported for a main-agent row when no transcript exists yet.
    pub model: String,
    /// How often to re-read the on-disk usage sources.
    pub poll_interval: Duration,
}

/// Spawn the usage watcher for a running session, broadcasting live token usage on the session's
/// presenter `event_tx`.
///
/// The main Claude agent (and its Task subagents) is included only for Claude sessions; other
/// agents (e.g. Cursor) report their usage solely through the tddy accounting file, so their
/// snapshot must not pick up a stray Claude transcript that may sit on disk.
pub fn spawn_session_usage_watcher(
    config: SessionUsageWatchConfig,
    event_tx: broadcast::Sender<PresenterEvent>,
) -> tokio::task::JoinHandle<()> {
    let target = UsageWatchTarget {
        session_dir: config.session_dir,
        session_id: config.session_id,
        claude_home: config.claude_home,
        fallback_model: config.model,
        include_main_agent: config.agent == "claude",
    };
    spawn_usage_watcher(target, config.poll_interval, event_tx)
}
