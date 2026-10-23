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
use super::{Options, Outcome};

/// The refusal of a verify run that names no ref to compare the tree against.
const VERIFY_NEEDS_AGAINST: &str = "verify needs --against <git-ref>";

/// The suffix of the files the statement comparison owns, and that are never handed to a verify of
/// another language.
const RUST_SOURCE_SUFFIX: &str = ".rs";

/// Hold the working tree's statements against a git ref's, as multisets.
///
/// A comparison that does not hold is an `Ok` carrying what is missing and what was added, for the
/// same reason a check with findings is: the comparison is the answer, and what it means is the
/// caller's to decide. Only a tree that could not be compared at all — one that is not a git
/// worktree, or a ref git will not read — is an error.
pub fn verify(root: &Path, options: Options) -> Result<crate::verify::Comparison> {
    let against = against_in_worktree(root, &options)?.to_string();

    let before = sources_at(root, &against, &options.spawns)?;
    let after = sources_now(root, &options.spawns)?;
    let declared =
        crate::verify::Declared::from_declarations(&options.retargets, &options.repoints)
            .and_then(|declared| declared.with_rebinds(&options.rebinds))
            .and_then(|declared| declared.with_detaches(&options.detaches))
            .map_err(usage)?;
    Ok(crate::verify::compare_with(&before, &after, &declared))
}

/// [`verify`] and [`verify_sources`] as one answer: [`Outcome::Verified`] when no verify of a language
/// beyond Rust found anything, [`Outcome::VerifiedWithFindings`] when one did.
///
/// A run that registered no verify beyond Rust does not read which files changed: it has nothing to
/// hand them to.
pub fn verified(root: &Path, options: Options) -> Result<Outcome> {
    let changed = if options.verifies.is_empty() {
        BTreeSet::new()
    } else {
        changed_sources(root, &options)?
    };
    let (comparison, findings) = verify_with_sources(root, options, &changed)?;
    Ok(if findings.is_empty() {
        Outcome::Verified(comparison)
    } else {
        Outcome::VerifiedWithFindings(comparison, findings)
    })
}

/// [`verify`] and [`verify_sources`] over `changed` — the [`changed_sources`] the caller already
/// read, so a front end that read them to decide which servers to start does not read them twice.
///
/// The findings are read first, because [`verify`] consumes the options.
pub fn verify_with_sources(
    root: &Path,
    options: Options,
    changed: &BTreeSet<String>,
) -> Result<(crate::verify::Comparison, Vec<crate::verify::SourceFinding>)> {
    let findings = verify_sources(root, &options, changed)?;
    let comparison = verify(root, options)?;
    Ok((comparison, findings))
}

/// The non-Rust files the working tree differs from `--against` in — changed, added, deleted or not
/// yet tracked — by path relative to the root: the files [`verify_sources`] offers to the verifies,
/// for a front end that starts a language's server only when one of its files is among them.
pub fn changed_sources(root: &Path, options: &Options) -> Result<BTreeSet<String>> {
    let against = against_in_worktree(root, options)?;
    changed_paths(root, against, &options.spawns)
}

/// R5: every file of `changed` — the [`changed_sources`] of this tree — that one of
/// `options.verifies` claims, handed to the first that claims it — `before` as the ref has it
/// (empty for a file the ref lacks), `after` as the tree has it (empty for a file it deleted) — and
/// everything they found, by path. Rust files are [`verify`]'s and are never handed over.
///
/// No verifies is no findings, without reading the tree: what a run that registered no language
/// beyond Rust always got.
pub fn verify_sources(
    root: &Path,
    options: &Options,
    changed: &BTreeSet<String>,
) -> Result<Vec<crate::verify::SourceFinding>> {
    if options.verifies.is_empty() {
        return Ok(Vec::new());
    }
    let against = against_in_worktree(root, options)?;
    let in_ref = files_in_ref(root, against, &options.spawns)?;
    let confined = root.canonicalize()?;
    let mut findings = Vec::new();
    for path in changed {
        let Some(verify) = options
            .verifies
            .iter()
            .find(|verify| verify.claims(Path::new(path)))
        else {
            continue;
        };
        let change = source_change(root, &confined, against, &in_ref, path, &options.spawns)?;
        findings.extend(verify.verify(root, &change)?);
    }
    Ok(findings)
}

/// The ref a verify run compares against, once the root is known to be a git worktree.
fn against_in_worktree<'a>(root: &Path, options: &'a Options) -> Result<&'a str> {
    let against = options
        .against
        .as_deref()
        .ok_or_else(|| usage(VERIFY_NEEDS_AGAINST))?;
    ensure_git_worktree(root, &options.spawns)?;
    Ok(against)
}

/// Every file `git_ref` holds under the root, by path relative to it.
fn files_in_ref(root: &Path, git_ref: &str, spawns: &SpawnRecorder) -> Result<BTreeSet<String>> {
    Ok(git_output(
        root,
        &["ls-tree", "-r", "--name-only", "-z", git_ref],
        spawns,
    )?
    .split('\0')
    .filter(|path| !path.is_empty())
    .map(str::to_string)
    .collect())
}

/// `path` as `git_ref` has it — empty when `in_ref` lacks it — and as the tree under `confined` has
/// it — empty when it is gone or is not a regular file inside the root.
fn source_change(
    root: &Path,
    confined: &Path,
    git_ref: &str,
    in_ref: &BTreeSet<String>,
    path: &str,
    spawns: &SpawnRecorder,
) -> Result<crate::verify::SourceChange> {
    let before = if in_ref.contains(path) {
        blob_at(root, git_ref, path, spawns)?
    } else {
        String::new()
    };
    let after = match confined_regular_file(confined, path) {
        Some(absolute) => std::fs::read_to_string(absolute)?,
        None => String::new(),
    };
    Ok(crate::verify::SourceChange {
        path: path.to_string(),
        before,
        after,
    })
}

/// The contents `git_ref` holds for `path`, relative to the root — `./` makes git read it from the
/// root rather than from the repository's top level, which differ when the root is a subdirectory.
fn blob_at(root: &Path, git_ref: &str, path: &str, spawns: &SpawnRecorder) -> Result<String> {
    git_output(root, &["show", &format!("{git_ref}:./{path}")], spawns)
}

/// Every non-Rust file the working tree differs from `git_ref` in — changed, added, deleted or not
/// yet tracked — by path relative to the root, in path order.
///
/// `--relative` keeps the diff to the root and its paths relative to it, as `ls-files` and
/// `ls-tree` already are: without it a root below the repository's top level gets the top level's
/// paths.
fn changed_paths(root: &Path, git_ref: &str, spawns: &SpawnRecorder) -> Result<BTreeSet<String>> {
    let tracked = git_output(
        root,
        &[
            "diff",
            "--name-only",
            "--relative",
            "--no-renames",
            "-z",
            git_ref,
        ],
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
        .filter(|path| !path.is_empty() && !path.ends_with(RUST_SOURCE_SUFFIX))
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
        sources.insert(path.to_string(), blob_at(root, git_ref, path, spawns)?);
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
    path.ends_with(RUST_SOURCE_SUFFIX) && !path.starts_with("target/") && !path.contains("/target/")
}
