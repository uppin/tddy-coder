//! Whether the tree an `apply` starts from, and the one it leaves, compile.
//!
//! An operation the engine accepts can still leave source the compiler rejects: rust-analyzer's
//! "extract into function" copied early returns into functions of another return type, and plan 10
//! of the lifecycle destructure reported "applied 6 of 6" over seven `E0308`s. So an apply ends by
//! asking the compiler, over every package it touched, and a tree that does not compile is a failed
//! run.
//!
//! **`--all-targets`.** A move re-points imports that test targets use, and a moved test binary is
//! a test target: a check of the library alone passes over exactly the breakage these operations
//! cause, and CI then finds it. The cost is checking the touched packages' tests and dev
//! dependencies — `check`, not `build`, and only those packages.
//!
//! **Why a baseline.** Afterwards, a failure the plan caused and one that was already there look
//! the same. A fresh run therefore checks the packages the plan names first, and refuses to write
//! anything into a tree that does not compile — saying so, so it is not mistaken for the plan's
//! doing.
//!
//! **Cancellable.** A check of a few packages' test targets takes minutes on a cold cache, and the
//! run's cancellation token is how its caller says it has stopped waiting — so the check is polled
//! against that token and killed when it fires, rather than left to run out on its own.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::apply::touched_paths;
use crate::crate_move::declared_package_name;
use crate::journal::{Journal, OpStatus};
use crate::{Plan, RestructureError, Result};

use super::{Options, StatePaths};

/// Refuse a fresh, writing run whose tree does not compile before the plan touches it.
///
/// Checked over the packages owning every file the plan names — its snapshot and its anchors — which
/// are the packages whose failure afterwards would otherwise be blamed on it.
///
/// Skipped for a dry run, which writes nothing, and for a run continuing a journal (`--resume`,
/// `--from`): that tree already holds the earlier run's edits, so a baseline would judge the plan's
/// own partial work, and [`refuse_a_broken_result`] checks everything the journal records anyway —
/// the same line [`super::open_run`] draws for the snapshot.
pub fn refuse_a_broken_baseline(
    root: &Path,
    plan: &Plan,
    options: &Options,
    cancel: &CancellationToken,
) -> Result<()> {
    if options.dry_run || options.resume || options.from.is_some() {
        return Ok(());
    }
    let named = plan
        .snapshot
        .keys()
        .cloned()
        .chain(plan.ops.iter().map(|op| op.anchor.file().to_string()));
    let packages = owning_packages(root, named)?;

    match failing_check(root, &packages, cancel)? {
        None => Ok(()),
        Some((checked, errors)) => {
            Err(RestructureError::BaselineDoesNotCompile { checked, errors })
        }
    }
}

/// Fail a writing run whose tree does not compile after its operations.
///
/// Over the packages owning every file the journal records a completed edit to — this run's and,
/// for a resumed run, the earlier ones' — since those are the edits the tree now holds. The edits
/// stay applied: the error says how to roll them back.
///
/// A check cancelled here leaves the same edits behind, uncompiled, so the run's progress sink is
/// told so before the cancellation is returned — the cancellation alone would read as though
/// nothing had happened.
pub fn refuse_a_broken_result(
    root: &Path,
    options: &Options,
    journal: &Journal,
    paths: &StatePaths,
    applied: usize,
    total: usize,
    cancel: &CancellationToken,
) -> Result<()> {
    if options.dry_run || applied == 0 {
        return Ok(());
    }
    let touched: BTreeSet<String> = journal
        .records
        .iter()
        .filter(|record| record.status == OpStatus::Completed)
        .filter_map(|record| record.edit.as_ref())
        .flat_map(touched_paths)
        .collect();
    let packages = owning_packages(root, touched.iter().cloned())?;

    let checked = failing_check(root, &packages, cancel);
    if matches!(checked, Err(RestructureError::CallerStopped)) {
        (options.progress)(&format!(
            "compile check cancelled: {applied} of {total} operation(s) are on disk and in the \
             journal, and were not checked to compile"
        ));
    }
    match checked? {
        None => Ok(()),
        Some((checked, errors)) => Err(RestructureError::AppliedTreeDoesNotCompile {
            applied,
            total,
            checked,
            touched: touched.into_iter().collect::<Vec<_>>().join(", "),
            journal: paths
                .journal
                .parent()
                .unwrap_or(&paths.journal)
                .display()
                .to_string(),
            errors,
        }),
    }
}

