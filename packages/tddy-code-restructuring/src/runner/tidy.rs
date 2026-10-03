//! What an `apply` does to the files it wrote once the tree compiles: drop the imports its moves
//! orphaned, and format what the engine wrote.
//!
//! **Why the compiler decides.** An operation leaves `use` lines that nothing reads any more, and
//! the lint gate (`cargo clippy -- -D warnings`) fails on every one. The engine never prunes an
//! import on its own reading of the file, because a trait import can be needed invisibly — a
//! `use std::fmt::Write;` has no name in the source that uses it. rustc's `unused_imports` lint is
//! trait-aware and carries machine-applicable removal suggestions, so those are the evidence.
//!
//! **Bounded.** Only files the run touched are edited, only a suggestion rustc marks
//! `MachineApplicable` is applied, and the loop stops after [`MAX_ROUNDS`] rounds (removing one
//! import can orphan another). Whatever the compiler still warns about is reported, never fixed.
//!
//! **Imports only the tests use.** `unused_imports` fires per compilation unit, so an import that
//! only a `#[cfg(test)] mod tests { use super::*; … }` reads is reported by the library unit, and
//! removing it breaks the test unit. When a round's removals break the re-check, the files of that
//! round are restored from the bytes held in memory and the round is redone with every import the
//! errors *name* (a backtick-quoted identifier equal to the name the `use` bound) gated with
//! `#[cfg(test)]` instead of removed — see [`gating`]. A nested group is left in place and
//! reported. A round that still does not compile is undone the same way and fails the run, saying
//! so: the tidy never leaves a broken tree behind, and never touches the plan's own edits.

mod diagnostics;
mod format;
mod gating;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::compile_gate::{compiler_errors, described_check, run_check};
use crate::backends::rust::ProgressSink;
use crate::{RestructureError, Result};
use diagnostics::{parse, Diagnostic, Fix, Span};
use format::format_touched;
use gating::{named_by_errors, place, quoted_in_errors, Placement};

/// How many times unused imports are removed and the tree re-checked.
const MAX_ROUNDS: usize = 3;

/// What to tidy: the packages to check and the files the run wrote.
pub(super) struct Tidying<'a> {
    pub root: &'a Path,
    pub packages: &'a BTreeSet<String>,
    pub touched: &'a BTreeSet<String>,
    pub progress: &'a ProgressSink,
    pub cancel: &'a CancellationToken,
}

/// Whether the tidied tree compiles.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Tidied {
    Compiles,
    Broken { checked: String, errors: String },
}

/// Tidy the files a complete run wrote, over the packages it touched.
///
/// Unused imports go first, then rustfmt, then the final check judges the formatted tree. Nothing
/// to check — no packages — is nothing to tidy.
pub(super) fn tidy(tidying: &Tidying<'_>) -> Result<Tidied> {
    if tidying.packages.is_empty() {
        return Ok(Tidied::Compiles);
    }
    let mut report = Report::default();
    let mut checked = tidy_imports(tidying, &mut report)?;
    report.say(tidying);
    if matches!(checked, Checked::Compiles(_)) && format_touched_files(tidying)? {
        checked = check(tidying)?;
    }
    match checked {
        Checked::Broken(failure) => Ok(Tidied::Broken {
            checked: failure.checked,
            errors: failure.errors,
        }),
        Checked::Compiles(diagnostics) => {
            report_remaining_warnings(tidying, &diagnostics);
            Ok(Tidied::Compiles)
        }
    }
}

/// A check's verdict: the warnings of a tree that compiles, or why it does not.
enum Checked {
    Compiles(Vec<Diagnostic>),
    Broken(Failure),
}

/// Why a check failed: the check as a reader runs it, its errors, and every identifier those
/// errors quote.
struct Failure {
    checked: String,
    errors: String,
    quoted: BTreeSet<String>,
}

