//! The single rule for "which session owns a branch".
//!
//! Three surfaces need the same answer — `QueryBranch`, the `StartSession` branch-conflict guard and
//! the Telegram spawn flow — so the scan lives here rather than inline at any one of them, and
//! "prefer active, then most-recently-updated" cannot drift between them.
//!
//! PRD: docs/ft/daemon/session-branch-conflict.md

use std::path::Path;

use tddy_core::output::SESSIONS_SUBDIR;

use crate::session_reader::{self, SessionEntry};

/// The session under `sessions_base` whose `Changeset.branch` equals `branch`, or `None` when no
/// session claims it — a branch that merely exists in git has no owner.
///
/// When several sessions claim the same branch, an **active** one wins; between equally active
/// candidates the most recently updated one does. A session whose changeset cannot be read is
/// skipped: it names no branch, so it can claim none.
///
/// `branch` is matched verbatim — callers trim it. Reads changesets and `.session.yaml` from disk,
/// so async callers wrap it in `spawn_blocking_with_timeout`.
pub fn find_session_owning_branch(
    sessions_base: &Path,
    branch: &str,
) -> anyhow::Result<Option<SessionEntry>> {
    let sessions = session_reader::list_sessions_in_dir(sessions_base)?;
    let mut best: Option<SessionEntry> = None;
    for session in sessions {
        let session_dir = sessions_base
            .join(SESSIONS_SUBDIR)
            .join(&session.session_id);
        let Ok(changeset) = tddy_core::read_changeset(&session_dir) else {
            continue;
        };
        if changeset.branch.as_deref() != Some(branch) {
            continue;
        }
        best = Some(match best {
            None => session,
            Some(current) => {
                if (session.is_active, session.updated_at.as_str())
                    > (current.is_active, current.updated_at.as_str())
                {
                    session
                } else {
                    current
                }
            }
        });
    }
    Ok(best)
}
