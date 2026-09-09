//! Moving a module's file from one crate to another.
//!
//! This is the **third** transformation this package authors itself, after `extract_class` and the
//! facade `use` line — and the only one that crosses a crate boundary. rust-analyzer has no
//! cross-crate move assist, so there is nothing to delegate to: `move_symbol` and `move_file` are
//! absent from the Rust backend's supported set, `RefactorOp::to` is read nowhere on the assist
//! path, and `extract_module_to_file` names and places its own file beside the parent module. The
//! vocabulary deliberately dropped `rewrite_import_path` for having no engine behind it, and this
//! operation is not that one returning: **it is engine-informed rather than engine-performed.**
//!
//! The distinction is the whole justification, so it is worth stating precisely. Everything this
//! module decides comes from the server:
//!
//! | Decision | Source |
//! |---|---|
//! | which callers to rewrite | `textDocument/references` on the moved module's public items |
//! | what each caller's new path is | the destination crate's name from its own `Cargo.toml` |
//! | what the moved file's own header needs | the same import-restoration pass `extract_module` uses |
//! | whether an item is reached from outside at all | the reference set, not a text search |
//!
//! What this module authors is the *mechanical* half: a `git mv`, two manifest edits, and a
//! `pub use` line. None of those is a code transformation an engine could have offered.
//!
//! # The facade
//!
//! With `reexport`, the crate the module left keeps `pub use <new_crate>::…;` and **no caller
//! changes at all** — the same principle as `extract_module`'s in-parent facade, one level up. That
//! is what made a 23,099-line intra-package split invisible to 93 referring files, and it is what
//! makes a staged cross-crate move affordable: move first with a facade, remove the facade later
//! when the callers are ready.

use std::collections::BTreeSet;
use std::path::Path;

use crate::edit::WorkspaceEdit;
use crate::plan::{Reexport, RefactorOp};
use crate::registry::Workspace;
use crate::RestructureError;

type Result<T> = std::result::Result<T, RestructureError>;

/// Where a module is going, resolved from the plan's `to` and the destination's own manifest.
///
/// The crate *name* is never taken from the directory name: `packages/tddy-daemon-kernel` could
/// declare any `[package] name`, and a caller's `use` path needs the declared one with its hyphens
/// turned into underscores. Reading the manifest is the only correct source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// The destination crate's directory, relative to the repository root, as the plan gave it.
    pub dir: String,
    /// `[package] name` from the destination's `Cargo.toml` — e.g. `tddy-daemon-kernel`.
    pub package: String,
    /// The identifier a `use` path needs — `package` with `-` replaced by `_`.
    pub extern_name: String,
}

impl Destination {
    /// Read a destination from its `Cargo.toml`.
    ///
    /// Refuses a directory with no manifest rather than creating one: a plan that names a crate
    /// which does not exist is a plan defect, and scaffolding a crate is authoring, not moving.
    pub fn read(root: &Path, dir: &str) -> Result<Destination> {
        let manifest = root.join(dir).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).map_err(|error| {
            RestructureError::MalformedPlan(format!(
                "`{dir}` is not a crate: {} could not be read ({error})",
                manifest.display()
            ))
        })?;
        let package = declared_package_name(&text).ok_or_else(|| {
            RestructureError::MalformedPlan(format!(
                "{} declares no `[package] name`",
                manifest.display()
            ))
        })?;

        Ok(Destination {
            dir: dir.to_string(),
            package: package.to_string(),
            extern_name: package.replace('-', "_"),
        })
    }
}

/// The `[package] name` a manifest declares, read without a TOML parser.
///
/// One key of one table is all this needs, and the shape it has to survive is a workspace manifest
/// where `[dependencies]` and `[[bin]]` also carry a `name`. Scoping the search to the lines between
/// `[package]` and the next table header is what keeps those out; a dependency's name being returned
/// as the crate's would produce a `use` path that compiles nowhere.
fn declared_package_name(manifest: &str) -> Option<&str> {
    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "[package]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "name")
        .and_then(|(_, value)| quoted(value))
}

/// The contents of the first double-quoted string in `value`, or `None` if it is not one.
fn quoted(value: &str) -> Option<&str> {
    let opened = value.trim_start().strip_prefix('"')?;
    opened.find('"').map(|end| &opened[..end])
}

/// One caller this move has to re-point, and the path it needs afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerRewrite {
    /// The referring file, relative to the repository root.
    pub path: String,
    /// The path as the caller writes it today — e.g. `crate::host_registry::HostRegistry`.
    pub from: String,
    /// The path it needs after the move — e.g. `tddy_host_service::HostRegistry`.
    pub to: String,
}

