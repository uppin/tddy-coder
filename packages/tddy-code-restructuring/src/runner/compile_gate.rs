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

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

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
pub fn refuse_a_broken_baseline(root: &Path, plan: &Plan, options: &Options) -> Result<()> {
    if options.dry_run || options.resume || options.from.is_some() {
        return Ok(());
    }
    let named = plan
        .snapshot
        .keys()
        .cloned()
        .chain(plan.ops.iter().map(|op| op.anchor.file().to_string()));
    let packages = owning_packages(root, named)?;

    match failing_check(root, &packages)? {
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
pub fn refuse_a_broken_result(
    root: &Path,
    options: &Options,
    journal: &Journal,
    paths: &StatePaths,
    applied: usize,
    total: usize,
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

    match failing_check(root, &packages)? {
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

/// Run `cargo check --all-targets` over `packages`, and return the command and the compiler's
/// errors when it fails. Nothing to check is nothing failing.
fn failing_check(root: &Path, packages: &BTreeSet<String>) -> Result<Option<(String, String)>> {
    if packages.is_empty() {
        return Ok(None);
    }
    let mut arguments = vec!["check", "--all-targets", "--message-format", "short"];
    for package in packages {
        arguments.extend(["-p", package.as_str()]);
    }
    let output = Command::new("cargo")
        .args(&arguments)
        .current_dir(root)
        .output()?;
    if output.status.success() {
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
    let said = String::from_utf8_lossy(&output.stderr);
    let errors: Vec<&str> = said.lines().filter(|line| line.contains("error")).collect();
    // Every line cargo wrote when none names an error: a failure it did not word as one is still
    // a failure, and its whole account is the only evidence there is.
    let errors = if errors.is_empty() {
        said.trim().to_string()
    } else {
        errors.join("\n")
    };
    Ok(Some((checked, errors)))
}
