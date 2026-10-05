//! Finding a module of a crate from its path: the file that holds it, and where in that file its own
//! items begin and end.
//!
//! Read from the text alone — `mod` declarations followed from the crate root — so a move that
//! names a module that is not there is refused before a language server exists.

use std::ops::Range;
use std::path::{Path, PathBuf};

use super::super::failure;
use crate::crate_move::source_scan::items_of_module;
use crate::item_anchor::owning_package;
use crate::registry::Workspace;
use crate::Result;

/// The package a file belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::backends::rust) struct Package {
    /// Relative to the repository root. Empty when the manifest sits at the root.
    pub(in crate::backends::rust) dir: PathBuf,
    /// The crate's name, as a path writes it (dashes read as underscores).
    pub(in crate::backends::rust) crate_name: String,
}

pub(in crate::backends::rust) fn package_of(root: &Path, file: &str) -> Result<Package> {
    let (dir, name) = owning_package(root, file)?;
    Ok(Package {
        dir,
        crate_name: name.replace('-', "_"),
    })
}

/// A module of the crate: its file, the span of its own items inside that file, and its path below
/// the crate root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::backends::rust) struct Module {
    pub(in crate::backends::rust) file: String,
    pub(in crate::backends::rust) scope: Range<usize>,
    pub(in crate::backends::rust) path: Vec<String>,
}

/// The outcome of looking for a module.
pub(in crate::backends::rust) enum Lookup {
    Found(Module),
    /// The first segment of the path that nothing declares.
    Missing {
        at: usize,
    },
}

/// The module at `path` below the crate root of `package`, followed through the `mod` declarations.
///
/// A package with both `src/lib.rs` and `src/main.rs` is tried library first.
pub(in crate::backends::rust) fn find_module(
    workspace: &Workspace<'_>,
    package: &Package,
    path: &[String],
) -> Result<Lookup> {
    let source_dir = package.dir.join("src");
    let mut deepest = 0usize;
    let mut tried_any = false;
    for root_name in ["lib.rs", "main.rs"] {
        let root_file = source_dir.join(root_name);
        let Ok(text) = workspace.read(&root_file.to_string_lossy()) else {
            continue;
        };
        tried_any = true;
        match walk(workspace, &root_file, &text, &source_dir, path)? {
            Lookup::Found(module) => return Ok(Lookup::Found(module)),
            Lookup::Missing { at } => deepest = deepest.max(at),
        }
    }
    if !tried_any {
        return Err(failure(format!(
            "`{}` has neither `src/lib.rs` nor `src/main.rs`, so no module of it can be found",
            package.crate_name
        )));
    }
    Ok(Lookup::Missing { at: deepest })
}

fn walk(
    workspace: &Workspace<'_>,
    root_file: &Path,
    root_text: &str,
    source_dir: &Path,
    path: &[String],
) -> Result<Lookup> {
    let mut file = root_file.to_string_lossy().to_string();
    let mut text = root_text.to_string();
    let mut scope = 0..text.len();
    let mut dir = source_dir.to_path_buf();

    for (at, name) in path.iter().enumerate() {
        let items = items_of_module(&text[scope.clone()]);
        let Some(child) = items.children.iter().find(|child| &child.name == name) else {
            return Ok(Lookup::Missing { at });
        };
        if let Some(body) = &child.body {
            scope = scope.start + body.start..scope.start + body.end;
        } else {
            let candidates = [
                dir.join(format!("{name}.rs")),
                dir.join(name).join("mod.rs"),
            ];
            let Some((found, found_text)) = candidates.iter().find_map(|candidate| {
                let relative = candidate.to_string_lossy().to_string();
                workspace.read(&relative).ok().map(|body| (relative, body))
            }) else {
                return Ok(Lookup::Missing { at });
            };
            file = found;
            scope = 0..found_text.len();
            text = found_text;
        }
        dir = dir.join(name);
    }

    Ok(Lookup::Found(Module {
        file,
        scope,
        path: path.to_vec(),
    }))
}