/// Everything the operation established before it wrote anything, so a refusal can name it.
///
/// A survey is worth having as a value rather than an internal step for the same reason
/// `refuse_stranded` names each item and each referring file: when a cross-crate move is going to be
/// wrong, the useful output is *what it found*, not that it declined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survey {
    /// The module file being moved, relative to the repository root.
    pub source: String,
    /// Where it is going.
    pub destination: Destination,
    /// Public items in the module that something outside it reaches.
    pub reached_from_outside: Vec<String>,
    /// Every caller found by `textDocument/references`, with its new path.
    pub callers: Vec<CallerRewrite>,
}

/// Survey a cross-crate move without writing anything.
///
/// This is what `restructure check --deep` rehearses, and it is deliberately separable from the
/// move: the reference set *is* the blast radius, and a plan author wants it before paying for a
/// cold index and an apply.
pub fn survey(_workspace: &Workspace<'_>, _op: &RefactorOp) -> Result<Survey> {
    // TODO(host-worktree-services): implement
    Err(RestructureError::MalformedPlan(
        "move_module_to_crate is not implemented yet".to_string(),
    ))
}

/// Resolve a cross-crate move into the multi-file edit that performs it.
///
/// The returned [`WorkspaceEdit`] carries, in this order:
///
/// 1. `FileEdit::Rename` for the module file — applied with `git mv`, so blame survives. This is the
///    first time the Rust path emits a rename; `convert_change` refused every resource operation
///    but `create` until this operation existed.
/// 2. `FileEdit::Change` for the moved file's own `use` header.
/// 3. `FileEdit::Change` for the source crate's `lib.rs`: the `mod` declaration goes, and with
///    [`Reexport::Glob`] or [`Reexport::Named`] a `pub use <new_crate>::…;` takes its place.
/// 4. `FileEdit::Change` for the destination crate's `lib.rs`: the new `mod` declaration.
/// 5. `FileEdit::Change` per caller, when no facade was asked for.
/// 6. `FileEdit::Change` for both `Cargo.toml`s — the destination's `[dependencies]` gains what the
///    moved code needs, and the workspace `members` list gains the destination if it is new.
///
/// With a facade, (5) is empty by construction. That is the difference between a move a reviewer can
/// read and one that touches ninety files.
pub fn resolve(_workspace: &Workspace<'_>, _op: &RefactorOp) -> Result<WorkspaceEdit> {
    // TODO(host-worktree-services): implement
    Err(RestructureError::MalformedPlan(
        "move_module_to_crate is not implemented yet".to_string(),
    ))
}

