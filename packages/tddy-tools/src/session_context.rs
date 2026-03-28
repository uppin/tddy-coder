//! Merge JSON into workflow session files for the `set-session-context` CLI.

use anyhow::{bail, Context as AnyhowContext, Result};
use std::fs;
use std::path::Path;
use tddy_core::workflow::session::Session;

/// Maximum length for a single context key string (aligned with safe identifier storage).
const MAX_CONTEXT_KEY_BYTES: usize = 256;

fn validate_context_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("session context key must not be empty");
    }
    if key.len() > MAX_CONTEXT_KEY_BYTES {
        bail!(
            "session context key exceeds {} bytes: {}",
            MAX_CONTEXT_KEY_BYTES,
            key.len()
        );
    }
    Ok(())
}

/// Merge `patch` (JSON object) into the session's persisted `context` field and write the file back.
pub fn apply_session_context_merge(
    workflow_storage_dir: &Path,
    session_id: &str,
    patch: &serde_json::Value,
) -> Result<()> {
    log::info!(
        target: "tddy_tools::session_context",
        "apply_session_context_merge: session_id={} dir={}",
        session_id,
        workflow_storage_dir.display()
    );
    let obj = patch
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("expected top-level JSON object"))?;
    for k in obj.keys() {
        validate_context_key(k)?;
    }

    let path = workflow_storage_dir.join(format!("{}.session.json", session_id));
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read session file {}", path.display()))?;
    let session: Session = serde_json::from_str(&raw)
        .with_context(|| format!("parse session JSON {}", path.display()))?;

    log::debug!(
        target: "tddy_tools::session_context",
        "merging {} key(s) into session context",
        obj.len()
    );
    session.context.merge_json_object_sync(obj);

    let out = serde_json::to_string_pretty(&session)?;
    fs::write(&path, out).with_context(|| format!("write session file {}", path.display()))?;
    log::info!(
        target: "tddy_tools::session_context",
        "apply_session_context_merge: wrote {}",
        path.display()
    );
    Ok(())
}
