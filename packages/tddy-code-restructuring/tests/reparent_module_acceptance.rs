//! `reparent_module` moves a module under a different parent of the **same crate**, against a live
//! rust-analyzer.
//!
//! `extract_module` writes a new module as a child of the file it cut from, and
//! `move_module_to_crate` moves between crates, so a module that sits under the wrong parent had
//! only a hand `git mv` — plus `mod` and `use` edits in every file that named it — with none of the
//! engine's guarantees. A module move takes its children with it, which is why a module under the
//! wrong parent travels with the wrong topic when that parent moves.
//!
//! The anchor is the module's `mod` declaration in its old parent, the same `items` anchor
//! `move_item` takes, so the two operations are written the same way.
//!
//! `cargo check` is the assertion no edit that merely looks right can satisfy: a parent left
//! declaring a module that is no longer beside it, or a `super::` path that now means something else,
//! fails it.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{assert_compiles, AFixtureWorkspace};
use same_crate::{
    a_reparent_module_op, an_app_holding, reparenting_module, the_anchor_over,
    what_a_static_check_finds_in,
};

const LIB: &str = "pub mod audit;\npub mod host;\npub mod split;\n";
const AN_ATTACHMENTS_MODULE: &str = "pub fn materialize() -> u32 {\n    1\n}\n";
const A_SPLIT_THAT_IMPORTS_IT: &str = concat!(
    "use crate::host::attachments::materialize;\n",
    "\n",
    "pub fn start() -> u32 {\n",
    "    materialize()\n",
    "}\n",
);
const AN_AUDIT_THAT_NAMES_IT_INLINE: &str = concat!(
    "pub fn audit() -> u32 {\n",
    "    crate::host::attachments::materialize()\n",
    "}\n",
);
const THE_HOST_ITSELF: &str = "pub fn host_name() -> &'static str {\n    \"host\"\n}\n";

/// The file a parent module named `name` lives in, in either of the two forms Rust 2018 allows.
fn the_file_of(name: &str, as_mod_rs: bool) -> String {
    if as_mod_rs {
        format!("src/{name}/mod.rs")
    } else {
        format!("src/{name}.rs")
    }
}

/// `host` declares `attachments`, which `split` and `audit` reach; `split` is where it will move.
///
/// The two flags choose the form each parent takes, since an operation that handled only one would
/// refuse half the subsystems in a real workspace.
fn a_crate_whose_module_sits_under(
    host_as_mod_rs: bool,
    split_as_mod_rs: bool,
) -> AFixtureWorkspace {
    let host = the_file_of("host", host_as_mod_rs);
    let split = the_file_of("split", split_as_mod_rs);
    an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            &host,
            &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}"),
        ),
        ("src/host/attachments.rs", AN_ATTACHMENTS_MODULE),
        (&split, A_SPLIT_THAT_IMPORTS_IT),
        ("src/audit.rs", AN_AUDIT_THAT_NAMES_IT_INLINE),
    ])
}

/// The default shape: both parents are `<name>.rs` files.
fn a_crate_whose_module_sits_under_the_wrong_parent() -> AFixtureWorkspace {
    a_crate_whose_module_sits_under(false, false)
}

/// AC1 — the module's file arrives under its new parent, and the old parent stops declaring it.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_file_under_another_parent_and_the_tree_compiles() {
    // Given `attachments` declared by `host`
    let workspace = a_crate_whose_module_sits_under_the_wrong_parent();

    // When it is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then the file is where `split` would look for it, and the old parent lets go of it
    assert!(
        workspace.holds("src/split/attachments.rs"),
        "the module file did not arrive under its new parent"
    );
    assert!(
        !workspace.holds("src/host/attachments.rs"),
        "the module file did not leave its old parent"
    );
    assert!(
        !workspace.read("src/host.rs").contains("mod attachments"),
        "the old parent still declares a module that is no longer beside it"
    );
    assert!(
        workspace.read("src/split.rs").contains("mod attachments;"),
        "the new parent does not declare the module:\n{}",
        workspace.read("src/split.rs")
    );
    assert_compiles(&workspace);
}

/// AC2 — the old parent is a `mod.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_out_of_a_parent_that_is_a_mod_rs() {
    // Given `host` as `src/host/mod.rs`
    let workspace = a_crate_whose_module_sits_under(true, false);

    // When `attachments` is re-parented under `split`
    reparenting_module(
        &workspace,
        "src/host/mod.rs",
        "attachments",
        "app::split",
        None,
    )
    .await
    .expect("the re-parent applies");

    // Then the `mod.rs` form is no obstacle on the way out
    assert!(
        !workspace
            .read("src/host/mod.rs")
            .contains("mod attachments"),
        "the mod.rs parent still declares the module"
    );
    assert!(workspace.holds("src/split/attachments.rs"));
    assert_compiles(&workspace);
}

