//! A cross-crate move takes along the test modules **beside** the moved code — `#reshape` 14/19.
//!
//! A `#[cfg(test)] mod t;` declared in the file that declares a moved module, when that file stays
//! in the origin, names the moved code through `super::`. Left behind, it no longer compiles:
//! `#carve` 21/21 R9 moved three such files by hand, and re-pointed their paths and added a
//! dev-dependency by hand too. These tests decide what the move writes from a reference set the
//! test hands it, so they need no server.

mod harness;

use harness::known_references::{nothing_reaches_anything, AKnownReferenceSet};
use tddy_code_restructuring::crate_move;
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    unrunnable_moves, Anchor, Destination, FileEdit, ModuleHome, MovingCluster, Overlay, Reexport,
    RefactorKind, RefactorOp, WorkspaceEdit,
};

const ORIGIN: &str = "crates/origin";
const PARENT: &str = "crates/origin/src/parent.rs";
const MODULE_A: &str = "crates/origin/src/parent/a.rs";
const MODULE_B: &str = "crates/origin/src/parent/b.rs";
const A_TESTS: &str = "crates/origin/src/parent/a_tests.rs";
const MIXED_TESTS: &str = "crates/origin/src/parent/mixed_tests.rs";
const UNRELATED_TESTS: &str = "crates/origin/src/parent/unrelated_tests.rs";
const AB_TESTS: &str = "crates/origin/src/parent/ab_tests.rs";
const DESTINATION_MANIFEST: &str = "crates/destination/Cargo.toml";
const DESTINATION_LIB: &str = "crates/destination/src/lib.rs";
const A_TESTS_MOVED_TO: &str = "crates/destination/src/a_tests.rs";

/// `parent` declares the moving `a`, a sibling `b`, a `keeper` that stays, globs `a` and `b` in, and
/// declares three test modules beside them.
const THE_PARENT: &str = "pub mod a;\npub mod b;\npub mod keeper;\n\npub use a::*;\npub use b::*;\n\n\
                          /// Tests `a` alone.\n#[cfg(test)]\nmod a_tests;\n\n\
                          /// Tests `a` against the parent's own items.\n#[cfg(test)]\nmod mixed_tests;\n\n\
                          #[cfg(test)]\nmod unrelated_tests;\n\n#[cfg(test)]\nmod ab_tests;\n";
const THE_MODULE_A: &str = "pub struct Thing;\n\npub fn recipe() -> bool {\n    true\n}\n";
/// `b` reaches `a`'s `recipe` through the parent's glob of `a`.
const THE_MODULE_B: &str =
    "use super::recipe;\n\npub struct Other;\n\npub fn ready() -> bool {\n    recipe()\n}\n";
/// `a_tests` names `a` directly, `a` through the parent's glob, an origin facade into `other`, and
/// `tempfile`, which the origin has only as a dev-dependency.
const THE_A_TESTS: &str = "use super::a::Thing;\nuse super::recipe;\n\n#[test]\nfn recipe_holds() {\n    \
                           assert!(recipe());\n    let _ = Thing;\n    crate::facade::defined::f();\n    \
                           let _ = tempfile::tempdir();\n}\n";
/// `mixed_tests` names `a`, and the whole parent through `super::*`.
const THE_MIXED_TESTS: &str =
    "use super::*;\n\n#[test]\nfn both() {\n    let _ = (Thing, keeper::Keeper);\n}\n";
const THE_UNRELATED_TESTS: &str = "#[test]\nfn arithmetic() {\n    assert_eq!(2, 1 + 1);\n}\n";
/// `ab_tests` names both `a` and `b`.
const THE_AB_TESTS: &str =
    "use super::a::Thing;\nuse super::b::Other;\n\n#[test]\nfn both() {\n    let _ = (Thing, Other);\n}\n";

