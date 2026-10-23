//! Holding the working tree's statements against a git ref's.
//!
//! The sources on both sides are read through git, so what is compared is what git says is there —
//! and only the files a comparison is defined over.

use crate::apply::{ensure_git_worktree, git_output};
use crate::spawn_record::SpawnRecorder;
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::options::usage;
use super::Options;

/// Hold the working tree's statements against a git ref's, as multisets.
///
/// A comparison that does not hold is an `Ok` carrying what is missing and what was added, for the
/// same reason a check with findings is: the comparison is the answer, and what it means is the
/// caller's to decide. Only a tree that could not be compared at all — one that is not a git
/// worktree, or a ref git will not read — is an error.
pub fn verify(root: &Path, options: Options) -> Result<crate::verify::Comparison> {
    let against = options
        .against
        .clone()
        .ok_or_else(|| usage("verify needs --against <git-ref>"))?;
    ensure_git_worktree(root, &options.spawns)?;

    let before = sources_at(root, &against, &options.spawns)?;
    let after = sources_now(root, &options.spawns)?;
    let declared =
        crate::verify::Declared::from_declarations(&options.retargets, &options.repoints)
            .and_then(|declared| declared.with_rebinds(&options.rebinds))
            .and_then(|declared| declared.with_detaches(&options.detaches))
            .map_err(usage)?;
    Ok(crate::verify::compare_with(&before, &after, &declared))
}

/// R5: every file the working tree changed against `--against` that one of `options.verifies`
/// claims, handed to the first that claims it — `before` as the ref has it (empty for a file the
/// ref lacks), `after` as the tree has it (empty for a file it deleted) — and everything they
/// found, by path. Rust files are [`verify`]'s and are never handed over.
///
/// No verifies is no findings, without reading the tree: what a run that registered no language
/// beyond Rust always got.
pub fn verify_sources(root: &Path, options: &Options) -> Result<Vec<crate::verify::SourceFinding>> {
    if options.verifies.is_empty() {
        return Ok(Vec::new());
    }
    let against = options
        .against
        .as_deref()
        .ok_or_else(|| usage("verify needs --against <git-ref>"))?;
    ensure_git_worktree(root, &options.spawns)?;

    let confined = root.canonicalize()?;
    let in_ref: BTreeSet<String> = git_output(
        root,
        &["ls-tree", "-r", "--name-only", "-z", against],
        &options.spawns,
    )?
    .split('\0')
    .map(str::to_string)
    .collect();
    let mut findings = Vec::new();
    for path in changed_paths(root, against, &options.spawns)? {
        let Some(verify) = options
            .verifies
            .iter()
            .find(|verify| verify.claims(Path::new(&path)))
        else {
            continue;
        };
        let before = if in_ref.contains(&path) {
            git_output(
                root,
                &["show", &format!("{against}:{path}")],
                &options.spawns,
            )?
        } else {
            String::new()
        };
        let after = match confined_regular_file(&confined, &path) {
            Some(absolute) => std::fs::read_to_string(absolute)?,
            None => String::new(),
        };
        findings.extend(verify.verify(
            root,
            &crate::verify::SourceChange {
                path,
                before,
                after,
            },
        )?);
    }
    Ok(findings)
}

/// Every non-Rust file the working tree differs from `git_ref` in — changed, added, deleted or not
/// yet tracked — by path relative to the root, in path order.
fn changed_paths(root: &Path, git_ref: &str, spawns: &SpawnRecorder) -> Result<BTreeSet<String>> {
    let tracked = git_output(
        root,
        &["diff", "--name-only", "--no-renames", "-z", git_ref],
        spawns,
    )?;
    let untracked = git_output(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        spawns,
    )?;
    Ok(tracked
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|path| !path.is_empty() && !path.ends_with(".rs"))
        .map(str::to_string)
        .collect())
}

fn sources_at(
    root: &Path,
    git_ref: &str,
    spawns: &SpawnRecorder,
) -> Result<BTreeMap<String, String>> {
    let listing = git_output(root, &["ls-tree", "-r", "--name-only", git_ref], spawns)?;
    let mut sources = BTreeMap::new();

    for path in listing.lines().filter(|path| is_comparable(path)) {
        let blob = git_output(root, &["show", &format!("{git_ref}:{path}")], spawns)?;
        sources.insert(path.to_string(), blob);
    }
    Ok(sources)
}

fn sources_now(root: &Path, spawns: &SpawnRecorder) -> Result<BTreeMap<String, String>> {
    let listing = git_output(
        root,
        &["ls-files", "--cached", "--others", "--exclude-standard"],
        spawns,
    )?;
    let confined = root.canonicalize()?;
    let mut sources = BTreeMap::new();

    for path in listing.lines().filter(|path| is_comparable(path)) {
        if let Some(absolute) = confined_regular_file(&confined, path) {
            sources.insert(path.to_string(), std::fs::read_to_string(absolute)?);
        }
    }
    Ok(sources)
}

fn confined_regular_file(root: &Path, relative: &str) -> Option<PathBuf> {
    let candidate = root.join(relative);
    if !candidate.symlink_metadata().ok()?.is_file() {
        return None;
    }
    let real = candidate.canonicalize().ok()?;
    real.starts_with(root).then_some(real)
}

fn is_comparable(path: &str) -> bool {
    path.ends_with(".rs") && !path.starts_with("target/") && !path.contains("/target/")
}
