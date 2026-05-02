//! Enumerate manifests under `<session>/actions`.

use std::path::Path;

use log::{debug, info};
use serde::Serialize;

use super::error::SessionActionsError;
use super::manifest::parse_action_manifest_file;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActionSummary {
    pub id: String,
    pub summary: String,
    pub has_input_schema: bool,
    pub has_output_schema: bool,
}

/// Discovery + metadata for `list-actions` JSON (`actions` sorted ascending by `id`).
pub fn list_action_summaries(
    session_dir: &Path,
) -> Result<Vec<ActionSummary>, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::list",
        "list_action_summaries: session_dir={}",
        session_dir.display()
    );
    let actions_dir = session_dir.join("actions");
    if !actions_dir.is_dir() {
        return Err(SessionActionsError::MissingActionsDir(actions_dir));
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&actions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml")
            && path.extension().and_then(|e| e.to_str()) != Some("yml")
        {
            continue;
        }
        let manifest = parse_action_manifest_file(&path)?;
        out.push(ActionSummary {
            id: manifest.id.clone(),
            summary: manifest.summary.clone(),
            has_input_schema: manifest.input_schema.is_some(),
            has_output_schema: manifest.output_schema.is_some(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    info!(
        target: "tddy_core::session_actions::list",
        "discovered {} action manifest(s) under {}",
        out.len(),
        actions_dir.display()
    );
    Ok(out)
}
