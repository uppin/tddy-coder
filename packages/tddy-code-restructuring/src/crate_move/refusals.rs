use std::path::Path;

use crate::{
    crate_move::{destination, header, module_home, moving},
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
    workspace: &Workspace<'_>,
    moving: &moving::Move,
    header: &header::Header,
    keeps_naming_it: &BTreeSet<String>,
) -> Result<()> {
    if !keeps_naming_it.contains(&moving.origin.dir) {
        return Ok(());
    }

    let origin_dependencies = origin_named_dependencies(workspace, moving, header)?;
    if origin_dependencies.is_empty() {
        return Ok(());
    }

    Err(malformed(format!(
        "`{}` still names `{}` ({}), so the destination would depend on the crate it left while \
         that crate goes on naming the module it lost — move what those paths reach, or move the \
         module's own dependencies with it",
        moving.source,
        moving.origin.package,
        origin_dependencies.join(", ")
    )))
}

/// Paths in the moved header that genuinely name the crate the module left, after re-export resolution.
pub(crate) fn origin_named_dependencies(
    workspace: &Workspace<'_>,
    moving: &moving::Move,
    header: &header::Header,
) -> Result<Vec<String>> {
    let mut kept = Vec::new();
    for path in &header.origin_paths {
        match module_home::defining_crate(workspace, &moving.origin, path)? {
            Some(defining) if defining == moving.destination.extern_name => {}
            Some(defining) if defining == moving.origin.extern_name => kept.push(path.clone()),
            Some(_) => {}
            None => kept.push(path.clone()),
        }
    }
    Ok(kept)
}

/// Every extern crate the moved code will need in its destination manifest.
pub(crate) fn crates_the_moved_code_names(
    workspace: &Workspace<'_>,
    origin: &destination::Destination,
    header: &header::Header,
) -> Result<BTreeSet<String>> {
    let mut named = BTreeSet::new();
    for path in &header.origin_paths {
        match module_home::defining_crate(workspace, origin, path)? {
            Some(defining) => {
                named.insert(defining);
            }
            None => {
                named.insert(origin.extern_name.clone());
            }
        }
    }
    for extern_name in &header.crates_named {
        if extern_name != &origin.extern_name {
            named.insert(extern_name.clone());
        }
    }
    Ok(named)
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
