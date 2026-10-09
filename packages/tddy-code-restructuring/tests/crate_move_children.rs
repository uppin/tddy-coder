//! A cross-crate move carries a module's **directory children** — `#reshape` 5/19.
//!
//! A Rust 2018 module `a` is `a.rs`, and the files of its children sit in `a/`. Moving `a.rs` alone
//! strands them (`E0583`), which is what `#live-plan` 12/15 and `#carve` 21/21 R3, R4 and R7 each
//! fixed by hand: a `git mv` of the directory, the manifest lines only the children needed, and a
//! self re-export deleted from the moved parent. These tests decide what the move writes from a
//! reference set the test hands it, so they need no server.

use std::collections::BTreeMap;

use tddy_code_restructuring::crate_move::{
    self, DeclarationKind, ItemReferences, ModuleReferences, Reference,
};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    Anchor, Destination, FileEdit, ModuleHome, MovingCluster, Overlay, Position, Reexport,
    RefactorKind, RefactorOp, RestructureError, WorkspaceEdit,
};

const ORIGIN: &str = "crates/origin";
const ORIGIN_LIB: &str = "crates/origin/src/lib.rs";
const PARENT: &str = "crates/origin/src/x.rs";
const MODULE: &str = "crates/origin/src/x/a.rs";
const CHILD: &str = "crates/origin/src/x/a/b.rs";
const RESTRICTED_CHILD: &str = "crates/origin/src/x/a/c/mod.rs";
const GRANDCHILD: &str = "crates/origin/src/x/a/c/e.rs";
const INLINE_CHILD: &str = "crates/origin/src/x/a/i/d.rs";
const CALLER: &str = "crates/origin/src/runtime.rs";
const DESTINATION_MANIFEST: &str = "crates/destination/Cargo.toml";
const DESTINATION_LIB: &str = "crates/destination/src/lib.rs";

/// `a`, the module that moves: a `pub` child `b`, a `pub(crate)` child `c` in the `mod.rs` shape
/// with a grandchild `e`, and an inline module `i` holding a file-backed `d`. Its `pub use` of `b`
/// is a path inside the tree.
const THE_MODULE: &str = "pub mod b;\npub(crate) mod c;\nmod i {\n    mod d;\n}\n\n\
                          pub use b::{make, Item};\n\npub struct A;\n";
/// `b` reaches its parent through `super::`, its sibling through `crate::x::a::…`, and two crates
/// nothing else in the tree names: `shared`, and `proptest` only from its tests.
const THE_CHILD: &str = "use super::A;\nuse crate::x::a::c::C;\nuse shared::Clock;\n\n\
                         pub struct Item;\n\npub fn make() -> (A, C, Clock) {\n    (A, C, Clock)\n}\n\n\
                         #[cfg(test)]\nmod tests {\n    use proptest::prelude::*;\n}\n";
/// `runtime` stays behind and names an item of the child `b`.
const THE_CALLER: &str = "use crate::x::a::b::Item;\n\npub fn boot() -> Item {\n    Item\n}\n";

/// A workspace whose `origin` crate holds `x::a` with its directory children and a top-level
/// `top` with one child `leaf`, a `runtime` calling into `a::b`, and an empty `destination`.
struct AWorkspace {
    root: tempfile::TempDir,
    overlay: Overlay,
}

