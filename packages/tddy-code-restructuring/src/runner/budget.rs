//! The file-budget report `check --budget LINES` produces.
//!
//! How many production lines each file a plan names has in the tree as it stands, and which of them are over the
//! budget — a record of where the tree stands rather than a verdict on the plan.

use crate::{Anchor, Plan, RefactorOp, RestructureError, Result};
use std::path::Path;

/// One file a plan names, and how many production lines it has in the tree as it stands.
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

/// The lines of Rust source before its test module — the length the repository's file budget and
/// every code-issue record count.
///
/// The test module opens at the first `#[cfg(test)]` whose next non-blank line starts a `mod`, so a
/// `#[cfg(test)] use ...;` above it does not end the count early. A text with no such module is
/// production throughout.
pub(crate) fn production_lines(text: &str) -> usize {
    let lines: Vec<&str> = text.lines().collect();
    let opens_a_test_module = |index: usize| {
        lines[index].trim_start().starts_with("#[cfg(test)]")
            && lines[index + 1..]
                .iter()
                .map(|line| line.trim_start())
                .find(|line| !line.is_empty())
                .is_some_and(|line| line.starts_with("mod ") || line.starts_with("pub mod "))
    };
    (0..lines.len())
        .find(|&index| opens_a_test_module(index))
        .unwrap_or(lines.len())
}

/// Production lines of a file: Rust is cut at its test module, any other language has none.
fn lines_of_file(path: &str, text: &str) -> usize {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("rs") => production_lines(text),
        _ => text.lines().count(),
    }
}

/// How many production lines each named file has, read from the tree the check is running against.
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
                lines: lines_of_file(path, &text),
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
            "budget: every file the plan names is within {budget} production lines"
        )];
    }

    let mut lines = vec![format!(
        "budget: {} of {} file(s) over {budget} production lines",
        over.len(),
        sizes.len()
    )];
    lines.extend(over.iter().map(|file| {
        format!(
            "budget: {} is {} production lines, {} over",
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
                    id: None,
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
                    group: None,
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
                "budget: 1 of 2 file(s) over 500 production lines",
                "budget: src/over.rs is 612 production lines, 112 over",
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
            ["budget: every file the plan names is within 500 production lines"]
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

    /// A file with no test module is measured whole: nothing in it is test code.
    #[test]
    fn counts_every_line_of_a_file_with_no_test_module() {
        // Given a source file with no `#[cfg(test)]` anywhere
        let text = "fn a() {}\nfn b() {}\nfn c() {}\n";

        // When its production lines are counted
        let lines = production_lines(text);

        // Then all three are production
        assert_eq!(lines, 3);
    }

    /// The count ends where the test module opens, so a long test module cannot push a short file
    /// over the budget.
    #[test]
    fn stops_counting_where_the_test_module_opens() {
        // Given two production lines followed by a test module of three lines
        let text = "fn a() {}\nfn b() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n";

        // When
        let lines = production_lines(text);

        // Then only the two lines before the attribute are production
        assert_eq!(lines, 2);
    }

    /// `#[cfg(test)] use ...;` is a test-only import, not the start of the test module.
    #[test]
    fn does_not_end_early_at_a_test_only_import_above_the_module() {
        // Given a test-only import on line 2 and the test module on line 5
        let text =
            "fn a() {}\n#[cfg(test)]\nuse std::fmt;\nfn b() {}\n#[cfg(test)]\nmod tests {}\n";

        // When
        let lines = production_lines(text);

        // Then the import and the item after it still count
        assert_eq!(lines, 4);
    }

    /// Blank lines between the attribute and `mod` do not make it something else.
    #[test]
    fn finds_a_test_module_after_blank_lines_following_its_attribute() {
        // Given a blank line between the attribute and a `pub mod`
        let text = "fn a() {}\n#[cfg(test)]\n\npub mod tests {}\n";

        // When
        let lines = production_lines(text);

        // Then
        assert_eq!(lines, 1);
    }

    /// A file that is nothing but its test module has no production lines.
    #[test]
    fn counts_no_production_lines_when_the_test_module_is_the_whole_file() {
        // Given a file that opens its test module on the first line
        let text = "#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n";

        // When
        let lines = production_lines(text);

        // Then
        assert_eq!(lines, 0);
    }

    /// Only Rust has `#[cfg(test)]`; in any other file the same text is just text.
    #[test]
    fn measures_every_line_of_a_file_that_is_not_rust() {
        // Given a TypeScript file whose text happens to contain the Rust attribute
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(
            dir.path().join("widget.ts"),
            "a\n#[cfg(test)]\nmod tests {}\n",
        )
        .expect("the fixture is written");

        // When it is measured
        let sizes = measured(dir.path(), &["widget.ts".to_string()]).expect("measured");

        // Then every line counts
        assert_eq!(sizes, [a_file_of("widget.ts", 3)]);
    }

    /// The measurement a plan author acts on: a file over the budget only by its tests is within it.
    #[test]
    fn measures_production_lines_only_so_a_long_test_module_does_not_count() {
        // Given a Rust file of 3 production lines and a 4-line test module on disk
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(
            dir.path().join("big.rs"),
            "fn a() {}\nfn b() {}\nfn c() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n",
        )
        .expect("the fixture is written");

        // When it is measured
        let sizes = measured(dir.path(), &["big.rs".to_string()]).expect("measured");

        // Then
        assert_eq!(sizes, [a_file_of("big.rs", 3)]);
    }
}
