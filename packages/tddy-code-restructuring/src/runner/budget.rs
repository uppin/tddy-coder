//! The file-budget report `check --budget LINES` produces.
//!
//! How long each file a plan names is in the tree as it stands, and which of them are over the
//! budget — a record of where the tree stands rather than a verdict on the plan.

use crate::{Anchor, Plan, RefactorOp, RestructureError, Result};
use std::path::Path;

/// One file a plan names, and how long it is in the tree as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileSize {
    path: String,
    lines: usize,
}

/// Every file a plan operates on, once each, in the order it first names one.
///
/// The anchors are the plan's own statement of what it touches, so this is the set the budget is
/// reported over — not the whole tree, which would bury this plan's outcome in the repository's.
/// Every anchor of an operation, so a cluster's co-moving members are measured too.
pub(super) fn files_named_by(plan: &Plan) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for file in plan
        .ops
        .iter()
        .flat_map(RefactorOp::anchors)
        .map(Anchor::file)
    {
        if !named.iter().any(|seen| seen == file) {
            named.push(file.to_string());
        }
    }
    named
}

/// How long each named file is, read from the tree the check is running against.
pub(super) fn measured(root: &Path, paths: &[String]) -> Result<Vec<FileSize>> {
    paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(root.join(path)).map_err(|error| {
                RestructureError::MalformedPlan(format!(
                    "`{path}` cannot be measured against the budget: {error}"
                ))
            })?;
            Ok(FileSize {
                path: path.clone(),
                lines: text.lines().count(),
            })
        })
        .collect()
}

/// The files longer than `budget` lines, longest first — the order a plan author splits them in.
///
/// A file *at* the budget is within it: the budget is a length a file may reach, and the agreed
/// policy is that seams are cut where they are cohesive rather than to hit a number.
fn over_budget(sizes: &[FileSize], budget: usize) -> Vec<FileSize> {
    let mut over: Vec<FileSize> = sizes
        .iter()
        .filter(|file| file.lines > budget)
        .cloned()
        .collect();
    over.sort_by(|left, right| {
        right
            .lines
            .cmp(&left.lines)
            .then_with(|| left.path.cmp(&right.path))
    });
    over
}

/// The file-budget report, as `check --budget LINES` prints it.
pub(super) fn budget_report(sizes: &[FileSize], budget: usize) -> Vec<String> {
    let over = over_budget(sizes, budget);
    if over.is_empty() {
        return vec![format!(
            "budget: every file the plan names is within {budget} lines"
        )];
    }

    let mut lines = vec![format!(
        "budget: {} of {} file(s) over {budget} lines",
        over.len(),
        sizes.len()
    )];
    lines.extend(over.iter().map(|file| {
        format!(
            "budget: {} is {} lines, {} over",
            file.path,
            file.lines,
            file.lines - budget
        )
    }));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Anchor, RefactorKind, RefactorOp};
    use std::collections::BTreeMap;

    fn a_file_of(path: &str, lines: usize) -> FileSize {
        FileSize {
            path: path.to_string(),
            lines,
        }
    }

    fn paths_of(sizes: &[FileSize]) -> Vec<&str> {
        sizes.iter().map(|size| size.path.as_str()).collect()
    }

    fn a_plan_operating_on(files: &[&str]) -> Plan {
        Plan {
            version: 1,
            snapshot: BTreeMap::new(),
            files: BTreeMap::new(),
            ops: files
                .iter()
                .map(|file| RefactorOp {
                    op: RefactorKind::ExtractModuleToFile,
                    anchor: Anchor::Symbol {
                        file: file.to_string(),
                        path: "an_item".to_string(),
                    },
                    name: None,
                    to: None,
                    variant: None,
                    with_private_deps: false,
                    reexport: None,
                    to_file: false,
                    also: Vec::new(),
                })
                .collect(),
        }
    }

    /// The report a plan author reads to decide what still has to be split: the files over the
    /// budget and only those, longest first, because the longest is the one worth splitting next.
    #[test]
    fn lists_exactly_the_files_over_the_budget_longest_first() {
        // Given three files a plan names, two of them longer than 500 lines
        let named = vec![
            a_file_of("packages/tddy-daemon/src/host_registry.rs", 612),
            a_file_of("packages/tddy-daemon/src/config.rs", 85),
            a_file_of("packages/tddy-daemon/src/connection_service.rs", 2416),
        ];

        // When the budget report is taken at 500 lines
        let over = over_budget(&named, 500);

        // Then
        assert_eq!(
            paths_of(&over),
            [
                "packages/tddy-daemon/src/connection_service.rs",
                "packages/tddy-daemon/src/host_registry.rs",
            ]
        );
    }

    /// The budget is a length a file may reach: 500 lines is within a 500-line budget. Reporting it
    /// as over would ask for a split the agreed policy does not.
    #[test]
    fn keeps_a_file_exactly_at_the_budget_within_it() {
        // Given one file exactly at the budget and one a single line over it
        let named = vec![a_file_of("src/at.rs", 500), a_file_of("src/over.rs", 501)];

        // When
        let over = over_budget(&named, 500);

        // Then
        assert_eq!(paths_of(&over), ["src/over.rs"]);
    }

    /// How far over matters as much as being over: it is the difference between a file to watch and
    /// one to split.
    #[test]
    fn reports_how_far_over_the_budget_each_file_is() {
        // Given a plan naming two files, one of them 112 lines over budget
        let named = vec![a_file_of("src/at.rs", 500), a_file_of("src/over.rs", 612)];

        // When
        let report = budget_report(&named, 500);

        // Then
        assert_eq!(
            report,
            [
                "budget: 1 of 2 file(s) over 500 lines",
                "budget: src/over.rs is 612 lines, 112 over",
            ]
        );
    }

    /// A clean budget is a result, not silence — the report is how the outcome gets recorded.
    #[test]
    fn reports_that_every_file_a_plan_names_is_within_the_budget() {
        // Given two files a plan names, both within a 500-line budget
        let named = vec![a_file_of("src/at.rs", 500), a_file_of("src/small.rs", 12)];

        // When
        let report = budget_report(&named, 500);

        // Then
        assert_eq!(
            report,
            ["budget: every file the plan names is within 500 lines"]
        );
    }

    /// A split plan names the file it is carving up once per operation — 29 times for a
    /// 29-operation plan — and measuring it 29 times would report it 29 times.
    #[test]
    fn names_each_file_a_plan_operates_on_once() {
        // Given a plan with two operations on one file and one on another
        let plan = a_plan_operating_on(&["src/big.rs", "src/big.rs", "src/other.rs"]);

        // When
        let named = files_named_by(&plan);

        // Then
        assert_eq!(named, ["src/big.rs", "src/other.rs"]);
    }
}