/// A workspace whose `origin` holds `parent::{a, b, keeper}` with test modules beside them, a
/// `facade` re-exporting `other::defined`, and an empty `destination`.
struct AWorkspace {
    root: tempfile::TempDir,
    overlay: Overlay,
}

fn a_workspace_whose_moving_module_has_test_modules_beside_it() -> AWorkspace {
    AWorkspace {
        root: tempfile::tempdir().expect("a temporary directory"),
        overlay: Overlay::new(),
    }
    .with(
        "Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/other\",\n    \"crates/origin\",\n    \
         \"crates/destination\",\n]\n",
    )
    .with(
        "crates/other/Cargo.toml",
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with(
        "crates/other/src/lib.rs",
        "pub mod defined {\n    pub fn f() {}\n}\n",
    )
    .with(
        "crates/origin/Cargo.toml",
        "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nother = { path = \"../other\" }\n\n[dev-dependencies]\ntempfile = \"3\"\n",
    )
    .with(
        "crates/origin/src/lib.rs",
        "//! The origin.\n\npub mod facade;\npub mod parent;\n",
    )
    .with("crates/origin/src/facade.rs", "pub use other::defined;\n")
    .with(PARENT, THE_PARENT)
    .with(MODULE_A, THE_MODULE_A)
    .with(MODULE_B, THE_MODULE_B)
    .with("crates/origin/src/parent/keeper.rs", "pub struct Keeper;\n")
    .with(A_TESTS, THE_A_TESTS)
    .with(MIXED_TESTS, THE_MIXED_TESTS)
    .with(UNRELATED_TESTS, THE_UNRELATED_TESTS)
    .with(AB_TESTS, THE_AB_TESTS)
    .with(
        DESTINATION_MANIFEST,
        "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with(DESTINATION_LIB, "//! The destination.\n\n")
}

impl AWorkspace {
    fn with(self, path: &str, text: &str) -> Self {
        let absolute = self.root.path().join(path);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
        self
    }

    /// `path` as the workspace reads it, pending edits of earlier operations included.
    fn read(&self, path: &str) -> String {
        self.workspace().read(path).expect("the file is read")
    }

    fn workspace(&self) -> Workspace<'_> {
        Workspace {
            root: self.root.path(),
            overlay: &self.overlay,
        }
    }

    /// Record `edit` as an earlier operation of the plan, the way a run carries it to the next.
    fn having_resolved(mut self, edit: &WorkspaceEdit) -> Self {
        self.overlay
            .record(self.root.path(), edit)
            .expect("the earlier edit is recorded");
        self
    }
}

fn the_destination() -> Destination {
    Destination {
        dir: "crates/destination".to_string(),
        package: "destination".to_string(),
        extern_name: "destination".to_string(),
    }
}

/// `parent::<name>`, declared by `parent.rs`.
fn the_module(name: &str) -> ModuleHome {
    ModuleHome {
        crate_dir: ORIGIN.to_string(),
        declared_in: PARENT.to_string(),
        path: vec!["parent".to_string(), name.to_string()],
    }
}

fn a_cluster_of(names: &[&str], reexport: Reexport) -> MovingCluster {
    MovingCluster {
        members: names.iter().map(|name| the_module(name)).collect(),
        destination: the_destination(),
        reexport,
        creates: None,
    }
}

/// One `move_module_to_crate` of `parent::<name>`, with a glob facade.
fn a_move_of(name: &str) -> RefactorOp {
    RefactorOp {
        id: None,
        op: RefactorKind::MoveModuleToCrate,
        anchor: Anchor::Symbol {
            file: format!("crates/origin/src/parent/{name}.rs"),
            path: format!("parent::{name}"),
        },
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport: Some(Reexport::Glob),
        to_file: false,
        also: Vec::new(),
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
    }
}

fn resolving(
    workspace: &AWorkspace,
    engine: &mut AKnownReferenceSet,
    cluster: &MovingCluster,
) -> WorkspaceEdit {
    crate_move::resolve_cluster(engine, &workspace.workspace(), cluster)
        .expect("the cluster resolves")
}

/// What the move says about the operation beside its edit.
fn notes_of(
    workspace: &AWorkspace,
    engine: &mut AKnownReferenceSet,
    cluster: &MovingCluster,
) -> Vec<String> {
    crate_move::cluster_resolution(engine, &workspace.workspace(), cluster)
        .expect("the cluster resolves")
        .notes
}

/// Every `from -> to` the edit renames, sorted.
fn renames(edit: &WorkspaceEdit) -> Vec<(String, String)> {
    let mut renamed: Vec<(String, String)> = edit
        .changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Rename { from, to } => Some((from.clone(), to.clone())),
            _ => None,
        })
        .collect();
    renamed.sort();
    renamed
}

