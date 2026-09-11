use super::SeededCloneGuard;

use super::session_enforces_a_withdrawal;

use tddy_rpc::Status;

use std::path::Path;

use std::path::PathBuf;

/// The clone an attach claimed for a remote agent.
///
/// `commissioned` is what makes a failed attach unwindable without taking a checkout away from an
/// agent that is still using it: two agents on one host share one clone, and only the attach that
/// minted it may delete it.
pub(crate) struct ClaimedAgentClone {
    pub(crate) codebase_session_id: String,
    pub(crate) commissioned: bool,
}

/// What a seed needs to know about the session's codebase.
///
/// Taken as arguments rather than read back from `.session.yaml`, because a co-located start seeds
/// *before* that file exists: it cannot be written until the agent it describes has a pid, and the
/// roster has to be in place before that agent is spawned or its withdrawal is unenforced until the
/// first resume. Where the agent runs is not what decides whether it can be named — the codebase
/// host serves a peer's agent over a synced clone on every placement — so the seed had to stop
/// depending on a file only some placements have written by then.
#[derive(Clone)]
pub struct SeedCodebase {
    pub(crate) session_dir: PathBuf,
    /// The checkout a peer's clone mirrors. `None` for a session with no checkout on this daemon,
    /// which can hold local agents but nothing a clone would have to be built for.
    pub(crate) worktree_root: Option<PathBuf>,
    pub(crate) project_id: String,
    /// Whether a withdrawal in this seed is actually enforced against the main agent
    /// ([`session_enforces_a_withdrawal`]).
    pub(crate) enforces_withdrawal: bool,
}

impl SeedCodebase {
    /// The codebase of a session this daemon is starting right now, as the start itself knows it.
    ///
    /// The checkout is required: a start that has not resolved one has nothing for a peer's clone
    /// to mirror, and no agent of its own to seed either.
    pub fn of_a_starting_session(
        session_dir: PathBuf,
        worktree_root: PathBuf,
        project_id: &str,
        enforces_withdrawal: bool,
    ) -> Self {
        Self {
            session_dir,
            worktree_root: Some(worktree_root),
            project_id: project_id.to_string(),
            enforces_withdrawal,
        }
    }

    /// The codebase of a session already on disk — every path but a co-located start, which has no
    /// `.session.yaml` to read at the point it seeds.
    pub(crate) fn read(session_id: &str, session_dir: &Path) -> Result<Self, Status> {
        let meta = tddy_core::read_session_metadata(session_dir).map_err(|e| {
            Status::not_found(format!(
                "session '{session_id}' has no readable metadata at {}: {e}",
                session_dir.display()
            ))
        })?;
        Ok(Self {
            session_dir: session_dir.to_path_buf(),
            worktree_root: meta.repo_path.as_ref().map(PathBuf::from),
            project_id: meta.project_id.clone(),
            enforces_withdrawal: session_enforces_a_withdrawal(&meta),
        })
    }
}

/// This daemon in its capacity as the claimant of the clones a session's seeded agents read.
///
/// A trait for the same reason [`StackParentHost`] is one: the co-located
/// cursor-cli spawn is a free function, and claiming a clone is the whole of `ConnectionService`'s
/// peer-facing surface — naming that type there would drag it through every caller of a function
/// that otherwise mentions nothing of the kind.
#[async_trait::async_trait]
pub trait SeededAgentClones: Send + Sync {
    /// Claim, on each peer the roster names, the checkout that peer's agent will read, stamping the
    /// claimed id onto the record so the roster the start persists names it.
    ///
    /// The returned guard hands every claim back unless [`SeededCloneGuard::keep`] is called, which
    /// the start does once its roster is on disk.
    async fn claim_for_seed(
        &self,
        session_id: &str,
        codebase: &SeedCodebase,
        session_token: &str,
        records: &mut [tddy_core::SessionAgentRecord],
    ) -> Result<SeededCloneGuard, Status>;
}