/// The `pub use` line a facade leaves in the crate the module left.
///
/// [`Reexport::Glob`] is one line and legal whatever moved, for the same reason it is inside a
/// parent module: a glob re-export caps at each item's own visibility rather than failing on a
/// member less visible than itself. [`Reexport::Named`] names only the items something outside
/// reaches, which the survey already knows. [`Reexport::None`] leaves nothing, and then every caller
/// in the survey is rewritten instead.
pub fn facade_line(
    destination: &Destination,
    reexport: Reexport,
    reached: &[String],
) -> Option<String> {
    let crate_name = &destination.extern_name;
    match reexport {
        Reexport::Glob => Some(format!("pub use {crate_name}::*;")),
        // A group is ordered and de-duplicated so the same survey always writes the same line: the
        // reference set arrives in whatever order the server listed it, and a facade that reordered
        // itself between runs would show up as a diff nobody asked for.
        Reexport::Named => {
            let named: BTreeSet<&str> = reached.iter().map(String::as_str).collect();
            if named.is_empty() {
                return None;
            }
            Some(format!(
                "pub use {crate_name}::{{{}}};",
                named.into_iter().collect::<Vec<_>>().join(", ")
            ))
        }
        Reexport::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Anchor, RefactorKind};

    fn a_move_of(module: &str, to: &str) -> RefactorOp {
        RefactorOp {
            op: RefactorKind::MoveModuleToCrate,
            anchor: Anchor::Symbol {
                file: format!("packages/tddy-daemon/src/{module}.rs"),
                path: module.to_string(),
            },
            name: None,
            to: Some(to.to_string()),
            variant: None,
            with_private_deps: false,
            reexport: None,
            to_file: false,
        }
    }

    fn a_destination_named(package: &str) -> Destination {
        Destination {
            dir: format!("packages/{package}"),
            package: package.to_string(),
            extern_name: package.replace('-', "_"),
        }
    }

    /// A directory name is not a crate name: `packages/tddy-host-service` may declare any
    /// `[package] name`, and a caller's `use` path needs the declared one. Reading the manifest is
    /// the only correct source, so a destination with no manifest is a plan defect rather than a
    /// crate to scaffold.
    #[test]
    fn refuses_a_destination_with_no_manifest() {
        // Given
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("packages/tddy-host-service")).unwrap();

        // When
        let outcome = Destination::read(root.path(), "packages/tddy-host-service");

        // Then
        assert!(
            outcome.is_err(),
            "a directory with no Cargo.toml is not a crate"
        );
    }

    /// The `use` path a caller needs is the declared package name with hyphens turned into
    /// underscores — not the directory name, and not the package name verbatim.
    #[test]
    fn reads_the_extern_name_from_the_declared_package_name() {
        // Given
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("packages/host-svc");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"tddy-host-service\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        // When
        let destination = Destination::read(root.path(), "packages/host-svc").unwrap();

        // Then
        assert_eq!(destination.package, "tddy-host-service");
        assert_eq!(destination.extern_name, "tddy_host_service");
    }

    /// A glob facade is one line and legal whatever moved, for the same reason it is inside a parent
    /// module: it caps at each item's own visibility rather than failing on a member less visible
    /// than itself.
    #[test]
    fn writes_a_glob_facade_naming_only_the_crate() {
        // Given
        let destination = a_destination_named("tddy-host-service");

        // When
        let line = facade_line(&destination, Reexport::Glob, &["HostRegistry".to_string()]);

        // Then
        assert_eq!(line.as_deref(), Some("pub use tddy_host_service::*;"));
    }

    /// A named facade re-exports only what something outside actually reaches, which the survey
    /// already knows — naming an item nothing reaches would force it public for no caller.
    #[test]
    fn writes_a_named_facade_covering_only_what_is_reached() {
        // Given
        let destination = a_destination_named("tddy-host-service");
        let reached = vec!["HostRegistry".to_string(), "FileHostRegistry".to_string()];

        // When
        let line = facade_line(&destination, Reexport::Named, &reached);

        // Then
        assert_eq!(
            line.as_deref(),
            Some("pub use tddy_host_service::{FileHostRegistry, HostRegistry};")
        );
    }

    /// Without a facade there is nothing to leave behind, and every caller is rewritten instead.
    #[test]
    fn writes_no_facade_when_none_was_asked_for() {
        // Given
        let destination = a_destination_named("tddy-host-service");

        // When
        let line = facade_line(&destination, Reexport::None, &["HostRegistry".to_string()]);

        // Then
        assert_eq!(line, None);
    }

    /// The survey is the blast radius, and it is worth having before paying for a cold index: a plan
    /// author wants to know which callers a move touches while the plan is still editable.
    #[test]
    fn surveys_the_callers_a_move_would_rewrite() {
        // Given
        let root = tempfile::tempdir().unwrap();
        let overlay = crate::Overlay::default();
        let workspace = Workspace {
            root: root.path(),
            overlay: &overlay,
        };
        let op = a_move_of("host_registry", "packages/tddy-host-service");

        // When
        let outcome = survey(&workspace, &op);

        // Then
        let found = outcome.expect("a survey reports what it found");
        assert_eq!(found.source, "packages/tddy-daemon/src/host_registry.rs");
        assert_eq!(found.destination.package, "tddy-host-service");
    }

    /// With a facade the caller list is empty by construction — that is the difference between a
    /// move a reviewer can read and one that touches ninety files.
    #[test]
    fn resolves_a_faceded_move_without_touching_a_single_caller() {
        // Given
        let root = tempfile::tempdir().unwrap();
        let overlay = crate::Overlay::default();
        let workspace = Workspace {
            root: root.path(),
            overlay: &overlay,
        };
        let mut op = a_move_of("host_registry", "packages/tddy-host-service");
        op.reexport = Some(Reexport::Glob);

        // When
        let outcome = resolve(&workspace, &op);

        // Then
        let edit = outcome.expect("a faceded move resolves");
        let renames = edit
            .changes
            .iter()
            .filter(|change| matches!(change, crate::FileEdit::Rename { .. }))
            .count();
        assert_eq!(renames, 1, "the module's file is moved with git mv");
    }
}