/// AC3 — the new parent is a `mod.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_into_a_parent_that_is_a_mod_rs() {
    // Given `split` as `src/split/mod.rs`
    let workspace = a_crate_whose_module_sits_under(false, true);

    // When `attachments` is re-parented under it
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then the module is declared by the `mod.rs` and its file sits beside it
    assert!(
        workspace
            .read("src/split/mod.rs")
            .contains("mod attachments;"),
        "the mod.rs parent does not declare the module"
    );
    assert!(workspace.holds("src/split/attachments.rs"));
    assert_compiles(&workspace);
}

/// AC4 — a module with children of its own takes its directory with it.
///
/// A re-parent that moved `attachments.rs` and left `attachments/` behind would leave the child
/// declared by a file that is no longer beside it.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_directory_shaped_module_with_its_children() {
    // Given `attachments` declaring a child `staging`
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/host.rs", &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}")),
        (
            "src/host/attachments.rs",
            "pub mod staging;\n\npub fn materialize() -> u32 {\n    staging::stage()\n}\n",
        ),
        (
            "src/host/attachments/staging.rs",
            "pub fn stage() -> u32 {\n    2\n}\n",
        ),
        ("src/split.rs", "pub fn start() -> u32 {\n    0\n}\n"),
        ("src/audit.rs", "pub fn audit() -> u32 {\n    0\n}\n"),
    ]);

    // When `attachments` is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then the child came along, and nothing is left in the old directory
    assert!(
        workspace.holds("src/split/attachments/staging.rs"),
        "the child module did not move with its parent"
    );
    assert!(
        !workspace.holds("src/host/attachments/staging.rs"),
        "the child module was left behind"
    );
    assert_compiles(&workspace);
}

/// AC5 — a `super::` path means the module's *parent*, and the parent just changed.
#[tokio::test(flavor = "multi_thread")]
async fn rebases_a_super_path_that_named_the_old_parent() {
    // Given `attachments` reaching its parent's `host_name` through `super`
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/host.rs", &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}")),
        (
            "src/host/attachments.rs",
            "use super::host_name;\n\npub fn materialize() -> u32 {\n    host_name().len() as u32\n}\n",
        ),
        ("src/split.rs", "pub fn start() -> u32 {\n    0\n}\n"),
        ("src/audit.rs", "pub fn audit() -> u32 {\n    0\n}\n"),
    ]);

    // When it is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then the path still reaches `host`'s function and not `split`'s (which has none)
    assert!(
        !workspace
            .read("src/split/attachments.rs")
            .contains("use super::host_name;"),
        "a `super::` path was left meaning the new parent:\n{}",
        workspace.read("src/split/attachments.rs")
    );
    assert_compiles(&workspace);
}

/// AC6 — every path that named the module is re-pointed, whatever form it took.
#[tokio::test(flavor = "multi_thread")]
async fn re_points_a_use_import_and_an_inline_qualified_path() {
    // Given one caller that imports the module's function and one that names it inline
    let workspace = a_crate_whose_module_sits_under_the_wrong_parent();

    // When the module is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then neither caller names the old parent
    assert!(
        workspace
            .read("src/split.rs")
            .contains("split::attachments::materialize")
            || workspace
                .read("src/split.rs")
                .contains("self::attachments::materialize"),
        "the importing caller still points at `host`:\n{}",
        workspace.read("src/split.rs")
    );
    assert!(
        workspace
            .read("src/audit.rs")
            .contains("split::attachments::materialize"),
        "the inline-qualified caller still points at `host`:\n{}",
        workspace.read("src/audit.rs")
    );
}

/// AC7 — with a facade, no caller changes at all.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_glob_facade_that_keeps_every_caller_unchanged() {
    // Given the module and its two callers
    let workspace = a_crate_whose_module_sits_under_the_wrong_parent();

    // When it is re-parented with `reexport: glob`
    reparenting_module(
        &workspace,
        "src/host.rs",
        "attachments",
        "app::split",
        Some("glob"),
    )
    .await
    .expect("the re-parent applies");

    // Then the old parent keeps a `pub use`, the callers are what they were, and it compiles
    assert!(
        workspace.read("src/host.rs").contains("pub use"),
        "no facade was left in the old parent:\n{}",
        workspace.read("src/host.rs")
    );
    assert_eq!(workspace.read("src/split.rs"), A_SPLIT_THAT_IMPORTS_IT);
    assert_eq!(workspace.read("src/audit.rs"), AN_AUDIT_THAT_NAMES_IT_INLINE);
    assert_compiles(&workspace);
}