fn renamed(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut renamed: Vec<(String, String)> = pairs
        .iter()
        .map(|(from, to)| ((*from).to_string(), (*to).to_string()))
        .collect();
    renamed.sort();
    renamed
}

/// What `path` reads as once the edit's text changes for it are applied.
fn after(workspace: &AWorkspace, edit: &WorkspaceEdit, path: &str) -> String {
    let edits: Vec<_> = edit
        .changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Change {
                path: changed,
                edits,
            } if changed == path => Some(edits.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    tddy_code_restructuring::apply::edited(workspace.read(path), &edits).expect("the edits apply")
}

fn holds_the_line(text: &str, line: &str) -> bool {
    text.lines().any(|written| written == line)
}

/// Test 1 — `a_tests` names nothing in the origin but `a`, so it travels in the same edit, landing
/// beside `a` at the destination's root; the test modules that need more stay.
#[test]
fn a_sibling_test_module_naming_only_moved_code_moves_beside_it_in_the_same_edit() {
    // Given `parent::a` moving, with `a_tests`, `mixed_tests`, `unrelated_tests` and `ab_tests`
    // declared beside it
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then `a` and `a_tests` are renamed, and nothing else
    assert_eq!(
        renames(&edit),
        renamed(&[
            (MODULE_A, "crates/destination/src/a.rs"),
            (A_TESTS, A_TESTS_MOVED_TO),
        ])
    );
}

/// Test 2 — the declaration leaves the origin with the attribute and the doc comment above it; the
/// declarations of the test modules that stay are untouched.
#[test]
fn the_origin_loses_the_test_declaration_with_its_attribute_and_doc_comment() {
    // Given `parent::a` moving
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then `parent` no longer declares `a_tests` or keeps its doc line, and still declares the rest
    let parent = after(&workspace, &edit, PARENT);
    assert_eq!(
        (
            parent.contains("mod a_tests;"),
            parent.contains("/// Tests `a` alone."),
            parent.contains(
                "/// Tests `a` against the parent's own items.\n#[cfg(test)]\nmod mixed_tests;\n"
            ),
            parent.contains("#[cfg(test)]\nmod unrelated_tests;\n"),
        ),
        (false, false, true, true),
        "{parent}"
    );
}

/// Test 3 — the destination root declares the follower under `#[cfg(test)]`, with its doc comment,
/// after the module it follows.
#[test]
fn the_destination_root_declares_the_test_module_under_cfg_test_after_its_last_line() {
    // Given `parent::a` moving
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then the root declares `a`, then the test module
    assert_eq!(
        after(&workspace, &edit, DESTINATION_LIB),
        "//! The destination.\n\npub mod a;\n\n/// Tests `a` alone.\n#[cfg(test)]\nmod a_tests;\n"
    );
}

/// Test 4 — `super::a::Thing` named the sibling `a` from beside it; at the destination's root, `a`
/// is `crate::a`.
#[test]
fn a_following_test_modules_super_path_to_the_moved_module_becomes_a_crate_path() {
    // Given `parent::a` moving
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then the follower names `a` from the destination's root
    let moved = after(&workspace, &edit, A_TESTS);
    assert!(holds_the_line(&moved, "use crate::a::Thing;"), "{moved}");
}

/// Test 5 — `super::recipe` reached `a`'s `recipe` through the parent's `pub use a::*;`. The header
/// pass follows that glob into `a`, which is moving, so the path lands in the destination: R9's
/// `super::recipe_enables_conversation_spawn` hand fix.
#[test]
fn a_super_path_through_the_parents_glob_of_a_moved_module_is_re_pointed_into_the_destination() {
    // Given `parent::a` moving
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then the follower names `recipe` where it is defined, in the destination
    let moved = after(&workspace, &edit, A_TESTS);
    assert!(holds_the_line(&moved, "use crate::a::recipe;"), "{moved}");
}

/// Test 6 — a `crate::` path through an origin facade into another crate names that crate once the
/// follower has left: R9's `crate::user_sessions_path::…` hand fix.
#[test]
fn a_crate_path_through_an_origin_facade_is_re_pointed_to_the_defining_crate() {
    // Given `parent::a` moving, and `a_tests` calling `crate::facade::defined::f()`
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then the call names `other`, which defines it
    let moved = after(&workspace, &edit, A_TESTS);
    assert!(
        holds_the_line(&moved, "    other::defined::f();"),
        "{moved}"
    );
}

/// Test 7 — a follower is test code throughout, so a crate it names is a dev-dependency of the
/// destination, never a dependency: R9's hand-written `[dev-dependencies] tempfile` line.
#[test]
fn the_crates_a_following_test_module_names_join_dev_dependencies() {
    // Given `parent::a` moving, whose only use of `tempfile` is in `a_tests`
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then `tempfile` is a dev-dependency and not a dependency
    let manifest = after(&workspace, &edit, DESTINATION_MANIFEST);
    let dependencies = manifest
        .split("[dev-dependencies]")
        .next()
        .unwrap_or_default();
    assert_eq!(
        (
            manifest.contains("[dev-dependencies]\ntempfile = \"3\"\n"),
            dependencies.contains("tempfile"),
        ),
        (true, false),
        "{manifest}"
    );
}

/// Test 8 — a file the move carries below the module, reached through a `#[cfg(test)] mod`
/// declaration, is test code throughout too: the `claude_cli_spawn_steps_tests` shape of R9.
#[test]
fn a_child_carried_under_a_cfg_test_declaration_sends_its_crates_to_dev_dependencies() {
    // Given `parent::a` moving, declaring its own test child that alone names `tempfile`
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it()
        .with(
            MODULE_A,
            "pub struct Thing;\n\npub fn recipe() -> bool {\n    true\n}\n\n\
             #[cfg(test)]\nmod inner_tests;\n",
        )
        .with(
            "crates/origin/src/parent/a/inner_tests.rs",
            "use super::Thing;\n\n#[test]\nfn builds() {\n    let _ = (Thing, tempfile::tempdir());\n}\n",
        )
        .with(A_TESTS, "use super::a::Thing;\n\n#[test]\nfn t() {\n    let _ = Thing;\n}\n");
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then `tempfile` is a dev-dependency and not a dependency
    let manifest = after(&workspace, &edit, DESTINATION_MANIFEST);
    let dependencies = manifest
        .split("[dev-dependencies]")
        .next()
        .unwrap_or_default();
    assert_eq!(
        (
            manifest.contains("[dev-dependencies]\ntempfile = \"3\"\n"),
            dependencies.contains("tempfile"),
        ),
        (true, false),
        "{manifest}"
    );
}

/// Test 9 — `mixed_tests` names `a` and also `super::*`, the parent that stays. It stays, unchanged,
/// and the resolution says which path keeps it: R9's `stack_child_spawn_tests`.
#[test]
fn a_test_module_that_also_names_code_staying_behind_stays_and_the_resolution_notes_why() {
    // Given `parent::a` moving
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let notes = notes_of(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then one note names `mixed_tests` and the path that keeps it
    assert_eq!(
        notes
            .iter()
            .filter(|note| note.contains("`mixed_tests` stays") && note.contains("`super::*`"))
            .count(),
        1,
        "{notes:?}"
    );
}

/// Test 10 — a test module whose helper another file names stays: taking it along would break that
/// file. The note names the file.
#[test]
fn a_test_module_whose_items_another_file_names_stays_and_the_resolution_notes_why() {
    // Given `a_tests` defining `shared_fixture`, which `mixed_tests` calls
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it()
        .with(
            A_TESTS,
            "use super::a::Thing;\n\npub(super) fn shared_fixture() -> Thing {\n    Thing\n}\n",
        )
        .with(
            MIXED_TESTS,
            "use super::a_tests::shared_fixture;\n\n#[test]\nfn uses() {\n    let _ = shared_fixture();\n}\n",
        );
    let mut engine = nothing_reaches_anything().reaching(
        "shared_fixture",
        A_TESTS,
        MIXED_TESTS,
        &workspace.read(MIXED_TESTS),
    );
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let notes = notes_of(&workspace, &mut engine, &cluster);

    // Then one note names `a_tests` and the file that keeps it
    assert_eq!(
        notes
            .iter()
            .filter(|note| note.contains("`a_tests` stays") && note.contains(MIXED_TESTS))
            .count(),
        1,
        "{notes:?}"
    );
}

/// Test 11 — a test module placed with `#[path]` lives wherever the attribute says; it stays, and
/// the note says why.
#[test]
fn a_test_module_placed_with_a_path_attribute_stays_and_the_resolution_notes_why() {
    // Given `parent` placing `a_tests` with `#[path]`
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it()
        .with(
            PARENT,
            &THE_PARENT.replace(
                "#[cfg(test)]\nmod a_tests;",
                "#[cfg(test)]\n#[path = \"parent/placed.rs\"]\nmod a_tests;",
            ),
        )
        .with("crates/origin/src/parent/placed.rs", THE_A_TESTS);
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let notes = notes_of(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then one note names `a_tests` and the attribute
    assert_eq!(
        notes
            .iter()
            .filter(|note| note.contains("`a_tests` stays") && note.contains("#[path]"))
            .count(),
        1,
        "{notes:?}"
    );
}

/// Test 12 (*green pin*) — `unrelated_tests` names nothing the move takes; it is left where it is,
/// declared as it was.
#[test]
fn a_test_module_naming_nothing_the_move_takes_is_left_alone() {
    // Given `parent::a` moving
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then `unrelated_tests` is neither renamed nor undeclared
    assert_eq!(
        (
            renames(&edit)
                .iter()
                .any(|(from, _)| from == UNRELATED_TESTS),
            after(&workspace, &edit, PARENT).contains("#[cfg(test)]\nmod unrelated_tests;\n"),
        ),
        (false, true)
    );
}

/// Test 13 — `ab_tests` names `a` and `b`. Moved one operation at a time, it stays while `b` is still
/// in the origin and follows once the operation moving `b` runs.
#[test]
fn a_test_module_of_two_modules_moved_by_two_operations_follows_the_second() {
    // Given `a` already moved to the destination by an earlier operation, and a `b` that names
    // nothing of `a`
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it()
        .with(MODULE_B, "pub struct Other;\n");
    let first = resolving(
        &workspace,
        &mut nothing_reaches_anything(),
        &a_cluster_of(&["a"], Reexport::Glob),
    );
    let workspace = workspace.having_resolved(&first);

    // When the operation moving `b` is resolved
    let second = resolving(
        &workspace,
        &mut nothing_reaches_anything(),
        &a_cluster_of(&["b"], Reexport::Glob),
    );

    // Then `ab_tests` stayed at the first and follows at the second
    assert_eq!(
        (
            renames(&first).iter().any(|(from, _)| from == AB_TESTS),
            renames(&second).contains(&(
                AB_TESTS.to_string(),
                "crates/destination/src/ab_tests.rs".to_string()
            )),
        ),
        (false, true)
    );
}

/// Test 14 — with no facade, a reference from inside a follower to a moved item is not a caller:
/// the follower moves, and its header pass is what re-points it.
#[test]
fn no_reference_inside_a_following_test_module_is_rewritten_as_a_caller() {
    // Given `a_tests` naming `Thing`, which `a` declares, and a move with no facade
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let mut engine = nothing_reaches_anything().reaching("Thing", MODULE_A, A_TESTS, THE_A_TESTS);
    let cluster = a_cluster_of(&["a"], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut engine, &cluster);

    // Then the follower names `Thing` from the destination's root, not by the destination's name
    let moved = after(&workspace, &edit, A_TESTS);
    assert_eq!(
        (
            moved.contains("destination::"),
            holds_the_line(&moved, "use crate::a::Thing;"),
        ),
        (false, true),
        "{moved}"
    );
}

/// Test 15 — the resolution names each follower, so a reader of the run can tell it moved.
#[test]
fn the_resolution_notes_each_test_module_that_follows() {
    // Given `parent::a` moving
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a"], Reexport::Glob);

    // When the move is resolved
    let notes = notes_of(&workspace, &mut nothing_reaches_anything(), &cluster);

    // Then one note says `a_tests` follows `parent::a`
    assert_eq!(
        notes
            .iter()
            .filter(|note| note.contains("test module `a_tests`")
                && note.contains("follows `parent::a`"))
            .count(),
        1,
        "{notes:?}"
    );
}

/// Test 16 — a moved member reaching a co-moving sibling through the parent's glob is re-pointed
/// into the destination, not back at the origin it left (which made the move refuse as a cycle).
#[test]
fn a_moved_member_reaching_a_co_moving_sibling_through_its_parents_glob_is_re_pointed_into_the_destination(
) {
    // Given `a` and `b` moving together, `b` naming `super::recipe` through the parent's glob of `a`
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it();
    let cluster = a_cluster_of(&["a", "b"], Reexport::Glob);

    // When the move is resolved
    let moved_b = crate_move::resolve_cluster(
        &mut nothing_reaches_anything(),
        &workspace.workspace(),
        &cluster,
    )
    .map(|edit| after(&workspace, &edit, MODULE_B))
    .map_err(|refusal| refusal.to_string());

    // Then `b` names `recipe` in the destination
    assert_eq!(
        moved_b,
        Ok(THE_MODULE_B.replace("use super::recipe;", "use crate::a::recipe;"))
    );
}

/// Test 18 — the merge a following test module would make is refused by `apply` in the words a plain
/// `check` reports it in.
#[test]
fn check_and_apply_refuse_a_following_test_modules_merge_with_the_same_message() {
    // Given a destination already holding `a_tests.rs`
    let workspace = a_workspace_whose_moving_module_has_test_modules_beside_it()
        .with(A_TESTS_MOVED_TO, "// already here\n");

    // When the move of `a` is checked, and resolved
    let findings = unrunnable_moves(&workspace.workspace(), &[a_move_of("a")])
        .expect("the preconditions read the workspace");
    let refusal = crate_move::resolve_cluster(
        &mut nothing_reaches_anything(),
        &workspace.workspace(),
        &a_cluster_of(&["a"], Reexport::Glob),
    )
    .map(|_| ())
    .map_err(|refusal| refusal.to_string());

    // Then the apply refuses, and the check reports the same words
    let refused = refusal.clone().err().unwrap_or_default();
    assert_eq!(
        (
            refusal.is_err(),
            refused.contains(A_TESTS_MOVED_TO),
            findings.iter().any(|finding| finding.contains(&refused)),
        ),
        (true, true, true),
        "apply: {refusal:?}\ncheck: {findings:?}"
    );
}
