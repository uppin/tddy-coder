//! Holding the working tree's statements against a git ref's.
//!
//! The sources on both sides are read through git, so what is compared is what git says is there —
//! and only the files a comparison is defined over.

use crate::apply::{ensure_git_worktree, git_output};
use crate::Result;
use std::collections::BTreeMap;
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
    ensure_git_worktree(root)?;

    let before = sources_at(root, &against)?;
    let after = sources_now(root)?;
    Ok(crate::verify::compare(&before, &after))
}

fn sources_at(root: &Path, git_ref: &str) -> Result<BTreeMap<String, String>> {
    let listing = git_output(root, &["ls-tree", "-r", "--name-only", git_ref])?;
    let mut sources = BTreeMap::new();

    for path in listing.lines().filter(|path| is_comparable(path)) {
        let blob = git_output(root, &["show", &format!("{git_ref}:{path}")])?;
        sources.insert(path.to_string(), blob);
    }
    Ok(sources)
}

fn sources_now(root: &Path) -> Result<BTreeMap<String, String>> {
    let listing = git_output(
        root,
        &["ls-files", "--cached", "--others", "--exclude-standard"],
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
