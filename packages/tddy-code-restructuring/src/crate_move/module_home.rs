use super::crate_holding;

use crate::{
    crate_move::{destination, header, manifest_edits},
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
/// `None` when the origin's own sources define it; `Some(extern_name)` when a `pub use` brings it
/// in — by a re-export in the crate root, or by a module file that is nothing but a forwarding
/// address.
/// The crate a crate-root facade — `pub use <crate>::{…, module, …};` or `pub use <crate>::*;` —
/// forwards `module` to, confirmed by `<crate>`'s own root declaring it.
#[allow(dead_code)] // TODO(move-facades): `defining_module_in_crate` consults this.
fn crate_root_facade_forwarding(
    workspace: &Workspace<'_>,
    root_text: &str,
    module: &str,
) -> Result<Option<String>> {
    // TODO(move-facades): implement
    let _ = (workspace, root_text, module);
    todo!("move-facades: see a module through a crate-root facade")
}

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
        return Ok(forwarded_by_the_module_file(workspace, origin, module));
    }

    Ok(None)
}

/// The crate a declared module's own file does nothing but forward to.
///
/// A `mod` line in the crate root is not proof that the crate defines anything:
/// `packages/tddy-daemon/src/config.rs` is a doc comment and `pub use tddy_daemon_kernel::config::*;`,
/// declared by the root as `pub mod config;`. Reading the root alone answers "the daemon defines
/// it", and a test re-pointed on that answer goes on naming the crate the carving was supposed to
/// take it out of.
///
/// Only the whole-file shape counts: one `pub use <crate>::<module>::*;` and nothing else. A file
/// that forwards *and* declares something of its own is a module of this crate with a re-export in
/// it, and calling the other crate its home would send a dependency to a crate holding half of what
/// the caller names. A partial re-export — a group, or a single item — is the same story.
fn forwarded_by_the_module_file(
    workspace: &Workspace<'_>,
    origin: &destination::Destination,
    module: &str,
) -> Option<String> {
    let text = module_file(workspace, origin, module)?;
    let items = items_of(&text);
    let [only] = items.as_slice() else {
        return None;
    };

    let forwarded = only
        .strip_prefix("pub use ")?
        .trim()
        .strip_suffix(';')?
        .trim()
        .strip_suffix("::*")?;
    let (crate_named, forwarded_module) = forwarded.split_once("::")?;

    (forwarded_module == module).then(|| crate_named.to_string())
}

/// A declared module's own file, as either shape Rust 2018 allows it to take.
fn module_file(
    workspace: &Workspace<'_>,
    origin: &destination::Destination,
    module: &str,
) -> Option<String> {
    workspace
        .read(&format!("{}/src/{module}.rs", origin.dir))
        .or_else(|_| workspace.read(&format!("{}/src/{module}/mod.rs", origin.dir)))
        .ok()
}

/// The lines of a file that declare something, with its documentation and attributes left out.
///
/// What is being asked is whether a file holds *only* a re-export, so everything that is not an
/// item has to stop counting — otherwise the shim's own `//!` line makes it look like a module with
/// contents.
fn items_of(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with('#'))
        .collect()
}

/// The extern crate a `pub use` in `text` re-exports `module` from, if any.
fn re_export_target(text: &str, module: &str) -> Option<String> {
    use_paths(text)
        .into_iter()
        .find(|path| re_exports(path, module))
        .and_then(|path| extern_crate_of_use_path(&path))
}

/// The path of every `use` and `pub use` in a file, each read whole.
///
/// A declaration is not a line. Both facades in this workspace are braced groups spread over
/// several lines, and the first of those lines — `pub use tddy_session_lifecycle::{` — names no
/// member at all: read a line at a time, a group of forty re-exports looks like a re-export of
/// nothing, and every module in it is reported as defined by the crate that merely passes it on.
/// So a declaration is accumulated to the `;` that ends it before anything is matched against it.
fn use_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut opened: Option<String> = None;

    for line in text.lines() {
        // A trailing `//` comment is not part of the path, and left in it would swallow the `;`
        // this scan ends a declaration on.
        let trimmed = line.split("//").next().unwrap_or_default().trim();
        let continued = opened.take();
        let fragment = match continued {
            Some(_) => trimmed,
            None => match trimmed
                .strip_prefix("pub use ")
                .or_else(|| trimmed.strip_prefix("use "))
            {
                Some(rest) => rest,
                None => continue,
            },
        };

        let declaration = match continued {
            Some(started) => format!("{started} {fragment}"),
            None => fragment.to_string(),
        };
        match declaration.trim_end().strip_suffix(';') {
            Some(path) => paths.push(path.trim().to_string()),
            None => opened = Some(declaration),
        }
    }
    paths
}

/// Whether a `use` path brings `module` into the crate writing it.
fn re_exports(path: &str, module: &str) -> bool {
    match group_members(path) {
        Some(members) => members.split(',').any(|member| arrives_as(member, module)),
        None => arrives_as(path, module),
    }
}

/// Whether one item of a `use` — a whole path, or one member of a group — arrives as `module`.
fn arrives_as(item: &str, module: &str) -> bool {
    let item = item.trim();
    let name = match item.split_once(" as ") {
        Some((_, alias)) => alias.trim(),
        None => item.rsplit("::").next().unwrap_or(item),
    };
    name == module
}

