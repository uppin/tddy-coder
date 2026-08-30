//! The `session` metadata block a coder session publishes on its LiveKit participant.
//!
//! On a workflow state transition the coder updates its [`SessionMetadata`] snapshot, serializes it
//! into a JSON document `{ "session": { ... } }` and sends it on the `metadata_tx` watch channel.
//! The existing `spawn_local_participant_metadata_watcher` shallow-merges that document into the
//! participant's wire metadata (preserving `owned_project_count` / `codex_oauth`) and calls
//! `set_metadata`.
//!
//! The block's *shape* is [`tddy_core::session_participant_metadata`]'s, not this module's: the
//! daemon publishes the same block for a `claude-cli` session, and because the merge is shallow two
//! publishers emitting two different `session` shapes would erase each other's keys rather than
//! merge. What lives here is the coder-specific half — what the snapshot starts as, and how a
//! `PresenterEvent` moves it forward.

use tddy_core::{AppMode, PresenterEvent};
use tddy_livekit::merge_participant_metadata_json;

/// The session block published under the `session` key — the one shape both publishers emit, so a
/// shallow merge of either is lossless. See the module docs.
pub use tddy_core::session_participant_metadata::{
    session_metadata_json, SessionParticipantMetadata as SessionMetadata,
};

/// Shallow-merge a `session` JSON document into an existing participant metadata baseline,
/// preserving sibling keys (`owned_project_count`, `codex_oauth`).
///
/// On a merge error (malformed baseline) the `session` document is returned alone — matching the
/// existing watcher's degrade-on-error behaviour (log + publish the update rather than drop it).
pub fn merge_session_metadata(baseline: &str, session_json: &str) -> String {
    match merge_participant_metadata_json(baseline, session_json) {
        Ok(merged) => merged,
        Err(e) => {
            log::warn!(
                target: "tddy_coder::metadata",
                "merge_session_metadata failed (baseline_len={}): {e}; publishing session block alone",
                baseline.len()
            );
            session_json.to_string()
        }
    }
}

/// Static seed for the session-metadata tap: values known at spawn time (CLI args, the session's
/// own changeset) that should appear on the first publish, before any workflow event lands.
///
/// The stack association is seeded rather than filled in by an event because it is true from the
/// moment the session exists, and it is what a PR-Stack view one host over joins the session's row
/// on (D37). A session that is nobody's stack child leaves those fields empty, and they are
/// published empty rather than omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetadataSeed {
    pub agent: String,
    pub model: String,
    pub recipe: String,
    pub repo_path: String,
    /// This session's own id.
    pub session_id: String,
    /// The pr-stack orchestrator that spawned this session (`--stack-parent`), if any.
    pub orchestrator_session_id: String,
    /// The planned node this session materializes (`--stack-node-id`), if any.
    pub stack_node_id: String,
    /// The branch this session works on.
    pub branch: String,
}

/// Build the initial `SessionMetadata` snapshot from a seed. Workflow-derived fields
/// (`goal`, `state`, `pending_elicitation`, …) start empty / false and are filled by events.
pub fn a_default_session_metadata(seed: &SessionMetadataSeed) -> SessionMetadata {
    SessionMetadata {
        goal: String::new(),
        state: String::new(),
        agent: seed.agent.clone(),
        model: seed.model.clone(),
        activity_status: String::new(),
        recipe: seed.recipe.clone(),
        repo_path: seed.repo_path.clone(),
        elapsed_display: String::new(),
        pending_elicitation: false,
        session_id: seed.session_id.clone(),
        orchestrator_session_id: seed.orchestrator_session_id.clone(),
        stack_node_id: seed.stack_node_id.clone(),
        branch: seed.branch.clone(),
    }
}

/// Apply a [`PresenterEvent`] to a `SessionMetadata` snapshot.
///
/// Returns `Some(updated)` when the event changes a published field (so the new snapshot should be
/// serialized and pushed onto the metadata watch channel), or `None` when the event is irrelevant
/// or carries no change. `StateChanged` is authoritative for `workflow_state`; `ModeChanged` only
/// fills `state` as a fallback (and drives `pending_elicitation` from the elicitation modes).
pub fn apply_session_metadata_event(
    meta: &SessionMetadata,
    event: &PresenterEvent,
) -> Option<SessionMetadata> {
    let mut next = meta.clone();
    let mut changed = false;
    match event {
        PresenterEvent::BackendSelected { agent, model } => {
            if !agent.is_empty() && next.agent != *agent {
                next.agent = agent.clone();
                changed = true;
            }
            if !model.is_empty() && next.model != *model {
                next.model = model.clone();
                changed = true;
            }
        }
        PresenterEvent::GoalStarted(goal) => {
            if next.goal != *goal {
                next.goal = goal.clone();
                changed = true;
            }
        }
        PresenterEvent::StateChanged { to, .. } => {
            if next.state != *to {
                next.state = to.clone();
                changed = true;
            }
        }
        PresenterEvent::ModeChanged(details) => {
            let pending = matches!(
                details.mode,
                AppMode::Select { .. }
                    | AppMode::MultiSelect { .. }
                    | AppMode::TextInput { .. }
                    | AppMode::FeatureInput
            );
            if next.pending_elicitation != pending {
                next.pending_elicitation = pending;
                changed = true;
            }
            // Only seed `state` from the mode when no `StateChanged` has set it yet — `StateChanged`
            // is the authoritative workflow-state source.
            let mode_state: &str = match &details.mode {
                AppMode::Done => "done",
                AppMode::ErrorRecovery { .. } => "error",
                AppMode::FeatureInput => "awaiting-feature",
                AppMode::Running => "running",
                _ => "",
            };
            if !mode_state.is_empty() && next.state.is_empty() {
                next.state = mode_state.to_string();
                changed = true;
            }
        }
        PresenterEvent::WorkflowComplete(res) => {
            let new_state = match res {
                Ok(_) => "done",
                Err(_) => "error",
            }
            .to_string();
            if next.state != new_state {
                next.state = new_state;
                changed = true;
            }
        }
        _ => {}
    }
    if changed {
        Some(next)
    } else {
        None
    }
}

/// Spawn a tokio task that taps `PresenterEvent`s into `session` metadata published on
/// `metadata_tx`.
///
/// Maintains a `SessionMetadata` snapshot updated via [`apply_session_metadata_event`] and sends
/// [`session_metadata_json`] on `metadata_tx` whenever the snapshot changes (and once at startup,
/// so presence carries the seeded agent/model/recipe/repo_path immediately). The channel carries
/// only the `session` delta — the LiveKit metadata watcher merges each publish into the
/// participant's wire metadata (preserving sibling keys like `codex_oauth` / `owned_project_count`).
/// The task ends when `event_rx` is closed (presenter dropped) or all senders are gone.
pub fn spawn_session_metadata_tap(
    mut event_rx: tokio::sync::broadcast::Receiver<PresenterEvent>,
    metadata_tx: tokio::sync::watch::Sender<String>,
    seed: SessionMetadataSeed,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut meta = a_default_session_metadata(&seed);
        // Publish an initial snapshot so the participant advertises the seeded fields before the
        // first workflow transition lands.
        let _ = metadata_tx.send(session_metadata_json(&meta));
        while let Ok(event) = event_rx.recv().await {
            if let Some(updated) = apply_session_metadata_event(&meta, &event) {
                meta = updated;
                let _ = metadata_tx.send(session_metadata_json(&meta));
            }
        }
    })
}

#[cfg(test)]
mod tests;
