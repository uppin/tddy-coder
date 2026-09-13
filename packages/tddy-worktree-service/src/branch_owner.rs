//! The single rule for "which session owns a branch".
//!
//! Three surfaces need the same answer — `QueryBranch`, the `StartSession` branch-conflict guard and
//! the Telegram spawn flow — so the scan lives here rather than inline at any one of them, and
//! "prefer active, then most-recently-updated" cannot drift between them.
//!
//! # Why the session listing arrives through a port
//!
//! A branch belongs to a **worktree**, which is this crate's; the thing that claims it is a
//! **session**, which is family C and stays in `tddy-daemon` deliberately. So this module cannot
//! read `.session.yaml` itself without depending on the crate it left. [`SessionListing`] is the
//! seam: `tddy-daemon` implements it over its own `session_reader`, and what crosses the boundary
//! is [`SessionClaim`] — the three fields the rule actually judges on — rather than everything a
//! session is.
//!
//! PRD: docs/ft/daemon/session-branch-conflict.md

use std::path::Path;

use tddy_core::output::SESSIONS_SUBDIR;

/// One session's claim on a branch, reduced to what the ownership rule reads.
///
/// Deliberately not the daemon's whole session record: the rule needs an identity, an activity flag
/// and a recency stamp, and a port that carried more would make every later field of a session part
/// of this crate's contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionClaim {
    pub session_id: String,
    /// Whether the session's process is still alive — an active claimant beats a dormant one.
    pub is_active: bool,
    /// The session's own status string, passed through to whoever is told about the conflict.
    pub status: String,
    /// Compared as a string, because that is how the underlying `.session.yaml` stamps it and two
    /// parsings of one timestamp are two chances to order them differently.
    pub updated_at: String,
}

/// What sessions exist under a sessions base directory.
///
/// The one thing this crate cannot answer for itself. Implemented in `tddy-daemon` over
/// `session_reader`, which owns what a session is.
pub trait SessionListing: Send + Sync {
    /// Every session recorded under `sessions_base`, in any order.
    ///
    /// A session whose record cannot be read is omitted rather than reported as an error: it claims
    /// no branch, so it cannot own one, and failing the whole scan over one unreadable directory
    /// would turn a stale file into a refusal to start any session on any branch.
    fn sessions_under(&self, sessions_base: &Path) -> anyhow::Result<Vec<SessionClaim>>;
}

/// The session under `sessions_base` whose `Changeset.branch` equals `branch`, or `None` when no
/// session claims it — a branch that merely exists in git has no owner.
///
/// When several sessions claim the same branch, an **active** one wins; between equally active
/// candidates the most recently updated one does. A session whose changeset cannot be read is
/// skipped: it names no branch, so it can claim none.
///
/// `branch` is matched verbatim — callers trim it. Reads changesets from disk, so async callers
/// wrap it in `spawn_blocking_with_timeout`.
pub fn find_session_owning_branch(
    listing: &dyn SessionListing,
    sessions_base: &Path,
    branch: &str,
) -> anyhow::Result<Option<SessionClaim>> {
    let sessions = listing.sessions_under(sessions_base)?;
    let mut best: Option<SessionClaim> = None;
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