/// What a `use` path's braced group holds, when it has one.
fn group_members(path: &str) -> Option<&str> {
    let opened = path.find('{')?;
    let closed = path.rfind('}')?;
    (opened < closed).then(|| &path[opened + 1..closed])
}

/// The first segment of a `use` path — the crate it names.
///
/// `None` for a path rooted in the crate writing it: `crate`, `self` and `super` name no crate this
/// one could depend on, and reporting one of them as a defining crate would author a dependency
/// line on a keyword.
fn extern_crate_of_use_path(path: &str) -> Option<String> {
    let head = path.split("::").next()?.trim();
    let named = !matches!(head, "crate" | "self" | "super")
        && !head.is_empty()
        && head.chars().all(header::is_path_character);

    named.then(|| head.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::Overlay;

    /// One crate on disk: the root a test hands it, and whatever module files it writes.
    struct ACrate {
        directory: tempfile::TempDir,
    }

    /// A crate named `daemon` whose root says what the test needs it to say.
    fn a_crate_whose_root_declares(lib: &str) -> ACrate {
        let crate_under_test = ACrate {
            directory: tempfile::tempdir().expect("a temporary directory"),
        };
        crate_under_test
            .writing(
                "packages/daemon/Cargo.toml",
                "[package]\nname = \"daemon\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            )
            .writing("packages/daemon/src/lib.rs", lib)
    }

    impl ACrate {
        /// The module file the crate root declares, with the text this test gives it.
        fn holding(self, module: &str, text: &str) -> Self {
            self.writing(&format!("packages/daemon/src/{module}.rs"), text)
        }

        /// Which crate defines what `daemon::<module>` reaches — `None` when this crate does.
        fn crate_defining(&self, module: &str) -> Option<String> {
            let overlay = Overlay::new();
            let workspace = Workspace {
                root: self.directory.path(),
                overlay: &overlay,
            };
            let origin = destination::Destination::read(self.directory.path(), "packages/daemon")
                .expect("the crate has a manifest");

            defining_crate(&workspace, &origin, &format!("daemon::{module}"))
                .expect("the crate root reads")
        }

        fn writing(self, relative: &str, text: &str) -> Self {
            let absolute = self.directory.path().join(relative);
            std::fs::create_dir_all(absolute.parent().expect("a parent directory"))
                .expect("the directory is created");
            std::fs::write(absolute, text).expect("the file is written");
            self
        }
    }

    /// A re-export is a declaration, not a line.
    ///
    /// Every facade in this workspace is a braced group spread over several lines, and the line
    /// naming the crate — `pub use tddy_session_lifecycle::{` — names no member at all. Matched a
    /// line at a time, a group of forty re-exports looks like a re-export of nothing, and every
    /// module in it is reported as defined by the crate that merely passes it on.
    #[test]
    fn resolves_a_module_named_inside_a_multi_line_re_export_group() {
        // Given a crate root that re-exports three modules across three lines
        let daemon = a_crate_whose_root_declares(
            "//! The endpoint crate.\n\npub use session_lifecycle::{\n    action_service,\n    \
             base_sync_cache, connection_service,\n};\n",
        );

        // When the crate defining one of the group's members is asked for
        let defining = daemon.crate_defining("base_sync_cache");

        // Then it is the crate on the other side of the group
        assert_eq!(defining, Some("session_lifecycle".to_string()));
    }

    /// A `mod` line in the crate root is not proof that the crate defines anything.
    ///
    /// `packages/tddy-daemon/src/config.rs` is a doc comment and one glob re-export; the root
    /// declares it as `pub mod config;`. Reading the root alone answers "the daemon defines it",
    /// and a caller re-pointed on that answer goes on naming the crate it was moving away from.
    #[test]
    fn resolves_a_declared_module_whose_file_only_forwards_to_another_crate() {
        // Given a declared module that is nothing but a forwarding address
        let daemon = a_crate_whose_root_declares("//! The endpoint crate.\n\npub mod config;\n")
            .holding(
                "config",
                "//! Configuration, owned by the kernel.\npub use daemon_kernel::config::*;\n",
            );

        // When the crate defining it is asked for
        let defining = daemon.crate_defining("config");

        // Then it is the crate the file forwards to
        assert_eq!(defining, Some("daemon_kernel".to_string()));
    }

    /// The other half of the same question: a module with code in it stays where it is.
    ///
    /// Over-resolving is the worse failure. A module this crate really defines, sent to a crate
    /// that merely appears in its header, would move a caller's dependency to a crate that does not
    /// hold what it names.
    #[test]
    fn leaves_a_module_the_crate_itself_defines_unresolved() {
        // Given a declared module with contents of its own
        let daemon = a_crate_whose_root_declares(
            "//! The endpoint crate.\n\npub mod connection_service;\n",
        )
        .holding(
            "connection_service",
            "use daemon_kernel::config::DaemonConfig;\n\npub struct ConnectionService {\n  \
                     config: DaemonConfig,\n}\n",
        );

        // When the crate defining it is asked for
        let defining = daemon.crate_defining("connection_service");

        // Then the crate that declares it is the one that defines it
        assert_eq!(defining, None);
    }
}
