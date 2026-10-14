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
//! reported. A redo that still does not compile is repeated while each redo leaves fewer quoted
//! names failing (at most [`MAX_REPAIRS`] times); one that stops improving is undone the same way
//! and fails the run, saying so: the tidy never leaves a broken tree behind, and never touches the
//! plan's own edits.
//!
//! **Statements are rewritten from sets, not from spans.** A library is checked as itself and with
//! its tests, and each unit words its removal of one group its own way (`a, ` here, `, b` there):
//! the member edits of the two overlap. So the units are counted — one `compiler-artifact` per
//! unit — and a statement they disagree on is rebuilt from the *names*: reported by every unit,
//! removed; reported by fewer units than build the file, read by the rest, so gated in a repair;
//! reported by none, kept. Nothing is applied as two overlapping edits, and [`apply_fixes`] fails
//! loudly on an edit it cannot apply instead of dropping it — a dropped edit once left one name of
//! a wide facade group un-gated, and the redo then removed it.

mod diagnostics;
mod format;
mod gating;
#[cfg(test)]
mod wide_facade_tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::compile_gate::{compiler_errors, described_check, run_check};
use crate::backends::rust::{ProgressSink, WAIT_HEARTBEAT};
use crate::spawn_record::SpawnRecorder;
use crate::{RestructureError, Result};
use diagnostics::{parse, Diagnostic, Fix, Span};
use format::format_touched;
use gating::{place, quoted_in_errors, reconcile, to_gate_in_a_repair, Placement};

/// How many times unused imports are removed and the tree re-checked.
const MAX_ROUNDS: usize = 3;

/// What to tidy: the packages to check and the files the run wrote.
pub(super) struct Tidying<'a> {
    pub root: &'a Path,
    pub packages: &'a BTreeSet<String>,
    pub touched: &'a BTreeSet<String>,
    pub progress: &'a ProgressSink,
    pub cancel: &'a CancellationToken,
    /// Where the `cargo` and `rustfmt` this tidy runs are recorded.
    pub spawns: &'a SpawnRecorder,
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
    // The tidy's own heartbeat is the production default: `Tidying` carries the run's sink but not
    // its cadence, and the tidy's check is short next to a cold baseline. A host that injects a
    // cadence gets the default here rather than the run's.
    // TODO(apply-heartbeat): thread the run's cadence through `Tidying` so the tidy beats with it.
    let output = match run_check(
        tidying.root,
        tidying.packages,
        "json",
        tidying.spawns,
        tidying.cancel,
        tidying.progress,
        WAIT_HEARTBEAT,
    ) {
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
        self.warnings.extend(round.declined);
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
    /// `warning remains: …` lines for statements the round left alone because it could not rebuild
    /// them from the compiler's sets.
    declined: Vec<String>,
    /// How this round was redone, once a first attempt broke the tree.
    placement: Option<Placement>,
    /// What redoing it has been tried: the imports gated so far, and the quoted names the check
    /// that last failed still had — a repair only goes on while that count falls.
    repairs: Repairs,
}