/// Check the tree, telling the caller the run stopped when its token fires: the files may already
/// be tidied by then, and the cancellation alone would not say so.
fn check(tidying: &Tidying<'_>) -> Result<Checked> {
    let output = match run_check(tidying.root, tidying.packages, "json", tidying.cancel) {
        Err(RestructureError::CallerStopped) => {
            (tidying.progress)("tidy cancelled: the tidied files are on disk and were not checked");
            return Err(RestructureError::CallerStopped);
        }
        other => other?,
    };
    let diagnostics = parse(&output.stdout);
    if output.succeeded {
        return Ok(Checked::Compiles(diagnostics));
    }
    Ok(Checked::Broken(Failure {
        checked: described_check(tidying.packages),
        errors: errors_of(&diagnostics, &output.stderr),
        quoted: quoted_in_errors(&diagnostics),
    }))
}

/// The errors of a failed check: `file:line: error[code]: message` per compiler error, or what
/// cargo said when it named none.
fn errors_of(diagnostics: &[Diagnostic], stderr: &str) -> String {
    let lines: Vec<String> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.level == "error")
        .map(|diagnostic| {
            let (file, line) = diagnostic.location.clone().unwrap_or_default();
            let code = diagnostic
                .code
                .as_ref()
                .map(|code| format!("[{code}]"))
                .unwrap_or_default();
            format!("{file}:{line}: error{code}: {}", diagnostic.message)
        })
        .collect();
    if lines.is_empty() {
        compiler_errors(stderr)
    } else {
        lines.join("\n")
    }
}

/// What the tidy says when it is done: imports removed, imports gated, imports it had to leave.
#[derive(Default)]
struct Report {
    removed: BTreeMap<String, usize>,
    gated: Vec<(String, String)>,
    warnings: BTreeSet<String>,
}

impl Report {
    /// A round whose result compiled: what it did is now fact.
    fn accept(&mut self, round: Round) {
        let removed = round
            .unused
            .primaries
            .iter()
            .filter(|span| round.removes(span));
        for span in removed {
            *self.removed.entry(span.file.clone()).or_default() += 1;
        }
        if let Some(placement) = round.placement {
            self.gated.extend(placement.gated);
            self.warnings.extend(placement.declined);
        }
    }

    fn say(&self, tidying: &Tidying<'_>) {
        for (file, count) in self.removed.iter().filter(|(_, count)| **count > 0) {
            (tidying.progress)(&format!(
                "tidied: removed {count} unused import(s) from {file}"
            ));
        }
        for (file, path) in &self.gated {
            (tidying.progress)(&format!("gated for tests: {file}: use {path}"));
        }
        for warning in &self.warnings {
            (tidying.progress)(warning);
        }
    }
}

impl Round {
    /// Whether an applied edit took the import at `span` out: it covers it and does not write its
    /// text back, as a gated import's replacement does.
    fn removes(&self, span: &Span) -> bool {
        let text = gating::text_of(&self.before, span);
        self.applied
            .iter()
            .filter(|edit| edit.covers(span))
            .any(|edit| !gating::mentions(&edit.replacement, text))
    }
}

/// One round of removals: the files as they were before it, what it removed, and — once a first
/// attempt broke the tree — how it was redone.
struct Round {
    before: BTreeMap<String, Vec<u8>>,
    unused: UnusedImports,
    /// The edits that were applied, in the coordinates of `before`.
    applied: Vec<Fix>,
    placement: Option<Placement>,
}

