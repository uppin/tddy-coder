//! CPU architecture constraint from manifest vs host.

use log::{debug, info};

use super::error::SessionActionsError;

/// Verify `manifest.architecture` against the current host (or the `native` sentinel).
pub fn ensure_action_architecture(
    manifest_triple_or_label: &str,
) -> Result<(), SessionActionsError> {
    let requested = manifest_triple_or_label.trim();
    if requested.is_empty() {
        let host = std::env::consts::ARCH.to_string();
        debug!(
            target: "tddy_core::session_actions::arch",
            "empty architecture field; host={host}"
        );
        return Err(SessionActionsError::ArchitectureMismatch {
            requested: String::new(),
            host,
        });
    }

    if requested.eq_ignore_ascii_case("native") {
        info!(
            target: "tddy_core::session_actions::arch",
            "architecture guard: `native` accepted for host {}",
            std::env::consts::ARCH
        );
        return Ok(());
    }

    let host = std::env::consts::ARCH;
    if requested == host {
        debug!(
            target: "tddy_core::session_actions::arch",
            "explicit architecture `{requested}` matches host"
        );
        return Ok(());
    }

    // Accept common rustc-style triple if the leading segment equals the host arch.
    let first = requested.split('-').next().unwrap_or("");
    if first == host {
        debug!(
            target: "tddy_core::session_actions::arch",
            "`{}` first segment matches host `{host}`",
            requested
        );
        return Ok(());
    }

    Err(SessionActionsError::ArchitectureMismatch {
        requested: requested.to_string(),
        host: host.to_string(),
    })
}
