//! A cross-crate move of a module declared `pub(crate)`, `pub(super)`, `pub(in …)` or privately —
//! `#reshape` 5/19.
//!
//! The move used to refuse such a module as one its parent "declares no `mod`" for, and `#carve`
//! R1 and R4 widened three declarations to `pub mod` by hand to get past it. The facade a move
//! leaves in the declaration's place keeps the declaration's own visibility, so a move never makes
//! a module more visible than it was. No server: nothing reaches the moved modules.

use tddy_code_restructuring::crate_move::{self, ItemReferences, ModuleReferences};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    Destination, FileEdit, ModuleHome, MovingCluster, Overlay, Reexport, RestructureError,
    WorkspaceEdit,
};

const ORIGIN_LIB: &str = "crates/origin/src/lib.rs";
const OUTER: &str = "crates/origin/src/outer.rs";

/// No item of a moved module is reached from outside it.
struct NothingReachesTheModules;

impl ModuleReferences for NothingReachesTheModules {
    fn outside_references(
        &mut self,
        _workspace: &Workspace<'_>,
        _file: &str,
    ) -> Result<Vec<ItemReferences>, RestructureError> {
        Ok(Vec::new())
    }
}

/// An `origin` whose root is `root`, holding each named file, and an empty `destination`.
fn an_origin_whose_root_is(root: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for (relative, text) in [
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (ORIGIN_LIB, root),
        (
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/destination/src/lib.rs", "\n"),
    ]
    .iter()
    .chain(files)
    {
        let absolute = directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
    }
    directory
}

/// A glob move of each module at `paths` (declared by `declared_in`) into `destination`.
fn a_glob_move_of(paths: &[&[&str]], declared_in: &str) -> MovingCluster {
    MovingCluster {
        members: paths
            .iter()
            .map(|path| ModuleHome {
                crate_dir: "crates/origin".to_string(),
                declared_in: declared_in.to_string(),
                path: path.iter().map(|segment| (*segment).to_string()).collect(),
            })
            .collect(),
        destination: Destination {
            dir: "crates/destination".to_string(),
            package: "destination".to_string(),
            extern_name: "destination".to_string(),
        },
        reexport: Reexport::Glob,
        creates: None,
    }
}

/// What `path` reads as once the move's edits are applied to it.
fn after_moving(root: &tempfile::TempDir, cluster: &MovingCluster, path: &str) -> String {
    let overlay = Overlay::new();
    let workspace = Workspace {
        root: root.path(),
        overlay: &overlay,
    };
    let edit: WorkspaceEdit =
        crate_move::resolve_cluster(&mut NothingReachesTheModules, &workspace, cluster)
            .expect("the move resolves");
    let edits: Vec<_> = edit
        .changes
        .into_iter()
        .filter_map(|change| match change {
            FileEdit::Change {
                path: changed,
                edits,
            } if changed == path => Some(edits),
            _ => None,
        })
        .flatten()
        .collect();
    let before = std::fs::read_to_string(root.path().join(path)).expect("the file is read");
    tddy_code_restructuring::apply::edited(before, &edits).expect("the edits apply")
}

#[test]
fn a_pub_crate_module_moved_with_a_glob_facade_leaves_a_pub_crate_facade() {
    // Given `config`, declared `pub(crate)` by the root
    let origin = an_origin_whose_root_is(
        "pub(crate) mod config;\npub mod runtime;\n",
        &[
            ("crates/origin/src/config.rs", "pub struct Settings;\n"),
            ("crates/origin/src/runtime.rs", "\n"),
        ],
    );

    // When it moves with a glob facade
    let root = after_moving(
        &origin,
        &a_glob_move_of(&[&["config"]], ORIGIN_LIB),
        ORIGIN_LIB,
    );

    // Then the facade is as visible as the declaration was
    assert_eq!(
        root,
        "pub(crate) use destination::config;\npub mod runtime;\n"
    );
}

#[test]
fn a_pub_super_module_in_a_nested_parent_leaves_a_pub_super_facade() {
    // Given `outer::inner`, declared `pub(super)` by `outer`
    let origin = an_origin_whose_root_is(
        "pub mod outer;\n",
        &[
            (OUTER, "pub(super) mod inner;\n\npub struct Outer;\n"),
            ("crates/origin/src/outer/inner.rs", "pub struct Inner;\n"),
        ],
    );

    // When it moves with a glob facade
    let outer = after_moving(
        &origin,
        &a_glob_move_of(&[&["outer", "inner"]], OUTER),
        OUTER,
    );

    // Then `outer`'s facade is `pub(super)`, like the declaration it replaces
    assert_eq!(
        outer,
        "pub(super) use destination::inner;\n\npub struct Outer;\n"
    );
}

#[test]
fn a_private_module_moved_with_a_glob_facade_leaves_a_private_use() {
    // Given `config`, declared privately by the root
    let origin = an_origin_whose_root_is(
        "mod config;\npub mod runtime;\n",
        &[
            ("crates/origin/src/config.rs", "pub struct Settings;\n"),
            ("crates/origin/src/runtime.rs", "\n"),
        ],
    );

    // When it moves with a glob facade
    let root = after_moving(
        &origin,
        &a_glob_move_of(&[&["config"]], ORIGIN_LIB),
        ORIGIN_LIB,
    );

    // Then the facade is a private `use`, not a `pub use` that would publish it
    assert_eq!(root, "use destination::config;\npub mod runtime;\n");
}

#[test]
fn two_pub_crate_modules_moved_to_one_destination_leave_one_grouped_pub_crate_line_beside_a_pub_line(
) {
    // Given two `pub(crate)` modules and a `pub` one, moving together
    let origin = an_origin_whose_root_is(
        "pub(crate) mod auth;\npub(crate) mod config;\npub mod paths;\n",
        &[
            ("crates/origin/src/auth.rs", "pub struct Token;\n"),
            ("crates/origin/src/config.rs", "pub struct Settings;\n"),
            ("crates/origin/src/paths.rs", "pub struct Root;\n"),
        ],
    );

    // When they move with a glob facade
    let root = after_moving(
        &origin,
        &a_glob_move_of(&[&["auth"], &["config"], &["paths"]], ORIGIN_LIB),
        ORIGIN_LIB,
    );

    // Then each visibility keeps one line
    assert_eq!(
        root,
        "pub(crate) use destination::{auth, config};\npub use destination::paths;\n"
    );
}

#[test]
fn a_pub_module_moved_with_a_glob_facade_still_leaves_pub_use() {
    // Given `config`, declared `pub` by the root
    let origin = an_origin_whose_root_is(
        "pub mod config;\npub mod runtime;\n",
        &[
            ("crates/origin/src/config.rs", "pub struct Settings;\n"),
            ("crates/origin/src/runtime.rs", "\n"),
        ],
    );

    // When it moves with a glob facade
    let root = after_moving(
        &origin,
        &a_glob_move_of(&[&["config"]], ORIGIN_LIB),
        ORIGIN_LIB,
    );

    // Then the facade is `pub use`, as before
    assert_eq!(root, "pub use destination::config;\npub mod runtime;\n");
}
