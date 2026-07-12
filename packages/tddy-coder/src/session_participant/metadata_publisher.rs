//! `SessionMetadata` + `session` metadata JSON publisher.
//!
//! On a workflow state transition the coder serializes its `SessionMetadata` into a JSON document
//! `{ "session": { ... } }` and sends it on the `metadata_tx` watch channel. The existing
//! `spawn_local_participant_metadata_watcher` shallow-merges that document into the participant's
//! wire metadata (preserving `owned_project_count` / `codex_oauth`) and calls `set_metadata`.

use serde::Serialize;
use tddy_core::{AppMode, PresenterEvent};
use tddy_livekit::merge_participant_metadata_json;

/// Session metadata published on the coder's LiveKit participant under the `session` key.
///
/// JSON key names mirror what the web parses in `parseSessionParticipantMetadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetadata {
    pub goal: String,
    pub state: String,
    pub agent: String,
    pub model: String,
    pub activity_status: String,
    pub recipe: String,
    pub repo_path: String,
    pub elapsed_display: String,
    pub pending_elicitation: bool,
}

#[derive(Serialize)]
struct SessionMetadataJson {
    #[serde(rename = "workflow_goal")]
    workflow_goal: String,
    #[serde(rename = "workflow_state")]
    workflow_state: String,
    agent: String,
    model: String,
    activity_status: String,
    recipe: String,
    #[serde(rename = "repo_path")]
    repo_path: String,
    #[serde(rename = "elapsed_display")]
    elapsed_display: String,
    #[serde(rename = "pending_elicitation")]
    pending_elicitation: bool,
}

impl From<&SessionMetadata> for SessionMetadataJson {
    fn from(m: &SessionMetadata) -> Self {
        SessionMetadataJson {
            workflow_goal: m.goal.clone(),
            workflow_state: m.state.clone(),
            agent: m.agent.clone(),
            model: m.model.clone(),
            activity_status: m.activity_status.clone(),
            recipe: m.recipe.clone(),
            repo_path: m.repo_path.clone(),
            elapsed_display: m.elapsed_display.clone(),
            pending_elicitation: m.pending_elicitation,
        }
    }
}

/// Serialize a `SessionMetadata` into the JSON document published on the participant.
///
/// The document is `{ "session": { workflow_goal, workflow_state, elapsed_display, agent, model,
/// activity_status, recipe, repo_path, pending_elicitation } }`.
pub fn session_metadata_json(meta: &SessionMetadata) -> String {
    let payload = SessionMetadataJson::from(meta);
    let session = match serde_json::to_value(&payload) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(target: "tddy_coder::metadata", "session_metadata_json serialize failed: {e}");
            serde_json::Value::Null
        }
    };
    serde_json::json!({ "session": session }).to_string()
}

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

/// Static seed for the session-metadata tap: values known at spawn time (CLI args) that should
/// appear on the first publish, before any workflow event lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetadataSeed {
    pub agent: String,
    pub model: String,
    pub recipe: String,
    pub repo_path: String,
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