/// Remove unused imports until the compiler reports none or [`MAX_ROUNDS`] removals have been
/// made, counting what was removed per file. Returns the last check, which judged the tree as it
/// now stands.
fn tidy_imports(tidying: &Tidying<'_>, report: &mut Report) -> Result<Checked> {
    let mut rounds = 0;
    let mut pending: Option<Round> = None;
    loop {
        let diagnostics = match check(tidying)? {
            Checked::Compiles(diagnostics) => {
                pending
                    .take()
                    .into_iter()
                    .for_each(|round| report.accept(round));
                diagnostics
            }
            Checked::Broken(failure) => match pending.take() {
                None => return Ok(Checked::Broken(failure)),
                Some(round) => match repair(tidying, round, failure, report)? {
                    Repair::Retry(round) => {
                        pending = Some(round);
                        continue;
                    }
                    Repair::Nothing => {
                        rounds = MAX_ROUNDS;
                        continue;
                    }
                    Repair::Failed(failure) => return Ok(Checked::Broken(failure)),
                },
            },
        };
        let unused = unused_imports(&diagnostics, tidying.touched);
        if unused.fixes.is_empty() || rounds == MAX_ROUNDS {
            return Ok(Checked::Compiles(diagnostics));
        }
        pending = Some(begin(tidying.root, unused)?);
        rounds += 1;
    }
}

/// Apply a round's removals, keeping the bytes of every file it edits.
fn begin(root: &Path, unused: UnusedImports) -> Result<Round> {
    let mut before = BTreeMap::new();
    for file in unused.fixes.keys() {
        before.insert(file.clone(), std::fs::read(root.join(file))?);
    }
    let applied = apply_fixes(root, &unused.fixes)?;
    Ok(Round {
        before,
        unused,
        applied,
        placement: None,
    })
}

/// What became of a round whose removals broke the tree.
enum Repair {
    /// Redone with imports gated; check it.
    Retry(Round),
    /// Every import it would have removed had to stay: the tree is as it was before the round.
    Nothing,
    /// Undone, and the tree still does not compile.
    Failed(Failure),
}

/// Undo a round that broke the tree and redo it with the imports the errors name gated for tests.
/// A round that was already redone is undone for good, loudly.
fn repair(
    tidying: &Tidying<'_>,
    round: Round,
    failure: Failure,
    report: &mut Report,
) -> Result<Repair> {
    restore(tidying.root, &round.before)?;
    let matched = named_by_errors(&round.unused.primaries, &round.before, &failure.quoted);
    if round.placement.is_some() || matched.is_empty() {
        return Ok(Repair::Failed(undone(&round, &matched, failure)));
    }
    let placement = place(
        &round.unused.fixes,
        &round.unused.primaries,
        &round.before,
        &matched,
    );
    if placement.fixes.values().all(BTreeSet::is_empty) {
        report.warnings.extend(placement.declined);
        return Ok(Repair::Nothing);
    }
    let applied = apply_fixes(tidying.root, &placement.fixes)?;
    Ok(Repair::Retry(Round {
        before: round.before,
        unused: round.unused,
        applied,
        placement: Some(placement),
    }))
}

/// The failure of a round that could not be placed, saying what was undone and which imports it
/// could not place.
fn undone(round: &Round, matched: &[&Span], failure: Failure) -> Failure {
    let culprits: Vec<&Span> = if matched.is_empty() {
        round.unused.primaries.iter().collect()
    } else {
        matched.to_vec()
    };
    let names: Vec<&str> = culprits
        .iter()
        .map(|span| gating::text_of(&round.before, span))
        .collect();
    Failure {
        errors: format!(
            "the tidy was undone: this round's import edits are restored, because neither removing \
             nor gating for tests ({}) leaves a tree that compiles\n{}",
            names.join(", "),
            failure.errors
        ),
        ..failure
    }
}

/// Put back the bytes a round started from.
fn restore(root: &Path, before: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    for (file, bytes) in before {
        std::fs::write(root.join(file), bytes)?;
    }
    Ok(())
}

/// What the compiler says to remove: edits per file, and the imports it reports.
struct UnusedImports {
    fixes: BTreeMap<String, BTreeSet<Fix>>,
    primaries: BTreeSet<Span>,
}

