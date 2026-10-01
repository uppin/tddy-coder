//! What one commit changed, in the counts a tool summary carries.
//!
//! Derived from git's own `--name-status` and `--numstat` of the staged change, so a `Shell` call
//! that touched files it never named gets the same accounting as a `Write`.

use serde::{Deserialize, Serialize};

/// Files a change created, updated and removed. A rename is one removed and one created.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCounts {
    pub created: u32,
    pub updated: u32,
    pub removed: u32,
}

/// Lines a change added and removed. A binary file contributes none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineCounts {
    pub added: u64,
    pub removed: u64,
}

/// What one mutating tool call did to its conversation's worktree — the `worktreeChange` object on
/// that call's result and summary.
///
/// `commit` is the short hash of the commit that recorded the change, and is absent when the call
/// changed nothing (no commit is made then). Bounded by construction: no paths, so a turn of fifty
/// calls adds fifty small objects to its outcome, not fifty file lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub files: FileCounts,
    pub lines: LineCounts,
}

/// Count `git diff --cached --name-status -z`-style lines (here newline-separated
/// `STATUS\tpath[\tpath]`) and `git diff --cached --numstat` lines (`added\tremoved\tpath`, `-` for
/// binary).
pub(crate) fn count_change(name_status: &str, numstat: &str) -> (FileCounts, LineCounts) {
    let mut files = FileCounts::default();
    for status in name_status
        .lines()
        .filter_map(|line| line.split('\t').next())
    {
        match status.chars().next() {
            Some('A') | Some('C') => files.created += 1,
            Some('M') | Some('T') => files.updated += 1,
            Some('D') => files.removed += 1,
            Some('R') => {
                files.removed += 1;
                files.created += 1;
            }
            _ => {}
        }
    }
    let mut lines = LineCounts::default();
    for line in numstat.lines() {
        let mut columns = line.split('\t');
        // A binary file reports `-` for both columns and so adds nothing.
        lines.added += columns
            .next()
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0);
        lines.removed += columns
            .next()
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0);
    }
    (files, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_added_file_is_created() {
        let (files, _) = count_change("A\tsrc/new.rs\n", "3\t0\tsrc/new.rs\n");
        assert_eq!(
            files,
            FileCounts {
                created: 1,
                updated: 0,
                removed: 0
            }
        );
    }

    #[test]
    fn a_modified_and_a_type_changed_file_are_updated() {
        let (files, _) = count_change(
            "M\tsrc/lib.rs\nT\tscripts/run\n",
            "2\t1\tsrc/lib.rs\n0\t0\tscripts/run\n",
        );
        assert_eq!(
            files,
            FileCounts {
                created: 0,
                updated: 2,
                removed: 0
            }
        );
    }

    #[test]
    fn a_deleted_file_is_removed() {
        let (files, _) = count_change("D\tsrc/old.rs\n", "0\t12\tsrc/old.rs\n");
        assert_eq!(
            files,
            FileCounts {
                created: 0,
                updated: 0,
                removed: 1
            }
        );
    }

    #[test]
    fn a_rename_is_one_removed_and_one_created() {
        let (files, _) = count_change("R100\tsrc/a.rs\tsrc/b.rs\n", "0\t0\tsrc/{a.rs => b.rs}\n");
        assert_eq!(
            files,
            FileCounts {
                created: 1,
                updated: 0,
                removed: 1
            }
        );
    }

    #[test]
    fn lines_are_summed_across_files() {
        let (_, lines) = count_change(
            "M\tsrc/lib.rs\nA\tsrc/new.rs\n",
            "5\t2\tsrc/lib.rs\n40\t0\tsrc/new.rs\n",
        );
        assert_eq!(
            lines,
            LineCounts {
                added: 45,
                removed: 2
            }
        );
    }

    #[test]
    fn a_binary_file_counts_as_a_file_and_adds_no_lines() {
        let (files, lines) = count_change("A\tlogo.png\n", "-\t-\tlogo.png\n");
        assert_eq!(
            (files, lines),
            (
                FileCounts {
                    created: 1,
                    updated: 0,
                    removed: 0
                },
                LineCounts::default()
            )
        );
    }

    #[test]
    fn an_empty_change_counts_nothing() {
        assert_eq!(
            count_change("", ""),
            (FileCounts::default(), LineCounts::default())
        );
    }

    #[test]
    fn a_change_without_a_commit_serializes_without_the_commit_key() {
        let change = WorktreeChange {
            commit: None,
            files: FileCounts::default(),
            lines: LineCounts::default(),
        };
        assert_eq!(
            serde_json::to_value(&change).unwrap(),
            serde_json::json!({
                "files": { "created": 0, "updated": 0, "removed": 0 },
                "lines": { "added": 0, "removed": 0 }
            })
        );
    }
}
