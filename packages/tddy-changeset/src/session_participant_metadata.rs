//! The `session` block a session's LiveKit participant publishes about itself.
//!
//! Presence is the only cross-host signal the web has about a session: `ListSessions` does not fan
//! out, so a row for a session running on another daemon is synthesized from the common room's
//! participants and hydrated from whatever that participant publishes about itself. The block
//! therefore carries not just what the session is doing but **what it belongs to** — the session's
//! own id, the orchestrator that spawned it, the planned pr-stack node it materializes and the
//! branch it created (D37, docs/ft/coder/pr-stack-live-status.md).
//!
//! The shape lives in `tddy-core` rather than in the publisher because there are now **two**
//! publishers: `tddy-coder`, for the sessions it runs, and `tddy-daemon`, for a `claude-cli`
//! session whose LiveKit bridge previously published nothing at all. Participant metadata is merged
//! **shallowly** ([`merge_participant_metadata_json`]), so a `session` object carrying
//! only one publisher's subset would *replace* the other's rather than merge into it. One owned
//! shape, with every key always emitted, is what keeps that merge lossless — and it is also what
//! lets a reader tell "this session has no orchestrator" from "an older publisher omitted the key",
//! since the first is an empty string and the second cannot occur.

use serde::Serialize;

/// Everything a session's participant publishes about itself under the `session` metadata key.
///
/// Field names here are the Rust-side ones; the JSON keys the web parses are pinned by
/// [`SessionMetadataJson`] and are not free to drift.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionParticipantMetadata {
    /// The workflow goal in progress (`workflow_goal`).
    pub goal: String,
    /// The workflow state in progress (`workflow_state`).
    pub state: String,
    /// The coding agent behind the session ("claude", "cursor", …).
    pub agent: String,
    /// The model the agent runs.
    pub model: String,
    /// A short human-readable activity line ("Running", …).
    pub activity_status: String,
    /// The workflow recipe the session runs ("tdd", "pr-stack", …).
    pub recipe: String,
    /// The checkout the session works in.
    pub repo_path: String,
    /// Pre-rendered elapsed time, so a reader neither computes nor clock-skews it.
    pub elapsed_display: String,
    /// The session is waiting for an operator answer.
    pub pending_elicitation: bool,
    /// The session's own id. The web recovers it from the participant identity too, but a block
    /// that does not name itself cannot be checked against that.
    pub session_id: String,
    /// The pr-stack orchestrator that spawned this session. Empty for a session that is nobody's
    /// stack child — which is a fact, not a missing value.
    pub orchestrator_session_id: String,
    /// The planned node in that orchestrator's stack this session materializes. The exact identity:
    /// it survives a branch rename and it survives the host boundary.
    pub stack_node_id: String,
    /// The branch this session works on.
    pub branch: String,
}

/// The wire shape of the `session` block. Every key is always serialized — see the module docs: a
/// partial object would erase the other publisher's keys on the shallow merge.
#[derive(Serialize)]
struct SessionMetadataJson {
    workflow_goal: String,
    workflow_state: String,
    agent: String,
    model: String,
    activity_status: String,
    recipe: String,
    repo_path: String,
    elapsed_display: String,
    pending_elicitation: bool,
    session_id: String,
    orchestrator_session_id: String,
    stack_node_id: String,
    branch: String,
}

impl From<&SessionParticipantMetadata> for SessionMetadataJson {
    fn from(m: &SessionParticipantMetadata) -> Self {
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
            session_id: m.session_id.clone(),
            orchestrator_session_id: m.orchestrator_session_id.clone(),
            stack_node_id: m.stack_node_id.clone(),
            branch: m.branch.clone(),
        }
    }
}

/// Serialize the block into the document a publisher sends on its metadata watch:
/// `{ "session": { … } }`.
///
/// The wrapper object is what makes the publish a *delta*: the participant's metadata watcher
/// shallow-merges this document into the wire metadata, leaving sibling keys
/// (`owned_project_count`, `codex_oauth`) untouched.
pub fn session_metadata_json(meta: &SessionParticipantMetadata) -> String {
    let payload = SessionMetadataJson::from(meta);
    match serde_json::to_value(&payload) {
        Ok(session) => serde_json::json!({ "session": session }).to_string(),
        Err(e) => {
            // An empty payload, never `{"session": null}`. The merge is **shallow**: a `null`
            // would replace the other publisher's `session` object outright rather than leave it
            // alone, which is precisely the mutual erasure this module was centralised to prevent.
            // The watcher skips an empty string (its `v.is_empty()` guard), so nothing is
            // published and whatever the participant already advertises survives untouched.
            log::warn!(
                target: "tddy_core::session_participant_metadata",
                "session_metadata_json serialize failed: {e}; publishing nothing rather than a null `session` that would erase a sibling publisher's block"
            );
            String::new()
        }
    }
}

/// Shallow-merge two JSON **objects** for a participant's metadata.
///
/// Top-level keys from `update` overwrite or add to `baseline`. Nested values are replaced as a whole (not deep-merged).
/// Non-object `baseline` (or invalid JSON) is treated as an empty object with a warning.
///
/// Moved here from `tddy-livekit`: it is pure JSON over the block this module owns, and its
/// consumers must not depend on the LiveKit crate to reach it.
pub fn merge_participant_metadata_json(
    baseline: &str,
    update: &str,
) -> Result<String, serde_json::Error> {
    log::debug!(
        target: "tddy_changeset::metadata",
        "merge_participant_metadata_json: baseline_len={} update_len={}",
        baseline.len(),
        update.len()
    );

    let mut base_map = if baseline.trim().is_empty() {
        serde_json::Map::new()
    } else {
        match serde_json::from_str::<serde_json::Value>(baseline)? {
            serde_json::Value::Object(m) => m,
            other => {
                log::warn!(
                    target: "tddy_changeset::metadata",
                    "merge_participant_metadata_json: baseline is not a JSON object (got {:?}); starting from {{}}",
                    other
                );
                serde_json::Map::new()
            }
        }
    };

    let update_val: serde_json::Value = if update.trim().is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_str(update)?
    };

    if let serde_json::Value::Object(up_map) = update_val {
        for (k, v) in up_map {
            base_map.insert(k, v);
        }
    }

    let merged = serde_json::Value::Object(base_map);
    log::debug!(
        target: "tddy_changeset::metadata",
        "merge_participant_metadata_json: merged top-level keys={}",
        merged.as_object().map(|m| m.len()).unwrap_or(0)
    );
    serde_json::to_string(&merged)
}

#[cfg(test)]
mod merge_tests {
    use super::merge_participant_metadata_json;

    #[test]
    fn merging_participant_metadata_overwrites_top_level_keys() {
        let merged: serde_json::Value = serde_json::from_str(
            &merge_participant_metadata_json(
                r#"{"session":{"agent":"claude"},"codex_oauth":{"pending":true}}"#,
                r#"{"session":{"agent":"cursor"}}"#,
            )
            .expect("both are JSON objects"),
        )
        .unwrap();

        assert_eq!(
            merged,
            serde_json::json!({"session":{"agent":"cursor"},"codex_oauth":{"pending":true}})
        );
    }
}
