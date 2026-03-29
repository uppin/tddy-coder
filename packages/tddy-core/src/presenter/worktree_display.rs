//! Worktree path → status-bar display string (PRD: basename / repo-relative truncation).

use std::path::Path;

const MAX_WORKTREE_DISPLAY_CHARS: usize = 48;

/// Formats a worktree directory path for the TUI status row.
///
/// Uses the final path component (directory name) when available; otherwise falls back to the
/// last non-empty component. Very long labels are truncated on a UTF-8 boundary so the status row
/// stays usable on 80-column terminals.
pub(crate) fn format_worktree_for_status_bar(path: &Path) -> String {
    let path_len = path.as_os_str().len();
    log::debug!(
        "format_worktree_for_status_bar: path_len={} components={}",
        path_len,
        path.components().count()
    );

    let raw = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.components().rev().find_map(|c| {
                c.as_os_str()
                    .to_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
        })
        .unwrap_or_default();

    if raw.is_empty() {
        log::info!(
            "format_worktree_for_status_bar: empty display for path {:?}",
            path
        );
        return String::new();
    }

    if raw.chars().count() <= MAX_WORKTREE_DISPLAY_CHARS {
        log::info!("format_worktree_for_status_bar: using {:?}", raw);
        return raw;
    }

    let truncated: String = raw.chars().take(MAX_WORKTREE_DISPLAY_CHARS).collect();
    log::info!(
        "format_worktree_for_status_bar: truncated long segment (chars={}) → {:?}…",
        raw.chars().count(),
        truncated
    );
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Lower-level (PRD): non-empty display derived from the worktree path for the status segment.
    #[test]
    fn format_worktree_for_status_bar_includes_path_marker_segment() {
        let path = Path::new("/tmp/wt-acceptance-marker/my-branch");
        let s = format_worktree_for_status_bar(path);
        assert!(
            s.contains("wt-acceptance-marker") || s.contains("my-branch"),
            "expected basename or marker segment in display; got {s:?}"
        );
    }
}
