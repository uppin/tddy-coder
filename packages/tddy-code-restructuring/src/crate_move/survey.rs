//! The path survey: every path a moved file names, and the crate that defines what each reaches.
//!
//! A cross-crate move used to decide what the moved file depends on, and how its paths must be
//! rewritten, from its header `use` lines read as strings. Four defects followed from that — a
//! facade followed back into the destination, the destination named by its own extern name, a
//! crate named only in a body carried nowhere, a `super::` import read as an edge back — and all of
//! them are the same gap: the move never looked at every path, and never resolved the ones it did.
//!
//! The survey reads `use` items at any depth **and** paths in bodies, resolves `self::` and
//! `super::` against the file's own module path before anything else, and follows each through the
//! origin's re-exports (glob and chained) to the crate that **defines** the item. The header
//! rewrite, the edge test and the manifest pass all read it, so they cannot disagree about what the
//! file names. `check-parity` reads it too, for the findings `check --deep` owes.

use std::collections::BTreeSet;

use super::destination::Destination;
use super::manifest_edits;
use super::source_scan::{items_of_module, sightings};
use super::test_binary::{is_a_built_in_root, names_bound_in};
use super::{malformed, reexports, Result};
use crate::edit::Position;
use crate::registry::Workspace;

/// One path the moved file writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SurveyedPath {
    /// The path as written: `super::helper`, `crate::config::DaemonConfig`, `shared::Clock`.
    pub(crate) written: String,
    /// The path with `self`/`super`/`crate` resolved against the moved file's module path,
    /// crate-rooted by extern name: `origin::outer::helper`.
    pub(crate) resolved: String,
    /// The extern name of the crate that defines the item, after following re-exports.
    pub(crate) defining_crate: String,
    /// The crate-rooted path of the item where it is defined, after following re-exports:
    /// `destination::helper_mod::helper` for the `resolved` path above when `outer` globs it in.
    /// Equal to `resolved` when the path is not the origin's to forward.
    pub(crate) defined_at: String,
    /// Whether the path is written under a `#[cfg(test)]` item — what sends a crate to
    /// `[dev-dependencies]`.
    pub(crate) in_test: bool,
    /// Whether the path is in a body rather than a `use` item.
    pub(crate) in_body: bool,
    /// Where it is written, one-based. Every leaf of one `use` tree shares the tree's.
    pub(crate) site: Position,
}

/// Every path a moved file names, in the order it names them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PathSurvey {
    pub(crate) paths: Vec<SurveyedPath>,
}

/// Survey `text`, the moved file, which sits at `module_path` (`["outer", "inner"]`) inside
/// `origin`.
///
/// A path is surveyed when it is relative to the file's own crate (`crate`, `self`, `super`) or
/// names a crate. A `use` item's first segment is a crate unless the file binds the name itself; in
/// code a path is only read as one when the origin's manifest declares its first segment, because
/// `PermissionMode::Plan` and `mpsc::channel` are paths too, and a dependency line for either would
/// be a manifest edit nobody asked for.
///
/// # Errors
///
/// Refuses when a path climbs above the crate root, or a file the walk through the origin's
/// re-exports must read cannot be.
pub(crate) fn survey_moved_file(
    workspace: &Workspace<'_>,
    text: &str,
    origin: &Destination,
    module_path: &[String],
) -> Result<PathSurvey> {
    let manifest = workspace.read(&format!("{}/Cargo.toml", origin.dir))?;
    let bound = names_bound_by(text);
    let mut paths = Vec::new();

    for sighting in sightings(text) {
        // A glob or group with nothing before it (`use *;`) names no path to resolve.
        let Some(head) = sighting.segments.first().map(String::as_str) else {
            continue;
        };
        let relative = matches!(head, "crate" | "self" | "super");
        if !relative {
            if is_a_built_in_root(head) || bound.contains(head) {
                continue;
            }
            if !sighting.in_use
                && !manifest_edits::declares_dependency_in_either_table(&manifest, head)
            {
                continue;
            }
        }

        let written = sighting.segments.join("::");
        let within: Vec<String> = module_path
            .iter()
            .chain(&sighting.modules)
            .cloned()
            .collect();
        let resolved = resolved_against(&written, &origin.extern_name, &within)?;
        let defined_at = reexports::followed(workspace, origin, &resolved)?;
        let defining_crate = defined_at
            .split("::")
            .next()
            .unwrap_or(origin.extern_name.as_str())
            .to_string();

        paths.push(SurveyedPath {
            written,
            resolved,
            defining_crate,
            defined_at,
            in_test: sighting.in_test,
            in_body: !sighting.in_use,
            site: manifest_edits::position_of(text, sighting.head_at),
        });
    }

    Ok(PathSurvey { paths })
}

/// What the file itself binds: the modules it declares, what its `use` items bring into scope, and
/// the items it defines. A path starting with one of these is relative to the file, so its first
/// segment is no crate — `use Kind::*;` in a function over an `enum Kind` of the same file.
fn names_bound_by(text: &str) -> BTreeSet<String> {
    let mut bound = names_bound_in(text);
    let items = items_of_module(text);
    bound.extend(items.defined);
    bound.extend(items.children.into_iter().map(|child| child.name));
    bound
}

