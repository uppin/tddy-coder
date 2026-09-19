use super::crate_holding;

use crate::{
    crate_move::{destination, manifest_edits},
    registry::Workspace,
};

use super::malformed;

use std::path::Path;

use super::Result;

/// The module file's own name — the identifier `mod` declares.
pub(crate) fn module_name(source: &str) -> Result<String> {
    let stem = Path::new(source)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| malformed(format!("`{source}` is not a Rust module file")))?;

    if matches!(stem, "lib" | "main" | "mod") {
        return Err(malformed(format!(
            "`{source}` is a crate or module root, not a module — moving one moves everything it \
             declares, which is a plan of its own"
        )));
    }
    Ok(stem.to_string())
}

/// Where a module sits in its crate: the crate that owns it, and the file that declares it.
///
/// Replaces the crate-root-only assumption [`source_crate_of`] encodes. A top-level module is
/// declared by `<crate>/src/lib.rs`; a nested one by its parent's own module file, which Rust 2018
/// allows to be either `<crate>/src/<parent>.rs` or `<crate>/src/<parent>/mod.rs`. Both are looked
/// for, and the refusal survives only for a parent that exists as neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleHome {
    /// The crate directory, relative to the repository root — `packages/tddy-daemon`.
    pub crate_dir: String,
    /// The file carrying this module's `mod` declaration. The crate root for a top-level module,
    /// the parent's own module file otherwise.
    pub declared_in: String,
    /// The module path inside the crate, outermost first — `["model_registry", "store"]` for a
    /// nested module, `["host_registry"]` for a top-level one.
    pub path: Vec<String>,
}

impl ModuleHome {
    /// Whether the crate root declares this module itself.
    #[must_use]
    pub fn is_top_level(&self) -> bool {
        self.path.len() == 1
    }
}

/// Resolve a module file to the crate that owns it and the file that declares it.
///
/// This is what lets a **directory-shaped** subsystem move. `source_crate_of` requires
/// `<crate>/src/<module>.rs` and refuses everything deeper before rust-analyzer is spawned, which
/// is not an edge case — it is the normal shape of a subsystem worth extracting.
///
/// The nesting is not guessed: the anchor already carries it, and the parent's declaring file is
/// **located** on disk rather than assumed.
///
/// # Errors
///
/// Refuses when `source` is not under a crate's `src/`, and when the parent module exists as
/// neither `<crate>/src/<parent>.rs` nor `<crate>/src/<parent>/mod.rs` — naming both paths it
/// looked for.
pub fn module_home(workspace: &Workspace<'_>, source: &str, module: &str) -> Result<ModuleHome> {
    let crate_dir = crate_holding(workspace, source)?;
    let src_prefix = format!("{crate_dir}/src/");
    let within_src = source.strip_prefix(&src_prefix).ok_or_else(|| {
        malformed(format!(
            "`{source}` is not under `{src_prefix}` — `move_module_to_crate` moves a module \
             inside a crate's `src/` tree"
        ))
    })?;

    let path = module_path_within_src(within_src, module)?;
    let declared_in = if path.len() == 1 {
        format!("{crate_dir}/src/lib.rs")
    } else {
        parent_declaring_file(workspace, &crate_dir, &path[..path.len() - 1])?
    };

    Ok(ModuleHome {
        crate_dir,
        declared_in,
        path,
    })
}

/// The module path a file under `src/` carries, ending in `module`.
fn module_path_within_src(within_src: &str, module: &str) -> Result<Vec<String>> {
    let top_level = format!("{module}.rs");
    if within_src == top_level {
        return Ok(vec![module.to_string()]);
    }

    let nested_suffix = format!("/{module}.rs");
    if let Some(parent) = within_src.strip_suffix(&nested_suffix) {
        if parent.is_empty() {
            return Err(malformed(format!(
                "`{within_src}` is not a nested module file path this operation understands"
            )));
        }
        let path = parent
            .split('/')
            .map(str::to_string)
            .chain(std::iter::once(module.to_string()))
            .collect();
        return Ok(path);
    }

    Err(malformed(format!(
        "`{within_src}` is not `<crate>/src/{module}.rs` or \
         `<crate>/src/<parent>/…/{module}.rs`"
    )))
}