/// The `unused_imports` removals in the touched files. Each target (library, tests) repeats a
/// diagnostic for a file they share, so edits are kept as sets.
fn unused_imports(diagnostics: &[Diagnostic], touched: &BTreeSet<String>) -> UnusedImports {
    let mut fixes: BTreeMap<String, BTreeSet<Fix>> = BTreeMap::new();
    let mut primaries: BTreeSet<Span> = BTreeSet::new();
    let unused = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("unused_imports"));
    for diagnostic in unused {
        for fix in diagnostic
            .fixes
            .iter()
            .filter(|fix| touched.contains(&fix.file))
        {
            fixes
                .entry(fix.file.clone())
                .or_default()
                .insert(fix.clone());
        }
        primaries.extend(diagnostic.primaries.iter().cloned());
    }
    primaries.retain(|span| fixes.contains_key(&span.file));
    UnusedImports { fixes, primaries }
}

/// Apply every file's edits from the highest offset down, so none moves another's coordinates. An
/// edit overlapping one already applied is left for the next round to find again. Returns the
/// spans of the edits that were applied.
fn apply_fixes(root: &Path, fixes: &BTreeMap<String, BTreeSet<Fix>>) -> Result<Vec<Fix>> {
    let mut applied = Vec::new();
    for (file, edits) in fixes {
        let path = root.join(file);
        let mut bytes = std::fs::read(&path)?;
        let mut floor = bytes.len();
        for edit in edits.iter().rev() {
            if edit.end > floor || edit.start > edit.end {
                continue;
            }
            bytes.splice(edit.start..edit.end, edit.replacement.bytes());
            floor = edit.start;
            applied.push(edit.clone());
        }
        std::fs::write(&path, bytes)?;
    }
    Ok(applied)
}

/// Format the touched files; whether any changed, so the caller knows to check again.
fn format_touched_files(tidying: &Tidying<'_>) -> Result<bool> {
    let changed = format_touched(tidying.root, tidying.touched, tidying.progress)?;
    Ok(changed > 0)
}