/// The `[package] name` of the nearest manifest above each file, within `root`.
///
/// Walked from the file's directory rather than read from `cargo metadata`, because a file a move
/// just renamed away no longer exists — its directory usually still does, and a manifest above it
/// certainly does. A file no package owns is compiled by nothing, so it adds nothing to check.
fn owning_packages(root: &Path, files: impl Iterator<Item = String>) -> Result<BTreeSet<String>> {
    let mut packages = BTreeSet::new();
    for file in files {
        let mut directory = root.join(&file);
        while directory.pop() && directory.starts_with(root) {
            let manifest = directory.join("Cargo.toml");
            if !manifest.is_file() {
                continue;
            }
            if let Some(name) = declared_package_name(&std::fs::read_to_string(&manifest)?) {
                packages.insert(name.to_string());
                break;
            }
        }
    }
    Ok(packages)
}

/// How often a running check is asked whether it has finished, and the token whether to stop it.
const CANCEL_CHECK: Duration = Duration::from_millis(100);

/// Run `cargo check --all-targets` over `packages`, and return the command and the compiler's
/// errors when it fails. Nothing to check is nothing failing.
///
/// Returns [`RestructureError::CallerStopped`] when `cancel` fires first, after killing the check:
/// nothing about the tree is known then, so it is neither a pass nor a failure.
fn failing_check(
    root: &Path,
    packages: &BTreeSet<String>,
    cancel: &CancellationToken,
) -> Result<Option<(String, String)>> {
    if packages.is_empty() {
        return Ok(None);
    }
    let mut arguments = vec!["check", "--all-targets", "--message-format", "short"];
    for package in packages {
        arguments.extend(["-p", package.as_str()]);
    }
    let mut child = Command::new("cargo")
        .args(&arguments)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    // Drained on a thread of its own while the check runs: a check that fails writes more than a
    // pipe holds, and a cargo blocked writing stderr would never exit for `try_wait` to see.
    let stderr = drain(&mut child);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.is_cancelled() {
            // Cargo's own `rustc` children are not killed with it; they finish the unit they are
            // compiling and exit, writing only into `target/`.
            child.kill()?;
            child.wait()?;
            return Err(RestructureError::CallerStopped);
        }
        std::thread::sleep(CANCEL_CHECK);
    };
    let said = stderr.join().unwrap_or_default();
    if status.success() {
        return Ok(None);
    }

    let checked = format!(
        "cargo check --all-targets {}",
        packages
            .iter()
            .map(|package| format!("-p {package}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Ok(Some((checked, compiler_errors(&said))))
}

/// Everything `child` writes to stderr, read to the end on a thread of its own.
fn drain(child: &mut Child) -> JoinHandle<String> {
    let mut stderr = child.stderr.take();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(stderr) = stderr.as_mut() {
            // A read that fails part-way still leaves what was read, which is the evidence
            // there is.
            let _ = stderr.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// The lines of a failed check that report an error: cargo's own `error: …` and, in the short
/// message format, the compiler's `path:line:col: error[E…]: …` / `path:line:col: error: …`.
///
/// Not every line mentioning the word: a warning about `error_handling.rs`, or a note quoting an
/// `Error` type, is not an error, and the whole point of the list is which lines are.
fn compiler_errors(said: &str) -> String {
    let errors: Vec<&str> = said
        .lines()
        .filter(|line| line.starts_with("error") || line.contains(": error"))
        .collect();
    // Every line cargo wrote when none names an error: a failure it did not word as one is still
    // a failure, and its whole account is the only evidence there is.
    if errors.is_empty() {
        said.trim().to_string()
    } else {
        errors.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_error_lines_of_a_failed_check_and_drops_the_ones_merely_mentioning_the_word() {
        // Given a failed check's stderr, with a warning and a note that mention "error"
        let said = "\
warning: unused import in src/error_handling.rs
src/lib.rs:3:5: error[E0308]: mismatched types
src/lib.rs:9:1: error: cannot find macro `nope` in this scope
note: expected `Result<(), Error>`
error: could not compile `demo` (lib) due to 2 previous errors";

        // When its errors are picked out
        let errors = compiler_errors(said);

        // Then exactly the three error lines remain, in order
        assert_eq!(
            errors,
            "src/lib.rs:3:5: error[E0308]: mismatched types\n\
             src/lib.rs:9:1: error: cannot find macro `nope` in this scope\n\
             error: could not compile `demo` (lib) due to 2 previous errors"
        );
    }

    #[test]
    fn keeps_everything_cargo_said_when_no_line_names_an_error() {
        // Given a failed check whose stderr words nothing as an error
        let said = "  Blocking waiting for file lock\nfailed to spawn rustc  \n";

        // When its errors are picked out
        let errors = compiler_errors(said);

        // Then its whole account is kept, trimmed
        assert_eq!(
            errors,
            "Blocking waiting for file lock\nfailed to spawn rustc"
        );
    }
}