/// The parent's own module file — `<parent>.rs` or `<parent>/mod.rs`.
fn parent_declaring_file(
    workspace: &Workspace<'_>,
    crate_dir: &str,
    parent: &[String],
) -> Result<String> {
    let parent_base = parent.join("/");
    let as_rs = format!("{crate_dir}/src/{parent_base}.rs");
    let as_mod = format!("{crate_dir}/src/{parent_base}/mod.rs");

    if workspace.root.join(&as_rs).exists() {
        return Ok(as_rs);
    }
    if workspace.root.join(&as_mod).exists() {
        return Ok(as_mod);
    }

    Err(malformed(format!(
        "no parent module file for `{parent_base}` — looked for `{as_rs}` and `{as_mod}`"
    )))
}

/// The crate that **defines** what an origin-named path reaches, resolving one level of re-export.
///
/// A back-compat facade in the origin — `pub use tddy_daemon_kernel::config;` — makes
/// rust-analyzer canonicalise a caller's `crate::config::DaemonConfig` as
/// `tddy_daemon::config::DaemonConfig`. [`refuse_a_dependency_cycle`] reads that as the destination
/// depending on the crate it left, and refuses a move that is in fact clean.
///
/// Returns the defining crate's **extern name** when `path` resolves through a re-export, and
/// `None` when the origin genuinely defines the item — which is the case the refusal is for.
///
/// # Errors
///
/// Refuses when the origin's crate root cannot be read.
pub fn defining_crate(
    workspace: &Workspace<'_>,
    origin: &destination::Destination,
    path: &str,
) -> Result<Option<String>> {
    let segments: Vec<&str> = path.split("::").collect();
    if segments.is_empty() {
        return Ok(None);
    }

    let head = segments[0];
    if head != origin.extern_name {
        return Ok(Some(head.to_string()));
    }

    let module = segments
        .get(1)
        .ok_or_else(|| malformed(format!("`{path}` names the crate but no module inside it")))?;

    defining_module_in_crate(workspace, origin, module)
}

/// Whether `module` is defined in `origin` itself, or re-exported from another crate.
///
/// `None` when the origin's own sources define it; `Some(extern_name)` when a `pub use` brings it in.
fn defining_module_in_crate(
    workspace: &Workspace<'_>,
    origin: &destination::Destination,
    module: &str,
) -> Result<Option<String>> {
    let lib = format!("{}/src/lib.rs", origin.dir);
    let text = workspace.read(&lib)?;

    if let Some(re_export) = re_export_target(&text, module) {
        return Ok(Some(re_export));
    }

    if manifest_edits::module_declaration(&text, module).is_some() {
        return Ok(None);
    }

    Ok(None)
}

/// The extern crate a `pub use` in `text` re-exports `module` from, if any.
fn re_export_target(text: &str, module: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        let declaration = trimmed
            .strip_prefix("pub use ")
            .or_else(|| trimmed.strip_prefix("use "));
        let Some(rest) = declaration else {
            continue;
        };
        let rest = rest.trim_end_matches(';').trim();
        let (path, alias) = match rest.split_once(" as ") {
            Some((path, alias)) => (path.trim(), alias.trim()),
            None => (rest, ""),
        };

        if !alias.is_empty() {
            if alias == module {
                return extern_crate_of_use_path(path);
            }
            continue;
        }

        if let Some(name) = path.rsplit("::").next() {
            if name == module {
                return extern_crate_of_use_path(path);
            }
        }

        if let Some(inner) = path
            .strip_prefix('{')
            .and_then(|group| group.strip_suffix('}'))
        {
            for item in inner.split(',') {
                let item = item.trim();
                if item == module {
                    return extern_crate_of_use_path(path);
                }
                if let Some((_, alias)) = item.split_once(" as ") {
                    if alias.trim() == module {
                        return extern_crate_of_use_path(path);
                    }
                }
            }
        }
    }
    None
}

/// The first segment of a `use` path — the crate it names.
fn extern_crate_of_use_path(path: &str) -> Option<String> {
    path.split("::").next().map(str::to_string)
}