/// Every warning still reported in a touched file: surfaced, never fixed.
fn report_remaining_warnings(tidying: &Tidying<'_>, diagnostics: &[Diagnostic]) {
    let remaining: BTreeSet<String> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.level == "warning")
        .filter_map(|diagnostic| {
            let (file, line) = diagnostic.location.as_ref()?;
            tidying
                .touched
                .contains(file)
                .then(|| format!("warning remains: {file}:{line}: {}", diagnostic.message))
        })
        .collect();
    for line in remaining {
        (tidying.progress)(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A throw-away dependency-free crate named `demo`, so a check is fast.
    struct ACrate {
        directory: tempfile::TempDir,
    }

    fn a_crate_with(files: &[(&str, &str)]) -> ACrate {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let manifest = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
        std::fs::create_dir_all(directory.path().join("src")).expect("src is created");
        std::fs::write(directory.path().join("Cargo.toml"), manifest).expect("manifest");
        for (file, text) in files {
            std::fs::write(directory.path().join(file), text).expect("a file is written");
        }
        ACrate { directory }
    }

    impl ACrate {
        fn read(&self, file: &str) -> String {
            std::fs::read_to_string(self.directory.path().join(file)).expect("a file is read")
        }

        /// Whether the crate compiles with its test targets, as the tidy's own check builds it.
        fn compiles_with_its_tests(&self) -> std::result::Result<(), String> {
            let output = std::process::Command::new("cargo")
                .args(["check", "--all-targets", "--message-format", "short"])
                .current_dir(self.directory.path())
                .output()
                .expect("cargo runs");
            if output.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).into_owned())
            }
        }

        /// Whether a check of every target still warns about an unused import.
        fn warns_of_an_unused_import(&self) -> bool {
            let output = std::process::Command::new("cargo")
                .args(["check", "--all-targets", "--message-format", "short"])
                .current_dir(self.directory.path())
                .output()
                .expect("cargo runs");
            String::from_utf8_lossy(&output.stderr).contains("unused import")
        }

        /// Tidy `touched`, returning the verdict and every progress line.
        fn tidied_touching(&self, touched: &[&str]) -> (Result<Tidied>, Vec<String>) {
            let lines = Arc::new(Mutex::new(Vec::new()));
            let sink: ProgressSink = {
                let lines = Arc::clone(&lines);
                Arc::new(move |line: &str| lines.lock().expect("lines").push(line.to_string()))
            };
            let packages: BTreeSet<String> = ["demo".to_string()].into();
            let touched: BTreeSet<String> = touched.iter().map(|file| file.to_string()).collect();
            let cancel = CancellationToken::new();
            let verdict = tidy(&Tidying {
                root: self.directory.path(),
                packages: &packages,
                touched: &touched,
                progress: &sink,
                cancel: &cancel,
            });
            let said = lines.lock().expect("lines").clone();
            (verdict, said)
        }
    }

    #[test]
    fn removes_an_import_the_compiler_reports_unused() {
        // Given a library with an import nothing uses
        let demo = a_crate_with(&[(
            "src/lib.rs",
            "use std::collections::HashMap;\n\npub fn two() -> u32 {\n    2\n}\n",
        )]);

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the import is gone and the tree compiles
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        assert!(!demo.read("src/lib.rs").contains("HashMap"));
    }

    #[test]
    fn removes_only_the_unused_name_from_a_grouped_import() {
        // Given a grouped import of which only `Read` is used
        let demo = a_crate_with(&[(
            "src/lib.rs",
            "use std::{fmt::Debug, io::Read};\n\npub fn eof(mut input: impl Read) -> bool {\n    \
             input.read(&mut [0u8; 1]).is_ok()\n}\n",
        )]);

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then `Debug` is gone and `Read` stays
        verdict.expect("the tidy runs");
        let source = demo.read("src/lib.rs");
        assert!(!source.contains("Debug"), "Debug survived:\n{source}");
        assert!(source.contains("io::Read"), "Read was removed:\n{source}");
    }

    #[test]
    fn removes_an_unused_pub_crate_re_export() {
        // Given a facade re-export nothing reads
        let demo = a_crate_with(&[(
            "src/lib.rs",
            "mod refresh {\n    pub fn now() -> u32 {\n        1\n    }\n}\n\n\
             pub(crate) use refresh::now;\n\npub fn two() -> u32 {\n    refresh::now() + 1\n}\n",
        )]);

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the facade is gone
        verdict.expect("the tidy runs");
        assert!(!demo.read("src/lib.rs").contains("pub(crate) use"));
    }

    #[test]
    fn keeps_a_trait_import_a_method_call_needs() {
        // Given an import whose trait only a method call uses
        let source = "use std::fmt::Write;\n\npub fn hi(out: &mut String) {\n    \
                      out.write_str(\"hi\").unwrap();\n}\n";
        let demo = a_crate_with(&[("src/lib.rs", source)]);

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the file is untouched
        verdict.expect("the tidy runs");
        assert_eq!(demo.read("src/lib.rs"), source);
    }

    #[test]
    fn leaves_a_file_outside_the_touched_set_alone() {
        // Given an unused import in a file the run never wrote
        let other = "use std::collections::HashSet;\n\npub fn one() -> u32 {\n    1\n}\n";
        let demo = a_crate_with(&[("src/lib.rs", "pub mod other;\n"), ("src/other.rs", other)]);

        // When only the library file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the other file keeps its warning
        verdict.expect("the tidy runs");
        assert_eq!(demo.read("src/other.rs"), other);
    }

    #[test]
    fn reports_a_remaining_warning_it_does_not_fix() {
        // Given a function nothing calls
        let source = "fn never_called() {}\n";
        let demo = a_crate_with(&[("src/lib.rs", source)]);

        // When its file is tidied
        let (_, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the warning is reported and the file untouched
        assert!(
            said.iter().any(|line| line.starts_with(
                "warning remains: src/lib.rs:1: function `never_called` is never used"
            )),
            "no remaining warning was reported: {said:?}"
        );
        assert_eq!(demo.read("src/lib.rs"), source);
    }

    #[test]
    fn reports_how_many_imports_it_removed_per_file() {
        // Given two unused imports in one file
        let demo = a_crate_with(&[(
            "src/lib.rs",
            "use std::collections::HashMap;\nuse std::collections::HashSet;\n\n\
             pub fn two() -> u32 {\n    2\n}\n",
        )]);

        // When its file is tidied
        let (_, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then one line names the file and the count
        assert!(
            said.contains(&"tidied: removed 2 unused import(s) from src/lib.rs".to_string()),
            "{said:?}"
        );
    }

    /// A crate root whose tests, and only they, use what `imports` brings in.
    const THE_TEST_MODULE: &str = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    \
                                   #[test]\n    fn counts() {\n        \
                                   assert_eq!(BTreeMap::<u8, u8>::new().len(), 0);\n    }\n}\n";

    fn a_library_whose_only_tests_use(imports: &str) -> ACrate {
        let source = format!("{imports}\n\npub fn two() -> u32 {{\n    2\n}}\n{THE_TEST_MODULE}");
        a_crate_with(&[("src/lib.rs", &source)])
    }

    #[test]
    fn keeps_an_import_only_the_test_module_uses_by_gating_it_for_tests() {
        // Given an import that only the unit-test module uses, so the library unit reports it unused
        let demo = a_library_whose_only_tests_use("use std::collections::BTreeMap;");

        // When its file is tidied
        let (verdict, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the import stays, gated, the tree compiles with its tests and nothing is reported unused
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let source = demo.read("src/lib.rs");
        assert!(
            source.starts_with("#[cfg(test)]\nuse std::collections::BTreeMap;\n\npub fn two"),
            "the import was not gated:\n{source}"
        );
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(!demo.warns_of_an_unused_import());
        assert!(
            said.contains(
                &"gated for tests: src/lib.rs: use std::collections::BTreeMap".to_string()
            ),
            "{said:?}"
        );
    }

    #[test]
    fn gates_one_member_of_a_group_and_removes_the_other() {
        // Given a group whose `BTreeMap` only tests use and whose `BTreeSet` nothing uses
        let demo = a_library_whose_only_tests_use("use std::collections::{BTreeMap, BTreeSet};");

        // When its file is tidied
        let (verdict, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then `BTreeSet` is gone, `BTreeMap` is gated in an item of its own, and the counts say so
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let source = demo.read("src/lib.rs");
        assert!(
            source.starts_with("#[cfg(test)]\nuse std::collections::BTreeMap;\n\npub fn two"),
            "{source}"
        );
        assert!(!source.contains("BTreeSet"), "{source}");
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(
            said.contains(&"tidied: removed 1 unused import(s) from src/lib.rs".to_string()),
            "{said:?}"
        );
        assert!(said
            .contains(&"gated for tests: src/lib.rs: use std::collections::BTreeMap".to_string()));
    }

    #[test]
    fn gates_the_member_only_tests_use_and_keeps_the_one_production_code_uses() {
        // Given what a split leaves: `FileEdit` only the tests use, `WorkspaceEdit` production code uses
        let source = "mod edit {\n    pub struct FileEdit;\n    pub struct WorkspaceEdit;\n\n    \
                      impl WorkspaceEdit {\n        pub fn len(&self) -> usize {\n            0\n        }\n    }\n}\n\n\
                      use crate::edit::{FileEdit, WorkspaceEdit};\n\n\
                      pub fn empty(edit: &WorkspaceEdit) -> bool {\n    edit.len() == 0\n}\n\n\
                      #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    \
                      fn builds_a_file_edit() {\n        let _edit = FileEdit;\n    }\n}\n";
        let demo = a_crate_with(&[("src/lib.rs", source)]);

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then production keeps its member and the tests get theirs, gated
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let tidied = demo.read("src/lib.rs");
        assert!(
            tidied.contains("#[cfg(test)]\nuse crate::edit::FileEdit;\n")
                && tidied.contains("use crate::edit::WorkspaceEdit;\n")
                && !tidied.contains("{FileEdit"),
            "{tidied}"
        );
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(!demo.warns_of_an_unused_import());
    }

    #[test]
    fn keeps_the_visibility_prefix_when_gating() {
        // Given a `pub(crate)` import only the tests use
        let demo = a_library_whose_only_tests_use("pub(crate) use std::collections::BTreeMap;");

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the gate sits above the whole statement, prefix intact
        verdict.expect("the tidy runs");
        let source = demo.read("src/lib.rs");
        assert!(
            source.starts_with("#[cfg(test)]\npub(crate) use std::collections::BTreeMap;\n"),
            "{source}"
        );
        demo.compiles_with_its_tests().expect("the tree compiles");
    }

    #[test]
    fn leaves_a_nested_group_member_in_place_and_reports_it() {
        // Given a nested group whose inner member only the tests use
        let demo = a_library_whose_only_tests_use(
            "use std::collections::{btree_map::{BTreeMap}, BTreeSet};\n\n\
             pub fn set() -> BTreeSet<u8> {\n    BTreeSet::new()\n}",
        );

        // When its file is tidied
        let (verdict, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the import is neither removed nor gated, the tree compiles, and the report says why
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let source = demo.read("src/lib.rs");
        assert!(source.contains("btree_map::"), "{source}");
        assert!(!source.contains("#[cfg(test)]\nuse"), "{source}");
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(
            said.iter()
                .any(|line| line.starts_with("warning remains: unused import ")
                    && line.ends_with(" could not be gated for tests (nested group)")),
            "{said:?}"
        );
    }

    #[test]
    fn still_removes_an_import_that_is_unused_everywhere_beside_a_test_module() {
        // Given a test module that does not use the import either
        let demo = a_crate_with(&[(
            "src/lib.rs",
            "use std::collections::HashMap;\n\npub fn two() -> u32 {\n    2\n}\n\n\
             #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn two_is_two() {\n        \
             assert_eq!(two(), 2);\n    }\n}\n",
        )]);

        // When its file is tidied
        let (verdict, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then it is removed, not gated
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        assert!(!demo.read("src/lib.rs").contains("HashMap"));
        assert!(
            !said.iter().any(|line| line.starts_with("gated for tests")),
            "{said:?}"
        );
    }

    #[test]
    fn fails_loudly_and_undoes_the_tidy_when_gating_cannot_repair_the_tree() {
        // Given a trait import only the tests need: the error names the method, never the trait
        let source = "use std::fmt::Write;\n\npub fn two() -> u32 {\n    2\n}\n\n\
                      #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    \
                      fn writes() {\n        let mut out = String::new();\n        \
                      out.write_str(\"2\").unwrap();\n    }\n}\n";
        let demo = a_crate_with(&[("src/lib.rs", source)]);

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the run fails, says the tidy was undone and names the import, and the file is as it was
        let Tidied::Broken { errors, .. } = verdict.expect("the tidy runs") else {
            panic!("the tidy reported a tree that compiles");
        };
        assert!(errors.contains("the tidy was undone"), "{errors}");
        assert!(errors.contains("Write"), "{errors}");
        assert_eq!(demo.read("src/lib.rs"), source);
    }

    #[test]
    fn does_nothing_and_runs_no_check_when_there_are_no_packages() {
        // Given no packages and a root that holds no crate at all, so a check would fail
        let directory = tempfile::tempdir().expect("a temporary directory");
        let packages = BTreeSet::new();
        let touched: BTreeSet<String> = ["src/lib.rs".to_string()].into();
        let progress: ProgressSink = Arc::new(|_: &str| {});
        let cancel = CancellationToken::new();

        // When the tidy runs
        let verdict = tidy(&Tidying {
            root: directory.path(),
            packages: &packages,
            touched: &touched,
            progress: &progress,
            cancel: &cancel,
        });

        // Then it passes without having checked anything
        assert_eq!(verdict.expect("nothing to do"), Tidied::Compiles);
    }
}
