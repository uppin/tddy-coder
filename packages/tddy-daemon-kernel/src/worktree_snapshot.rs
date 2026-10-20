//! What one poll of a checkout observed, as the room and the agent roster both read it.

/// Everything one poll of a checkout observed.
///
/// `changed_paths` is carried here and published in room metadata but never in an event: an event
/// says *that* the checkout moved, and reading it is what the file-access RPCs in the same room are
/// for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorktreeSnapshot {
    pub head_commit: String,
    pub branch: String,
    pub changed_paths: Vec<String>,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub untracked_files: u32,
    /// The tree object of the whole working tree as it stands, staged into a **temporary** index.
    ///
    /// This is what makes a delta possible: diffing two of these yields an ordinary git patch that
    /// carries deletions, renames, modes and binary content, and — unlike `git diff HEAD` — it sees
    /// a newly written *untracked* file, which is exactly what a `Write` produces.
    ///
    /// The index is temporary and that is not an optimisation: `git add -A` against the agent's own
    /// index would rewrite its staging area mid-session.
    ///
    /// Empty when the tree could not be written; the tick then produces no delta rather than a
    /// wrong one.
    pub wip_tree: String,
}

impl WorktreeSnapshot {
    /// True when the two snapshots disagree about the **tracked** diff — the paths and their line
    /// counts, everything `git diff --numstat HEAD` measures.
    ///
    /// HEAD is deliberately excluded: it is the other half of the comparison and gets its own event.
    ///
    /// So is `untracked_files`, for two reasons. An event carries counts only, and a HEAD-relative
    /// diff has no line counts for a path git does not track — so a newly written untracked file
    /// could only be announced as `files=0 +0 -0`, which tells a receiver nothing it can act on.
    /// And every write is untracked for the moment between the file appearing and `git add`
    /// staging it, so treating that as activity would put an empty event in front of the commit
    /// event that actually describes the work. The fact is not lost: it lives in the room's
    /// metadata, which [`WorktreeSnapshot`] equality still covers, so a joiner reads the untracked
    /// count from state rather than from a notification with nothing in it.
    pub fn tracked_diff_differs(&self, other: &Self) -> bool {
        self.changed_paths != other.changed_paths
            || self.changed_files != other.changed_files
            || self.lines_added != other.lines_added
            || self.lines_removed != other.lines_removed
    }
}