/// AC8 — the declaration keeps the visibility and the attribute its author gave it.
#[tokio::test(flavor = "multi_thread")]
async fn carries_the_declarations_visibility_and_attribute_to_the_new_parent() {
    // Given a declaration with an attribute and a restricted visibility
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/host.rs",
            &format!("#[allow(dead_code)]\npub(crate) mod attachments;\n\n{THE_HOST_ITSELF}"),
        ),
        ("src/host/attachments.rs", AN_ATTACHMENTS_MODULE),
        ("src/split.rs", "pub fn start() -> u32 {\n    0\n}\n"),
        ("src/audit.rs", "pub fn audit() -> u32 {\n    0\n}\n"),
    ]);

    // When the module is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then `split` declares it the same way
    let declared = workspace.read("src/split.rs");
    assert!(
        declared.contains("#[allow(dead_code)]") && declared.contains("pub(crate) mod attachments;"),
        "the declaration's attribute or visibility was lost:\n{declared}"
    );
    assert_compiles(&workspace);
}

/// AC9 — a destination that does not exist is refused before a server exists.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_destination_that_does_not_exist() {
    // Given the module and a parent path that names nothing
    let workspace = a_crate_whose_module_sits_under_the_wrong_parent();
    let anchor = the_anchor_over(&workspace, "src/host.rs", &["attachments"]).await;

    // When a static check reads the plan
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_reparent_module_op(&anchor, "app::nowhere", None),
    )
    .await;

    // Then it says which parent is missing
    let said = findings.join("\n");
    assert!(
        said.contains("app::nowhere") && said.contains("does not exist"),
        "the refusal did not name the missing parent: {said}"
    );
}

/// AC10 — a name the new parent already declares is refused, and says so.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_module_name_the_new_parent_already_declares() {
    // Given `split` already declaring an `attachments` of its own
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/host.rs", &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}")),
        ("src/host/attachments.rs", AN_ATTACHMENTS_MODULE),
        ("src/split.rs", "pub mod attachments;\n"),
        ("src/split/attachments.rs", AN_ATTACHMENTS_MODULE),
        ("src/audit.rs", "pub fn audit() -> u32 {\n    0\n}\n"),
    ]);
    let anchor = the_anchor_over(&workspace, "src/host.rs", &["attachments"]).await;

    // When a static check reads the plan
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_reparent_module_op(&anchor, "app::split", None),
    )
    .await;

    // Then the clash is named
    let said = findings.join("\n");
    assert!(
        said.contains("attachments") && said.contains("already"),
        "the refusal did not name the clash: {said}"
    );
}

/// AC11 — a module cannot become its own descendant.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_destination_inside_the_module_being_moved() {
    // Given `attachments` with a child `staging`
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/host.rs", &format!("pub mod attachments;\n\n{THE_HOST_ITSELF}")),
        (
            "src/host/attachments.rs",
            "pub mod staging;\n\npub fn materialize() -> u32 {\n    0\n}\n",
        ),
        (
            "src/host/attachments/staging.rs",
            "pub fn stage() -> u32 {\n    2\n}\n",
        ),
        ("src/split.rs", "pub fn start() -> u32 {\n    0\n}\n"),
        ("src/audit.rs", "pub fn audit() -> u32 {\n    0\n}\n"),
    ]);
    let anchor = the_anchor_over(&workspace, "src/host.rs", &["attachments"]).await;

    // When the plan names its own child as the new parent
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_reparent_module_op(&anchor, "app::host::attachments::staging", None),
    )
    .await;

    // Then it is refused as a move into itself
    let said = findings.join("\n");
    assert!(
        said.contains("inside"),
        "the refusal did not say the destination is inside the module: {said}"
    );
}

/// AC12 — a module placed with `#[path]` is refused, naming the attribute.
///
/// Its file is wherever the attribute says, so "move the file beside the new parent" has no meaning
/// the operation could honour without rewriting the attribute too.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_module_placed_with_a_path_attribute() {
    // Given `attachments` declared with `#[path]`
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/host.rs",
            &format!(
                "#[path = \"elsewhere/attachments_impl.rs\"]\npub mod attachments;\n\n{THE_HOST_ITSELF}"
            ),
        ),
        ("src/host/elsewhere/attachments_impl.rs", AN_ATTACHMENTS_MODULE),
        ("src/split.rs", "pub fn start() -> u32 {\n    0\n}\n"),
        ("src/audit.rs", "pub fn audit() -> u32 {\n    0\n}\n"),
    ]);
    let anchor = the_anchor_over(&workspace, "src/host.rs", &["attachments"]).await;

    // When a static check reads the plan
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_reparent_module_op(&anchor, "app::split", None),
    )
    .await;

    // Then the attribute is named
    let said = findings.join("\n");
    assert!(
        said.contains("path"),
        "the refusal did not name the `#[path]` attribute: {said}"
    );
}
