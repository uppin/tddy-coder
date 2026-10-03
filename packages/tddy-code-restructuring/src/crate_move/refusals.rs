use std::path::Path;

use crate::{
    crate_move::{header, moving},
    plan::Reexport,
};

use super::PlannedRewrite;

use super::malformed;

use super::Result;

use std::collections::BTreeSet;

use crate::registry::Workspace;

/// A move that would make the two crates depend on each other is refused rather than written.
///
/// The crate the module left goes on naming it either way — through a facade, or through the
/// callers this operation re-points — so it gains a dependency on the destination. If the moved
/// code still names the crate it left, the destination depends on it back, and cargo refuses that
/// pair with an error naming neither the module nor the operation that produced it. Refusing here
/// names both, and names every path that forced it.
pub(crate) fn refuse_a_dependency_cycle(
    moving: &moving::Move,
    header: &header::Header,
    keeps_naming_it: &BTreeSet<String>,
) -> Result<()> {
    if !keeps_naming_it.contains(&moving.origin.dir) || header.origin_paths.is_empty() {
        return Ok(());
    }

    Err(malformed(format!(
        "`{}` still names `{}` ({}), so the destination would depend on the crate it left while \
         that crate goes on naming the module it lost — move what those paths reach, or move the \
         module's own dependencies with it",
        moving.source,
        moving.origin.package,
        header.origin_paths.join(", ")
    )))
}

/// Every crate directory that will name the destination once the move has been applied.
///
/// A facade keeps the crate the module left naming it. A re-pointed caller does the same from
/// whichever crate it sits in, which need not be that one — a workspace-wide move re-points callers
/// in crates the plan never mentioned, and each of them needs the dependency or stops compiling.
pub(crate) fn crates_still_naming_the_module(
    workspace: &Workspace<'_>,
    moving: &moving::Move,
    rewrites: &[PlannedRewrite],
) -> Result<BTreeSet<String>> {
    let mut crates = BTreeSet::new();
    if moving.reexport != Reexport::None {
        crates.insert(moving.origin.dir.clone());
        return Ok(crates);
    }

    for rewrite in rewrites {
        crates.insert(crate_holding(workspace, &rewrite.path)?);
    }
    Ok(crates)
}

/// The crate directory a file belongs to — the nearest ancestor with a `Cargo.toml`.
pub(crate) fn crate_holding(workspace: &Workspace<'_>, file: &str) -> Result<String> {
    let mut directory = Path::new(file).parent();
    while let Some(candidate) = directory {
        if workspace.root.join(candidate).join("Cargo.toml").exists() {
            return Ok(candidate.display().to_string());
        }
        directory = candidate.parent();
    }

    Err(malformed(format!(
        "`{file}` is in no crate — no `Cargo.toml` stands above it, so there is no manifest to \
         give the dependency its re-pointed path needs"
    )))
}
