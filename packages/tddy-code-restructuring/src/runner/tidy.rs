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
//! A tidy that leaves the tree not compiling fails the run; it is never silently reverted.

mod diagnostics;
mod format;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::compile_gate::{compiler_errors, described_check, run_check};
use crate::backends::rust::ProgressSink;
use crate::{RestructureError, Result};
use diagnostics::{parse, Diagnostic, Fix};
use format::format_touched;

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
    let mut removed = BTreeMap::new();
    let mut checked = tidy_imports(tidying, &mut removed)?;
    report_removals(tidying, &removed);
    if matches!(checked, Checked::Compiles(_)) && format_touched_files(tidying)? {
        checked = check(tidying)?;
    }
    match checked {
        Checked::Broken { checked, errors } => Ok(Tidied::Broken { checked, errors }),
        Checked::Compiles(diagnostics) => {
            report_remaining_warnings(tidying, &diagnostics);
            Ok(Tidied::Compiles)
        }
    }
}

/// A check's verdict: the warnings of a tree that compiles, or why it does not.
enum Checked {
    Compiles(Vec<Diagnostic>),
    Broken { checked: String, errors: String },
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
    Ok(Checked::Broken {
        checked: described_check(tidying.packages),
        errors: errors_of(&diagnostics, &output.stderr),
    })
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

/// Remove unused imports until the compiler reports none or [`MAX_ROUNDS`] removals have been
/// made, counting what was removed per file. Returns the last check, which judged the tree as it
/// now stands.
fn tidy_imports(tidying: &Tidying<'_>, removed: &mut BTreeMap<String, usize>) -> Result<Checked> {
    let mut round = 0;
    loop {
        let checked = check(tidying)?;
        let Checked::Compiles(diagnostics) = &checked else {
            return Ok(checked);
        };
        let unused = unused_imports(diagnostics, tidying.touched);
        if unused.fixes.is_empty() || round == MAX_ROUNDS {
            return Ok(checked);
        }
        apply_fixes(tidying.root, &unused.fixes)?;
        for (file, count) in unused.imports {
            *removed.entry(file).or_default() += count;
        }
        round += 1;
    }
}

/// What the compiler says to remove: edits per file, and how many imports they remove.
struct UnusedImports {
    fixes: BTreeMap<String, BTreeSet<Fix>>,
    imports: BTreeMap<String, usize>,
}

/// The `unused_imports` removals in the touched files. Each target (library, tests) repeats a
/// diagnostic for a file they share, so edits are kept as sets.
fn unused_imports(diagnostics: &[Diagnostic], touched: &BTreeSet<String>) -> UnusedImports {
    let mut fixes: BTreeMap<String, BTreeSet<Fix>> = BTreeMap::new();
    let mut spans: BTreeSet<(String, usize)> = BTreeSet::new();
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
        spans.extend(diagnostic.primaries.iter().cloned());
    }
    let mut imports: BTreeMap<String, usize> = BTreeMap::new();
    for (file, _) in spans
        .into_iter()
        .filter(|(file, _)| fixes.contains_key(file))
    {
        *imports.entry(file).or_default() += 1;
    }
    UnusedImports { fixes, imports }
}

/// Apply every file's edits from the highest offset down, so none moves another's coordinates. An
/// edit overlapping one already applied is left for the next round to find again.
fn apply_fixes(root: &Path, fixes: &BTreeMap<String, BTreeSet<Fix>>) -> Result<()> {
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
        }
        std::fs::write(&path, bytes)?;
    }
    Ok(())
}

fn report_removals(tidying: &Tidying<'_>, removed: &BTreeMap<String, usize>) {
    for (file, count) in removed {
        (tidying.progress)(&format!(
            "tidied: removed {count} unused import(s) from {file}"
        ));
    }
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
