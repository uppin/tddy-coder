//! The one rule that names a session for a human.
//!
//! Every surface that shows a session — the web drawer, a Telegram alert, an indicator tooltip —
//! must call it the same thing, or an operator reading one cannot find it in the other. The rule
//! is deliberately a mirror of the web's `sessionDrawerLabel`
//! (`packages/tddy-web/src/utils/sessionDrawerLabel.ts`): **the basename of `repo_path`, else
//! `workflow_goal`, else the first eight characters of `session_id`**. Change one side and the
//! other must change with it — the parity is pinned case for case by
//! `packages/tddy-core/tests/session_display_label_acceptance.rs` against
//! `packages/tddy-web/src/utils/sessionDrawerLabel.test.ts`.

/// The display placeholder the daemon's session-list enrichment puts in a field it has no value
/// for (`SessionListStatusDisplay::all_placeholders`), and which `ListSessions` hands to the
/// browser as-is. A session reports it as its `workflow_goal` when its `.session.yaml` is
/// unreadable, or when it is a workflow session whose `changeset.yaml` is unreadable or does not
/// list it — so the label rule counts it as *absent* rather than naming the session `—`.
///
/// It is not what a claude-cli or cursor-cli session reports: those take an earlier branch in the
/// enrichment and yield an empty `workflow_goal`, which the rule already treats as absent.
pub const DISPLAY_PLACEHOLDER: &str = "\u{2014}";

/// The rule's first step on its own: the basename of `repo_path`, or `None` when there is none to
/// take (empty, whitespace, or the filesystem root).
///
/// Exposed so a caller that would otherwise pay to *fetch* `workflow_goal` can find out whether it
/// is going to be consulted at all. The daemon resolves a label on every reported hook, and reading
/// the goal costs it a second file parse — one that a session with a worktree never needs.
pub fn label_from_repo_path(repo_path: &str) -> Option<String> {
    let trimmed = repo_path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }
    trimmed
        .split('/')
        .rfind(|segment| !segment.is_empty())
        .map(str::to_string)
}

/// The human-readable label for a session, from the three values `ListSessions` reports.
///
/// See the module docs for the rule and why it is shared. Never returns a padded or truncated-mid-
/// character id: the last resort takes the first eight *characters* of `session_id`, which for a
/// shorter id is the whole of it.
pub fn session_display_label(repo_path: &str, workflow_goal: &str, session_id: &str) -> String {
    if let Some(basename) = label_from_repo_path(repo_path) {
        return basename;
    }

    let trimmed_goal = workflow_goal.trim();
    if !trimmed_goal.is_empty() && trimmed_goal != DISPLAY_PLACEHOLDER {
        return trimmed_goal.to_string();
    }

    session_id.chars().take(8).collect()
}
