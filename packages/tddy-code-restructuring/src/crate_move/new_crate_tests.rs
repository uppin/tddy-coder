//! A crate move whose line carries `name` creates its destination in the same edit: the skeleton,
//! what the move writes into it, and a later operation finding it through the run's overlay.
//!
//! Library level against a reference set with no outside callers: what is decided here is text,
//! and the live test in `tests/move_module_to_crate_acceptance.rs` proves the result compiles.

use super::{cluster, resolve, resolve_cluster, ItemReferences, ModuleReferences};
use crate::edit::{FileEdit, WorkspaceEdit};
use crate::overlay::Overlay;
use crate::plan::{Plan, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

const HEADER: &str = r#"{"v":1,"snapshot":{}}"#;

const ROOT_MANIFEST: &str =
    "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/shared\",\n    \
                             \"crates/origin\",\n]\n";
const ORIGIN_MANIFEST: &str =
    "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                               [dependencies]\nshared = { path = \"../shared\" }\n";

/// A reference set in which nothing outside a moving file reaches it: no caller to re-point and
/// nothing to widen, so every edit under test is the creation's and the manifest pass's.
struct NoOutsideReferences;

impl ModuleReferences for NoOutsideReferences {
    fn outside_references(
        &mut self,
        _workspace: &Workspace<'_>,
        _file: &str,
    ) -> Result<Vec<ItemReferences>> {
        Ok(Vec::new())
    }
}

/// A workspace of `crates/shared` and `crates/origin`; `origin` declares `clock` and
/// `host_registry`, and `host_registry` names `shared::Clock`.
struct AWorkspace {
    directory: tempfile::TempDir,
    overlay: Overlay,
}

fn a_workspace_with_an_origin_naming_a_shared_crate() -> AWorkspace {
    AWorkspace {
        directory: tempfile::tempdir().expect("a temporary directory"),
        overlay: Overlay::new(),
    }
    .with("Cargo.toml", ROOT_MANIFEST)
    .with(
        "crates/shared/Cargo.toml",
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with("crates/shared/src/lib.rs", "pub struct Clock;\n")
    .with("crates/origin/Cargo.toml", ORIGIN_MANIFEST)
    .with(
        "crates/origin/src/lib.rs",
        "pub mod clock;\npub mod host_registry;\n",
    )
    .with(
        "crates/origin/src/host_registry.rs",
        "use shared::Clock;\n\npub struct HostRegistry {\n    pub clock: Clock,\n}\n",
    )
    .with("crates/origin/src/clock.rs", "pub struct Tick;\n")
}

impl AWorkspace {
    fn with(self, relative: &str, text: &str) -> Self {
        let absolute = self.directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
        self
    }

    fn workspace(&self) -> Workspace<'_> {
        Workspace {
            root: self.directory.path(),
            overlay: &self.overlay,
        }
    }

    /// The move one plan line describes, resolved against this workspace as it stands.
    fn resolving(&self, line: &str) -> WorkspaceEdit {
        resolve(
            &mut NoOutsideReferences,
            &self.workspace(),
            &the_operation(line),
        )
        .expect("the move resolves")
    }

    /// Fold `edit` into this workspace's overlay, as a dry run folds each operation it resolves.
    fn after(mut self, edit: &WorkspaceEdit) -> Self {
        self.overlay
            .record(self.directory.path(), edit)
            .expect("the edit folds into the overlay");
        self
    }

    /// What `path` reads as once `edit` is applied to this workspace.
    fn reading_after(&self, edit: &WorkspaceEdit, path: &str) -> String {
        let mut overlay = self.overlay.clone();
        overlay
            .record(self.directory.path(), edit)
            .expect("the edit folds into the overlay");
        overlay
            .read(self.directory.path(), std::path::Path::new(path))
            .expect("the file reads")
    }
}

fn the_operation(line: &str) -> RefactorOp {
    Plan::parse(&format!("{HEADER}\n{line}\n"))
        .expect("the plan parses")
        .ops
        .remove(0)
}

fn a_move_of(module: &str, to: &str, extra: &str) -> String {
    format!(
        r#"{{"op":"move_module_to_crate","anchor":{{"kind":"symbol","file":"crates/origin/src/{module}.rs","path":"{module}"}},"to":"{to}","reexport":"none"{extra}}}"#
    )
}

/// Every file the edit creates, in the order it lists them.
fn created(edit: &WorkspaceEdit) -> Vec<&str> {
    edit.changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Create { path } => Some(path.as_str()),
            _ => None,
        })
        .collect()
}

/// How many `FileEdit::Change`s the edit makes to `path`.
fn changes_to(edit: &WorkspaceEdit, path: &str) -> usize {
    edit.changes
        .iter()
        .filter(
            |change| matches!(change, FileEdit::Change { path: changed, .. } if changed == path),
        )
        .count()
}

/// Test 10 — the whole creation: a manifest naming the crate with the origin's version and edition
/// and the dependency the moved code names, and a root declaring the moved module.
#[test]
fn a_module_moved_with_name_creates_its_crate_with_a_manifest_and_a_root_declaring_it() {
    // Given `host_registry`, which names `shared`, moving into a crate that does not exist
    let workspace = a_workspace_with_an_origin_naming_a_shared_crate();

    // When the move creating `fresh` is resolved
    let edit = workspace.resolving(&a_move_of(
        "host_registry",
        "crates/fresh",
        r#","name":"fresh""#,
    ));

    // Then both files are created, each changed once, and read as the crate the module needs
    assert_eq!(
        created(&edit),
        ["crates/fresh/Cargo.toml", "crates/fresh/src/lib.rs"]
    );
    assert_eq!(changes_to(&edit, "crates/fresh/Cargo.toml"), 1);
    assert_eq!(changes_to(&edit, "crates/fresh/src/lib.rs"), 1);
    assert_eq!(
        workspace.reading_after(&edit, "crates/fresh/Cargo.toml"),
        "[package]\nname = \"fresh\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nshared = { path = \"../shared\" }\n"
    );
    assert_eq!(
        workspace.reading_after(&edit, "crates/fresh/src/lib.rs"),
        "pub mod host_registry;\n"
    );
}