fn a_workspace_whose_module_has_directory_children() -> AWorkspace {
    AWorkspace {
        root: tempfile::tempdir().expect("a temporary directory"),
        overlay: Overlay::default(),
    }
    .with(
        "Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/shared\",\n    \"crates/origin\",\n    \
         \"crates/destination\",\n]\n",
    )
    .with(
        "crates/shared/Cargo.toml",
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with("crates/shared/src/lib.rs", "pub struct Clock;\n")
    .with(
        "crates/origin/Cargo.toml",
        "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nshared = { path = \"../shared\" }\n\n[dev-dependencies]\nproptest = \"1\"\n",
    )
    .with(
        ORIGIN_LIB,
        "//! The origin.\n\npub mod runtime;\npub mod top;\npub mod x;\n",
    )
    .with(PARENT, "pub mod a;\n")
    .with(MODULE, THE_MODULE)
    .with(CHILD, THE_CHILD)
    .with(RESTRICTED_CHILD, "mod e;\n\npub struct C;\n")
    .with(GRANDCHILD, "pub fn helper() {}\n")
    .with(INLINE_CHILD, "pub fn inner() {}\n")
    .with("crates/origin/src/top.rs", "pub mod leaf;\n\npub struct Top;\n")
    .with("crates/origin/src/top/leaf.rs", "pub struct Leaf;\n")
    .with(CALLER, THE_CALLER)
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

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.path().join(path)).expect("the file is read")
    }

    fn workspace(&self) -> Workspace<'_> {
        Workspace {
            root: self.root.path(),
            overlay: &self.overlay,
        }
    }
}

/// A reference set standing in for `textDocument/references`: which places outside a file name
/// each item it declares.
#[derive(Default)]
struct AKnownReferenceSet {
    by_file: BTreeMap<String, Vec<ItemReferences>>,
}

fn nothing_reaches_the_tree() -> AKnownReferenceSet {
    AKnownReferenceSet::default()
}

impl AKnownReferenceSet {
    /// Every place `from` names `item`, which the file `declared_in` declares.
    fn reaching(
        mut self,
        item: &str,
        declared_in: &str,
        from: &str,
        workspace: &AWorkspace,
    ) -> Self {
        let text = workspace.read(from);
        let referenced_at = text
            .match_indices(item)
            .map(|(offset, _)| Reference {
                path: from.to_string(),
                at: position_of(&text, offset),
            })
            .collect();
        self.by_file
            .entry(declared_in.to_string())
            .or_default()
            .push(ItemReferences {
                item: item.to_string(),
                referenced_at,
                // The widening pass is not what these tests are about; a top-level item at the
                // file's start is what the reference set stood for before declarations had positions.
                declared_at: Position { line: 1, col: 1 },
                within: Vec::new(),
                kind: DeclarationKind::Item,
            });
        self
    }
}

impl ModuleReferences for AKnownReferenceSet {
    fn outside_references(
        &mut self,
        _workspace: &Workspace<'_>,
        file: &str,
    ) -> Result<Vec<ItemReferences>, RestructureError> {
        Ok(self.by_file.get(file).cloned().unwrap_or_default())
    }
}

/// The one-based position of a byte offset, as the server reports a reference.
fn position_of(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    Position {
        line: before.matches('\n').count() as u32 + 1,
        col: before
            .rsplit('\n')
            .next()
            .map_or(0, |line| line.chars().count()) as u32
            + 1,
    }
}

fn the_destination() -> Destination {
    Destination {
        dir: "crates/destination".to_string(),
        package: "destination".to_string(),
        extern_name: "destination".to_string(),
    }
}

/// The module at `path` in `origin`, declared by `declared_in`.
fn a_member(path: &[&str], declared_in: &str) -> ModuleHome {
    ModuleHome {
        crate_dir: ORIGIN.to_string(),
        declared_in: declared_in.to_string(),
        path: path.iter().map(|segment| (*segment).to_string()).collect(),
    }
}

fn the_module_a() -> ModuleHome {
    a_member(&["x", "a"], PARENT)
}

fn a_cluster_of(members: Vec<ModuleHome>, reexport: Reexport) -> MovingCluster {
    MovingCluster {
        members,
        destination: the_destination(),
        reexport,
    }
}