/// `self::`, `super::` (any depth) and `crate::` resolved against `module_path` in `crate_name`,
/// by segment — never by string prefix. A path that climbs above the crate root is refused.
pub(crate) fn resolved_against(
    written: &str,
    crate_name: &str,
    module_path: &[String],
) -> Result<String> {
    let mut segments = written.split("::").map(str::trim).peekable();
    let mut rooted: Vec<&str> = vec![crate_name];

    match segments.peek().copied() {
        Some("crate") => {
            segments.next();
        }
        Some("self" | "super") => {
            rooted.extend(module_path.iter().map(String::as_str));
            if segments.peek() == Some(&"self") {
                segments.next();
            }
            while segments.peek() == Some(&"super") {
                segments.next();
                if rooted.len() == 1 {
                    return Err(malformed(format!(
                        "`{written}` climbs above the root of `{crate_name}`, which has no parent \
                         module"
                    )));
                }
                rooted.pop();
            }
        }
        _ => return Ok(segments.collect::<Vec<_>>().join("::")),
    }

    rooted.extend(segments);
    Ok(rooted.join("::"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::Overlay;

    fn a_module_path(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|segment| segment.to_string()).collect()
    }

    /// The survey of `moved`, written at `origin/src/moved.rs`, over an origin with no dependencies.
    fn surveying(moved: &str) -> PathSurvey {
        let directory = tempfile::tempdir().expect("a temporary directory");
        std::fs::create_dir_all(directory.path().join("origin/src")).expect("the origin directory");
        std::fs::write(
            directory.path().join("origin/Cargo.toml"),
            "[package]\nname = \"origin\"\n",
        )
        .expect("the manifest is written");
        std::fs::write(
            directory.path().join("origin/src/lib.rs"),
            "pub mod moved;\n",
        )
        .expect("the crate root is written");

        let overlay = Overlay::new();
        let workspace = Workspace {
            root: directory.path(),
            overlay: &overlay,
        };
        let origin = Destination::read(directory.path(), "origin").expect("the origin's manifest");

        survey_moved_file(&workspace, moved, &origin, &a_module_path(&["moved"]))
            .expect("the file is surveyed")
    }

    #[test]
    fn super_resolves_to_the_parent_module() {
        // Given a path written from a module two deep
        let module = a_module_path(&["outer", "inner"]);

        // When it is resolved
        let resolved = resolved_against("super::helper", "origin", &module);

        // Then it names the parent's item
        assert_eq!(
            resolved.expect("the path resolves"),
            "origin::outer::helper"
        );
    }

    #[test]
    fn super_super_climbs_two_modules() {
        // Given a path climbing twice from a module three deep
        let module = a_module_path(&["a", "b", "c"]);

        // When it is resolved
        let resolved = resolved_against("super::super::helper", "origin", &module);

        // Then it lands in the grandparent
        assert_eq!(resolved.expect("the path resolves"), "origin::a::helper");
    }

    #[test]
    fn self_resolves_to_the_module_itself() {
        // Given a `self::` path in a nested module
        let module = a_module_path(&["outer", "inner"]);

        // When it is resolved
        let resolved = resolved_against("self::local", "origin", &module);

        // Then it names an item of that module
        assert_eq!(
            resolved.expect("the path resolves"),
            "origin::outer::inner::local"
        );
    }

    #[test]
    fn crate_resolves_to_the_crate_root() {
        // Given a `crate::` path written in a module
        let module = a_module_path(&["outer"]);

        // When it is resolved
        let resolved = resolved_against("crate::config::DaemonConfig", "origin", &module);

        // Then it is rooted at the crate, whatever module it was written in
        assert_eq!(
            resolved.expect("the path resolves"),
            "origin::config::DaemonConfig"
        );
    }

    #[test]
    fn a_path_that_climbs_above_the_crate_root_is_refused() {
        // Given a module one deep, and a path climbing twice
        let module = a_module_path(&["outer"]);

        // When it is resolved
        let refusal = resolved_against("super::super::x", "origin", &module);

        // Then it is refused
        assert!(refusal
            .expect_err("the path climbs too far")
            .to_string()
            .contains("climbs above the root of `origin`"));
    }

    #[test]
    fn an_extern_path_is_left_as_written() {
        // Given a path rooted in another crate
        let module = a_module_path(&["outer"]);

        // When it is resolved
        let resolved = resolved_against("shared::Clock", "origin", &module);

        // Then it is unchanged
        assert_eq!(resolved.expect("the path resolves"), "shared::Clock");
    }

    #[test]
    fn a_use_with_nothing_before_its_glob_names_no_path() {
        // Given `use` items whose trees spell no path at all
        let moved = "use *;\nuse {*};\n";

        // When the file is surveyed
        let survey = surveying(moved);

        // Then there is nothing to resolve, and nothing went wrong
        assert_eq!(survey.paths, Vec::new());
    }

    #[test]
    fn an_item_the_file_defines_is_no_crate_for_a_nested_use() {
        // Given a function importing the variants of an enum defined in the same file
        let moved = "pub enum Kind {\n    Idle,\n}\n\npub fn idle() -> Kind {\n    use Kind::*;\n    Idle\n}\n";

        // When the file is surveyed
        let survey = surveying(moved);

        // Then `Kind` is the file's own, not a path to carry
        assert_eq!(survey.paths, Vec::new());
    }
}