/// Test 11 — the version and edition are facts about this repository, copied as the origin writes
/// them, workspace inheritance included.
#[test]
fn the_new_manifest_copies_the_origins_version_and_edition_lines_as_written() {
    // Given an origin inheriting its version and declaring edition 2024
    let workspace = a_workspace_with_an_origin_naming_a_shared_crate().with(
        "crates/origin/Cargo.toml",
        "[package]\nname = \"origin\"\nversion.workspace = true\nedition = \"2024\"\n\n\
         [dependencies]\nshared = { path = \"../shared\" }\n",
    );

    // When the move creating `fresh` is resolved
    let edit = workspace.resolving(&a_move_of(
        "host_registry",
        "crates/fresh",
        r#","name":"fresh""#,
    ));

    // Then the new `[package]` carries both lines verbatim
    assert!(
        workspace
            .reading_after(&edit, "crates/fresh/Cargo.toml")
            .starts_with(
                "[package]\nname = \"fresh\"\nversion.workspace = true\nedition = \"2024\"\n"
            ),
        "{}",
        workspace.reading_after(&edit, "crates/fresh/Cargo.toml")
    );
}

/// Test 12 — cargo builds only what the workspace lists.
#[test]
fn a_crate_created_by_a_move_is_added_to_the_workspace_members() {
    // Given a workspace listing `shared` and `origin`
    let workspace = a_workspace_with_an_origin_naming_a_shared_crate();

    // When the move creating `fresh` is resolved
    let edit = workspace.resolving(&a_move_of(
        "host_registry",
        "crates/fresh",
        r#","name":"fresh""#,
    ));

    // Then `members` gains it once, at the end
    assert_eq!(
        workspace.reading_after(&edit, "Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/shared\",\n    \
         \"crates/origin\",\n    \"crates/fresh\",\n]\n"
    );
}

/// Test 13 — a set moves into one new crate, created once and declaring every member.
#[test]
fn a_cluster_moved_with_name_creates_its_crate_once_and_declares_every_member_in_sorted_order() {
    // Given `host_registry` and `clock` moving together into a crate that does not exist
    let workspace = a_workspace_with_an_origin_naming_a_shared_crate();
    let op = the_operation(
        r#"{"op":"move_cluster_to_crate","anchor":{"kind":"symbol","file":"crates/origin/src/host_registry.rs","path":"host_registry"},"also":[{"kind":"symbol","file":"crates/origin/src/clock.rs","path":"clock"}],"to":"crates/fresh","name":"fresh","reexport":"none"}"#,
    );

    // When the cluster is resolved
    let moving = cluster::named_by(&workspace.workspace(), &op).expect("the cluster is read");
    let edit = resolve_cluster(&mut NoOutsideReferences, &workspace.workspace(), &moving)
        .expect("the cluster resolves");

    // Then the crate is created once, and its root declares both members in order
    assert_eq!(
        created(&edit),
        ["crates/fresh/Cargo.toml", "crates/fresh/src/lib.rs"]
    );
    assert_eq!(
        workspace.reading_after(&edit, "crates/fresh/src/lib.rs"),
        "pub mod clock;\npub mod host_registry;\n"
    );
}

/// Test 14 — a dry run and `check --deep` fold each operation into an overlay and never touch disk;
/// the crate the first operation created is read from there by the second.
#[test]
fn a_move_into_a_crate_an_earlier_operation_created_resolves_through_the_overlay() {
    // Given the edit of an operation creating `fresh`, folded into the run's overlay only
    let workspace = a_workspace_with_an_origin_naming_a_shared_crate();
    let creating = workspace.resolving(&a_move_of(
        "host_registry",
        "crates/fresh",
        r#","name":"fresh""#,
    ));
    let workspace = workspace.after(&creating);

    // When a second module moves into `fresh`, with no `name`
    let edit = workspace.resolving(&a_move_of("clock", "crates/fresh", ""));

    // Then it lands beside the first, and nothing is created twice
    assert_eq!(created(&edit), Vec::<&str>::new());
    assert_eq!(
        workspace.reading_after(&edit, "crates/fresh/src/lib.rs"),
        "pub mod clock;\npub mod host_registry;\n"
    );
}

/// Test 15 — the path back to the crate the module left is authored relative to the new crate's own
/// directory, however deep it sits.
#[test]
fn a_created_crate_depends_on_the_origin_by_a_path_relative_to_its_own_directory() {
    // Given `host_registry` reaching `clock`, which stays in the origin
    let workspace = a_workspace_with_an_origin_naming_a_shared_crate().with(
        "crates/origin/src/host_registry.rs",
        "use crate::clock::Tick;\n\npub struct HostRegistry {\n    pub tick: Tick,\n}\n",
    );

    // When it moves into a crate created two levels down
    let edit = workspace.resolving(&a_move_of(
        "host_registry",
        "crates/new/fresh",
        r#","name":"fresh""#,
    ));

    // Then the new manifest reaches the origin from `crates/new/fresh`
    assert_eq!(
        workspace.reading_after(&edit, "crates/new/fresh/Cargo.toml"),
        "[package]\nname = \"fresh\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\norigin = { path = \"../../origin\" }\n"
    );
}
