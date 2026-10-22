//! The session list's `pending_elicitation` flag, as the tool records it in `.session.yaml`.
//!
//! Read on every daemon, with or without a chat channel, so it lives with the session list rather
//! than with Telegram.

use std::path::Path;

/// Reads `pending_elicitation` from `.session.yaml` for the Connection session list.
pub fn pending_elicitation_for_session_dir(session_dir: &Path) -> bool {
    match tddy_core::read_session_metadata(session_dir) {
        Ok(meta) => {
            log::debug!(
                target: "tddy_daemon::elicitation",
                "pending_elicitation_for_session_dir: session_id={} pending_elicitation={}",
                meta.session_id,
                meta.pending_elicitation
            );
            meta.pending_elicitation
        }
        Err(e) => {
            log::debug!(
                target: "tddy_daemon::elicitation",
                "pending_elicitation_for_session_dir: no metadata in {}: {e}",
                session_dir.display()
            );
            false
        }
    }
}