/// One `move_module_to_crate` of the module whose file is `file`.
fn a_move_of(file: &str, path: &str) -> RefactorOp {
    RefactorOp {
        id: None,
        op: RefactorKind::MoveModuleToCrate,
        anchor: Anchor::Symbol {
            file: file.to_string(),
            path: path.to_string(),
        },
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport: Some(Reexport::None),
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

/// What `path` reads as once the edit's text changes for it are applied — its own text where the
/// edit has none.
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

/// Every file the edit lands in the destination, as it reads there.
fn arriving(workspace: &AWorkspace, edit: &WorkspaceEdit) -> Vec<(String, String)> {
    renames(edit)
        .into_iter()
        .map(|(from, to)| (to, after(workspace, edit, &from)))
        .collect()
}

/// The whole tree under `a` moves in one edit: the module's file and every file its `mod`
/// declarations lead to — both child shapes, the grandchild and the child of an inline module.
#[test]
fn a_moved_module_carries_every_file_its_mod_declarations_lead_to_in_the_same_edit() {
    // Given `x::a` with its directory children, moving alone
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then every file of the tree is renamed to the same place under the destination's `src/`
    assert_eq!(
        renames(&edit),
        renamed(&[
            (MODULE, "crates/destination/src/a.rs"),
            (CHILD, "crates/destination/src/a/b.rs"),
            (RESTRICTED_CHILD, "crates/destination/src/a/c/mod.rs"),
            (GRANDCHILD, "crates/destination/src/a/c/e.rs"),
            (INLINE_CHILD, "crates/destination/src/a/i/d.rs"),
        ])
    );
}

/// A top-level module carries its directory to the destination root's own directory.
#[test]
fn a_carried_tree_lands_at_the_same_relative_places_under_the_destination() {
    // Given the top-level `top`, whose child `leaf` lives in `top/`
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(vec![a_member(&["top"], ORIGIN_LIB)], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then the child keeps its place beside its module
    assert_eq!(
        renames(&edit),
        renamed(&[
            ("crates/origin/src/top.rs", "crates/destination/src/top.rs"),
            (
                "crates/origin/src/top/leaf.rs",
                "crates/destination/src/top/leaf.rs"
            ),
        ])
    );
}

/// The destination's root declares the module; its children stay declared by their parent.
#[test]
fn the_destination_root_declares_the_moved_module_and_none_of_its_children() {
    // Given `x::a` with its directory children, moving alone
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then the destination's root gains `a` alone
    assert_eq!(
        after(&workspace, &edit, DESTINATION_LIB),
        "//! The destination.\n\npub mod a;\n"
    );
}

/// A carried child's paths are re-pointed for where it lands; a `super::` that stays inside the
/// carried tree still means the same thing and is left as written.
#[test]
fn a_carried_childs_crate_paths_are_re_pointed_and_its_super_paths_are_left_as_written() {
    // Given `x::a` moving, whose child `b` names its sibling through `crate::x::a::c`
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then `crate::x::a::c` becomes `crate::a::c`, and the rest of the child is untouched
    assert_eq!(
        after(&workspace, &edit, CHILD),
        THE_CHILD.replace("use crate::x::a::c::C;", "use crate::a::c::C;")
    );
}

/// The destination's manifest gains what the children name, not only what the module's own file
/// names: `shared` as a dependency, and `proptest`, which only a child's tests name, as a
/// dev-dependency.
#[test]
fn a_crate_only_a_child_names_joins_the_destination_dependencies_and_one_only_its_tests_name_joins_dev_dependencies(
) {
    // Given `x::a` moving, whose own file names no crate and whose child `b` names two
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then the destination's manifest declares each in its table
    let manifest = after(&workspace, &edit, DESTINATION_MANIFEST);
    assert_eq!(
        (
            manifest.contains("[dependencies]\nshared = { path = \"../shared\" }\n"),
            manifest.contains("[dev-dependencies]\nproptest = \"1\"\n"),
        ),
        (true, true),
        "{manifest}"
    );
}

/// A caller outside the moved tree of an item a carried child declares is re-pointed at the
/// destination when no facade is asked for.
#[test]
fn a_caller_of_an_item_in_a_carried_child_is_re_pointed_when_no_facade_is_asked_for() {
    // Given `runtime`, which stays, naming `Item` of the carried child `b`
    let workspace = a_workspace_whose_module_has_directory_children();
    let mut engine = nothing_reaches_the_tree().reaching("Item", CHILD, CALLER, &workspace);
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut engine, &cluster);

    // Then its path names the destination
    assert_eq!(
        after(&workspace, &edit, CALLER),
        THE_CALLER.replace("crate::x::a::b::Item", "destination::a::b::Item")
    );
}

/// A reference from one carried file to another is not a caller: both files move, and the header
/// pass is what re-points it.
#[test]
fn a_reference_from_inside_the_carried_tree_is_not_a_caller() {
    // Given the server reporting `C`, declared by the carried `c`, named in the carried `b`
    let workspace = a_workspace_whose_module_has_directory_children();
    let mut engine = nothing_reaches_the_tree().reaching("C", RESTRICTED_CHILD, CHILD, &workspace);
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut engine, &cluster);

    // Then `b` reads exactly as its header pass leaves it, with no caller rewrite on top
    assert_eq!(
        after(&workspace, &edit, CHILD),
        THE_CHILD.replace("use crate::x::a::c::C;", "use crate::a::c::C;")
    );
}

/// A plan naming a child in `also` beside its parent gets the child carried at its nested
/// position — not flattened to the destination's root and not declared there.
#[test]
fn an_also_member_that_is_a_directory_child_of_another_member_is_carried_at_its_nested_position() {
    // Given a cluster of `x::a` and its own child `x::a::b`
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(
        vec![the_module_a(), a_member(&["x", "a", "b"], MODULE)],
        Reexport::Glob,
    );

    // When the cluster is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then `b` lands under `a/`, nothing lands at the root beside `a`, and only `a` is declared
    let landed: Vec<String> = renames(&edit).into_iter().map(|(_, to)| to).collect();
    assert_eq!(
        (
            landed.contains(&"crates/destination/src/a/b.rs".to_string()),
            landed.contains(&"crates/destination/src/b.rs".to_string()),
            after(&workspace, &edit, DESTINATION_LIB),
        ),
        (
            true,
            false,
            "//! The destination.\n\npub mod a;\n".to_string()
        )
    );
}

/// The moved parent's own `mod b;` and `pub use b::…` go on resolving where it lands, so the move
/// leaves them as written.
#[test]
fn the_moved_parents_mod_line_for_a_carried_child_is_left_as_written() {
    // Given a cluster of `x::a` and its own child `x::a::b`
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(
        vec![the_module_a(), a_member(&["x", "a", "b"], MODULE)],
        Reexport::Glob,
    );

    // When the cluster is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then `a`'s text is unchanged
    assert_eq!(after(&workspace, &edit, MODULE), THE_MODULE);
}

/// No file that arrives in the destination names the destination's own crate — the self re-export
/// `pub use destination::b;` that `#carve` R3, R4 and R8 deleted or rewrote by hand.
#[test]
fn no_file_arriving_in_the_destination_names_the_destination_crate() {
    for reexport in [Reexport::Glob, Reexport::None] {
        // Given a cluster of `x::a` and its own child `x::a::b`
        let workspace = a_workspace_whose_module_has_directory_children();
        let cluster = a_cluster_of(
            vec![the_module_a(), a_member(&["x", "a", "b"], MODULE)],
            reexport,
        );

        // When the cluster is resolved
        let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

        // Then no arriving file names `destination`
        let naming_itself: Vec<String> = arriving(&workspace, &edit)
            .into_iter()
            .filter(|(_, text)| text.contains("destination::"))
            .map(|(path, _)| path)
            .collect();
        assert_eq!(naming_itself, Vec::<String>::new(), "{reexport:?}");
    }
}

/// A nested module whose parent stays behind still lands at the destination's root under its last
/// segment, as before.
#[test]
fn a_nested_member_whose_parent_stays_behind_still_lands_at_the_destination_root() {
    // Given `top::leaf` moving without `top`
    let workspace = a_workspace_whose_module_has_directory_children();
    let cluster = a_cluster_of(
        vec![a_member(&["top", "leaf"], "crates/origin/src/top.rs")],
        Reexport::None,
    );

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then it lands at the root
    assert_eq!(
        renames(&edit),
        renamed(&[(
            "crates/origin/src/top/leaf.rs",
            "crates/destination/src/leaf.rs"
        )])
    );
}

/// A child the tree declares `pub(crate)`, reached from outside the tree through its own path,
/// would be private to the destination after the move (`E0603`): the move is refused, naming the
/// child and the file that reaches it.
#[test]
fn a_restricted_child_reached_from_outside_the_tree_is_refused_naming_the_child_and_the_caller() {
    // Given `runtime`, which stays, naming `C` through the `pub(crate)` child `c`
    let workspace = a_workspace_whose_module_has_directory_children().with(
        CALLER,
        "use crate::x::a::c::C;\n\npub fn boot() -> C {\n    C\n}\n",
    );
    let mut engine = nothing_reaches_the_tree().reaching("C", RESTRICTED_CHILD, CALLER, &workspace);
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let refusal = crate_move::resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
        .expect_err("a restricted child reached from outside is refused")
        .to_string();

    // Then the refusal names the child's declaration and the caller
    assert_eq!(
        (
            refusal.contains("pub(crate) mod c"),
            refusal.contains(CALLER),
        ),
        (true, true),
        "{refusal}"
    );
}

/// A carried child that reaches back into the crate the move leaves makes the destination depend
/// on it while the origin names the destination: the cycle refusal lists the child's path.
#[test]
fn a_carried_childs_path_back_into_the_origin_makes_the_move_refuse_as_a_cycle() {
    // Given the carried child `b` calling `runtime`, which stays and calls `b` back
    let workspace = a_workspace_whose_module_has_directory_children().with(
        CHILD,
        "pub struct Item;\n\npub fn tick() -> crate::runtime::Tick {\n    \
         crate::runtime::Tick\n}\n",
    );
    let workspace = workspace.with(
        CALLER,
        "use crate::x::a::b::Item;\n\npub struct Tick;\n\npub fn boot() -> Item {\n    Item\n}\n",
    );
    let mut engine = nothing_reaches_the_tree().reaching("Item", CHILD, CALLER, &workspace);
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let refusal = crate_move::resolve_cluster(&mut engine, &workspace.workspace(), &cluster)
        .expect_err("a cycle through a carried child is refused")
        .to_string();

    // Then the refusal names the path that makes it one
    assert!(refusal.contains("origin::runtime::Tick"), "{refusal}");
}

/// A struct update's `..crate::…` base is a path like any other: one into a module the move
/// carries along is re-pointed for where it lands — `#carve` R8's hand fix.
#[test]
fn a_struct_update_path_into_a_co_moving_module_is_re_pointed() {
    // Given `a`'s body building a value from `..crate::x::a::defaults()`
    let workspace = a_workspace_whose_module_has_directory_children().with(
        MODULE,
        "pub struct A {\n    pub id: u32,\n}\n\npub fn defaults() -> A {\n    A { id: 0 }\n}\n\n\
         pub fn first() -> A {\n    A {\n        id: 1,\n        ..crate::x::a::defaults()\n    }\n}\n",
    );
    let cluster = a_cluster_of(vec![the_module_a()], Reexport::None);

    // When the move is resolved
    let edit = resolving(&workspace, &mut nothing_reaches_the_tree(), &cluster);

    // Then the base names where `a` lands
    assert!(
        after(&workspace, &edit, MODULE).contains("        ..crate::a::defaults()\n"),
        "{}",
        after(&workspace, &edit, MODULE)
    );
}

/// `move_module_to_crate` is a cluster of one, so it carries the children too.
#[test]
fn a_single_module_move_carries_the_same_files_as_a_cluster_of_one() {
    // Given a plan moving `x::a` alone
    let workspace = a_workspace_whose_module_has_directory_children();
    let op = a_move_of(MODULE, "x::a");

    // When it is resolved
    let edit = crate_move::resolve(&mut nothing_reaches_the_tree(), &workspace.workspace(), &op)
        .expect("the move resolves");

    // Then the four child files are renamed with it
    let children_moved = renames(&edit)
        .into_iter()
        .filter(|(from, _)| from.starts_with("crates/origin/src/x/a/"))
        .count();
    assert_eq!(children_moved, 4);
}