/// The progress of the repairs of one round.
#[derive(Default)]
struct Repairs {
    attempts: usize,
    failing: usize,
    matched: BTreeSet<Span>,
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
    let reconciled = reconcile(&unused, &before);
    let applied = apply_fixes(root, &reconciled.fixes)?;
    Ok(Round {
        before,
        unused,
        applied,
        declined: reconciled.declined,
        placement: None,
        repairs: Repairs::default(),
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

/// How many times one round is redone with more imports gated.
const MAX_REPAIRS: usize = 5;

/// Undo a round that broke the tree and redo it with the imports the errors name gated for tests.
/// A round that was already redone is redone again while each redo leaves fewer quoted names
/// failing, up to [`MAX_REPAIRS`] times; one that stops improving is undone for good, loudly.
fn repair(
    tidying: &Tidying<'_>,
    round: Round,
    failure: Failure,
    report: &mut Report,
) -> Result<Repair> {
    restore(tidying.root, &round.before)?;
    let named = to_gate_in_a_repair(&round.unused, &round.before, &failure.quoted);
    let mut repairs = Repairs {
        attempts: round.repairs.attempts + 1,
        failing: failure.quoted.len(),
        matched: round.repairs.matched.clone(),
    };
    repairs
        .matched
        .extend(named.iter().map(|span| (*span).clone()));
    if repairs.matched.is_empty() || !round.keeps_improving(&repairs) {
        return Ok(Repair::Failed(undone(&round, &repairs, failure)));
    }
    let matched: Vec<&Span> = repairs.matched.iter().collect();
    let placement = place(&round.unused, &round.before, &matched);
    if placement.fixes.values().all(BTreeSet::is_empty) {
        report.warnings.extend(placement.declined);
        return Ok(Repair::Nothing);
    }
    let applied = apply_fixes(tidying.root, &placement.fixes)?;
    Ok(Repair::Retry(Round {
        before: round.before,
        unused: round.unused,
        applied,
        declined: round.declined,
        placement: Some(placement),
        repairs,
    }))
}

impl Round {
    /// Whether redoing the round once more is worth a check: the first redo always is, a later one
    /// only when the check after the last redo quoted fewer names than the one before it and the
    /// redo gates something new, and none after [`MAX_REPAIRS`].
    fn keeps_improving(&self, now: &Repairs) -> bool {
        let first = self.placement.is_none();
        let fewer_failing = now.failing < self.repairs.failing;
        let gates_more = now.matched.len() > self.repairs.matched.len();
        now.attempts <= MAX_REPAIRS && (first || (fewer_failing && gates_more))
    }
}

/// The failure of a round that could not be placed, saying what was undone and which imports it
/// could not place.
fn undone(round: &Round, repairs: &Repairs, failure: Failure) -> Failure {
    let culprits: Vec<&Span> = if repairs.matched.is_empty() {
        round.unused.primaries.iter().collect()
    } else {
        repairs.matched.iter().collect()
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
    /// The imports some unit reads although another reports them unused: a library is checked once
    /// as itself and once with its tests, so a name the first reports and the second does not is
    /// one only the tests use. Always a subset of `primaries`.
    read_by_a_unit: BTreeSet<Span>,
}

/// The `unused_imports` removals in the touched files. Each target (library, tests) repeats a
/// diagnostic for a file they share, so edits are kept as sets.
fn unused_imports(diagnostics: &[Diagnostic], touched: &BTreeSet<String>) -> UnusedImports {
    let mut fixes: BTreeMap<String, BTreeSet<Fix>> = BTreeMap::new();
    let mut primaries: BTreeSet<Span> = BTreeSet::new();
    let unused: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("unused_imports"))
        .collect();
    for diagnostic in &unused {
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
    let read_by_a_unit = read_by_a_unit(&unused, &primaries);
    UnusedImports {
        fixes,
        primaries,
        read_by_a_unit,
    }
}

/// The imports reported unused by fewer units than build them: every unit that does not report one
/// reads it.
fn read_by_a_unit(unused: &[&Diagnostic], primaries: &BTreeSet<Span>) -> BTreeSet<Span> {
    let mut reports: BTreeMap<&Span, (usize, usize)> = BTreeMap::new();
    for diagnostic in unused {
        for span in diagnostic
            .primaries
            .iter()
            .filter(|span| primaries.contains(*span))
        {
            let (reported, units) = reports.entry(span).or_default();
            *reported += 1;
            *units = (*units).max(diagnostic.units);
        }
    }
    reports
        .into_iter()
        .filter(|(_, (reported, units))| reported < units)
        .map(|(span, _)| span.clone())
        .collect()
}

/// Apply every file's edits from the highest offset down, so none moves another's coordinates.
/// Returns the edits that were applied — all of them.
///
/// An edit that cannot be applied is an error, never a skip: two different edits over the same text
/// (the compilation units word one removal differently, so a statement is edited twice) or an edit
/// outside the file. Nothing is written for a file until all of its edits are known to apply, so a
/// dropped fix can never look like a success.
fn apply_fixes(root: &Path, fixes: &BTreeMap<String, BTreeSet<Fix>>) -> Result<Vec<Fix>> {
    let mut applied = Vec::new();
    for (file, edits) in fixes {
        let path = root.join(file);
        let mut bytes = std::fs::read(&path)?;
        let ordered: Vec<&Fix> = edits.iter().collect();
        refuse_unappliable(file, &ordered, bytes.len())?;
        for edit in ordered.into_iter().rev() {
            bytes.splice(edit.start..edit.end, edit.replacement.bytes());
            applied.push(edit.clone());
        }
        std::fs::write(&path, bytes)?;
    }
    Ok(applied)
}

/// Fail when an edit is malformed, lies outside the `length` bytes of `file`, or overlaps another.
fn refuse_unappliable(file: &str, edits: &[&Fix], length: usize) -> Result<()> {
    let refusal = |why: String| {
        Err(RestructureError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("the tidy's import edits for {file} cannot all be applied: {why}"),
        )))
    };
    for (at, edit) in edits.iter().enumerate() {
        if edit.start > edit.end || edit.end > length {
            return refusal(format!(
                "edit {}..{} lies outside the file's {length} bytes",
                edit.start, edit.end
            ));
        }
        if let Some(other) = edits[at + 1..]
            .iter()
            .find(|other| gating::overlap(edit, other))
        {
            return refusal(format!(
                "edits {}..{} and {}..{} overlap",
                edit.start, edit.end, other.start, other.end
            ));
        }
    }
    Ok(())
}

/// Format the touched files; whether any changed, so the caller knows to check again.
fn format_touched_files(tidying: &Tidying<'_>) -> Result<bool> {
    let changed = format_touched(
        tidying.root,
        tidying.touched,
        tidying.progress,
        tidying.spawns,
    )?;
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
                spawns: &SpawnRecorder::discard(),
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

    /// Was `fails_loudly_and_undoes_the_tidy_when_gating_cannot_repair_the_tree`, which pinned this
    /// shape as the known limitation: the error names the method, never the trait. `#reshape` 3/19
    /// gates what one build reads and another reports unused, so the shape now succeeds.
    #[test]
    fn gates_a_trait_import_only_a_tests_method_call_needs() {
        // Given a trait import only the tests need: the error names the method, never the trait
        let source = "use std::fmt::Write;\n\npub fn two() -> u32 {\n    2\n}\n\n\
                      #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    \
                      fn writes() {\n        let mut out = String::new();\n        \
                      out.write_str(\"2\").unwrap();\n    }\n}\n";
        let demo = a_crate_with(&[("src/lib.rs", source)]);

        // When its file is tidied
        let (verdict, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the import is gated for the tests, the tree compiles with them and the gate is reported
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let tidied = demo.read("src/lib.rs");
        assert!(
            tidied.starts_with("#[cfg(test)]\nuse std::fmt::Write;\n"),
            "the trait import was not gated:\n{tidied}"
        );
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(
            said.contains(&"gated for tests: src/lib.rs: use std::fmt::Write".to_string()),
            "{said:?}"
        );
    }

    /// A library whose only tests call `used_by_tests` through `glob`, a glob re-export of the
    /// module holding it.
    fn a_library_whose_tests_read_a_glob(glob: &str) -> ACrate {
        let source = format!(
            "mod tested {{\n    pub(crate) fn used_by_tests() -> u8 {{\n        1\n    }}\n}}\n\
             {glob}\n\npub fn two() -> u8 {{\n    2\n}}\n\n#[cfg(test)]\nmod tests {{\n    \
             use super::*;\n\n    #[test]\n    fn reads() {{\n        \
             assert_eq!(used_by_tests(), 1);\n    }}\n}}\n"
        );
        a_crate_with(&[("src/lib.rs", &source)])
    }

    #[test]
    fn gates_a_glob_only_the_tests_read_and_the_tree_compiles_with_its_tests() {
        // Given a glob re-export that only the unit-test module reads
        let demo = a_library_whose_tests_read_a_glob("pub(crate) use tested::*;");

        // When its file is tidied
        let (verdict, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the glob stays for the tests, gated, and the tree compiles with them
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let tidied = demo.read("src/lib.rs");
        assert!(
            tidied.contains("#[cfg(test)]\npub(crate) use tested::*;\n"),
            "the glob was not gated:\n{tidied}"
        );
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(
            said.contains(&"gated for tests: src/lib.rs: use tested::*".to_string()),
            "{said:?}"
        );
    }

    #[test]
    fn removes_a_glob_no_build_reads() {
        // Given a glob re-export of a module nothing reaches through it
        let demo = a_crate_with(&[(
            "src/lib.rs",
            "mod gone {\n    pub(crate) fn never() {}\n}\npub(crate) use gone::*;\n\n\
             pub fn two() -> u8 {\n    gone::never();\n    2\n}\n",
        )]);

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then the glob is gone and the tree compiles
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let tidied = demo.read("src/lib.rs");
        assert!(
            !tidied.contains("use gone::*"),
            "the glob survived:\n{tidied}"
        );
    }

    /// A library whose module `parts` holds `names`, imported in one group by the library file; its
    /// production code calls `used_by_production` and its tests call `used_by_tests`.
    fn a_library_with_a_group(
        names: &[&str],
        used_by_production: &[&str],
        used_by_tests: &[&str],
    ) -> ACrate {
        let functions: String = names
            .iter()
            .map(|name| format!("    pub fn {name}() -> u8 {{\n        1\n    }}\n"))
            .collect();
        let calls = |called: &[&str]| -> String {
            called.iter().map(|name| format!("{name}() + ")).collect()
        };
        let source = format!(
            "mod parts {{\n{functions}}}\nuse parts::{{{}}};\n\npub fn total() -> u8 {{\n    {}0\n}}\n\n\
             #[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn counts() {{\n        \
             assert!({}0 >= 1);\n    }}\n}}\n",
            names.join(", "),
            calls(used_by_production),
            calls(used_by_tests),
        );
        a_crate_with(&[("src/lib.rs", &source)])
    }

    #[test]
    fn gates_four_members_of_a_group_and_removes_the_fifth() {
        // Given a group of six: production reads `kept`, the tests read four, nothing reads the last
        let demo = a_library_with_a_group(
            &["one", "two", "three", "four", "five", "kept"],
            &["kept"],
            &["one", "two", "three", "four"],
        );

        // When its file is tidied
        let (verdict, said) = demo.tidied_touching(&["src/lib.rs"]);

        // Then each of the four is gated in an item of its own, the fifth is gone, the tree compiles
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let source = demo.read("src/lib.rs");
        let gated = |name: &str| source.contains(&format!("#[cfg(test)]\nuse parts::{name};\n"));
        assert!(
            gated("one") && gated("two") && gated("three") && gated("four"),
            "{source}"
        );
        assert!(source.contains("use parts::kept;\n"), "{source}");
        assert!(!source.contains("parts::five"), "{source}");
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(!demo.warns_of_an_unused_import());
        assert_eq!(
            said.iter()
                .filter(|line| line.starts_with("gated for tests"))
                .count(),
            4,
            "{said:?}"
        );
    }

    #[test]
    fn tidies_eleven_wide_groups_in_one_file_in_one_round() {
        // Given eleven modules, each imported in a group of three: production reads the first, the
        // tests the second, nothing the third
        let demo = a_library_with_eleven_groups();

        // When its file is tidied
        let (verdict, _) = demo.tidied_touching(&["src/lib.rs"]);

        // Then every group keeps its first, gates its second and loses its third
        assert_eq!(verdict.expect("the tidy runs"), Tidied::Compiles);
        let source = demo.read("src/lib.rs");
        for group in 0..11 {
            assert!(
                source.contains(&format!("use m{group}::a{group};\n")),
                "{source}"
            );
            assert!(
                source.contains(&format!("#[cfg(test)]\nuse m{group}::b{group};\n")),
                "{source}"
            );
            assert!(!source.contains(&format!("m{group}::c{group}")), "{source}");
        }
        demo.compiles_with_its_tests().expect("the tree compiles");
        assert!(!demo.warns_of_an_unused_import());
    }

    fn a_library_with_eleven_groups() -> ACrate {
        let mut source = String::new();
        let mut production = String::new();
        let mut tests = String::new();
        for group in 0..11 {
            source += &format!(
                "mod m{group} {{\n    pub fn a{group}() -> u8 {{ 1 }}\n    pub fn b{group}() -> u8 {{ 1 }}\n    \
                 pub fn c{group}() -> u8 {{ 1 }}\n}}\nuse m{group}::{{a{group}, b{group}, c{group}}};\n\n"
            );
            production += &format!("a{group}() + ");
            tests += &format!("b{group}() + ");
        }
        source += &format!(
            "pub fn total() -> u8 {{\n    {production}0\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    \
             #[test]\n    fn counts() {{\n        assert!({tests}0 >= 1);\n    }}\n}}\n"
        );
        a_crate_with(&[("src/lib.rs", &source)])
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
            spawns: &SpawnRecorder::discard(),
        });

        // Then it passes without having checked anything
        assert_eq!(verdict.expect("nothing to do"), Tidied::Compiles);
    }
}
