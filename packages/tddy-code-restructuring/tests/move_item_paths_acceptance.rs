//! `move_item` and `reparent_module` read a `use` the way rustc resolves it.
//!
//! Found on `#carve` 21/21: a moved block wrote `super::service_util::find_registered_project(…)`, its
//! parent held `pub use tddy_session_split::{…, service_util, …};`, and the move respelled the path
//! as `super::tddy_session_split::service_util::…` — the extern crate read as a child module (`E0433`).
//! A relative path through a module's import is kept when it still resolves from the destination,
//! and followed one hop to what the import brings in otherwise — through an alias, a glob or another
//! crate — or refused when a glob cannot say. A destination's renamed import of the item that moves
//! into it keeps its name.
//!
//! `cargo check` is the oracle: every wrong spelling here is a compile error, not a style choice.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{assert_compiles, assert_compiles_with_its_tests, checking_the_plan};
use same_crate::{
    a_move_item_op, an_app_holding, an_app_over_a_kernel, moving_items,
    moving_items_into_a_new_module, reparenting_module, the_anchor_over,
};

/// `kernel`'s one module: `util::answer`.
const KERNEL_LIB: &str = "pub mod util {\n    pub fn answer() -> u32 {\n        42\n    }\n}\n";

/// A function written below `host` that reaches `kernel::util` through `host`'s import of it.
const AN_OBSERVER: &str = "pub fn describe() -> u32 {\n    super::util::answer()\n}\n";

/// An `app` over `kernel` whose `host` module reads `host_text` and declares `observer`, beside an
/// empty `split`.
fn an_app_whose_host_imports_the_kernel_as(host_text: &str) -> harness::AFixtureWorkspace {
    an_app_over_a_kernel(
        &[
            ("src/lib.rs", "pub mod host;\npub mod split;\n"),
            ("src/host.rs", host_text),
            ("src/host/observer.rs", AN_OBSERVER),
            ("src/split.rs", "pub fn marker() {}\n"),
        ],
        &[("src/lib.rs", KERNEL_LIB)],
    )
}

/// A `host` that imports `kernel::util` privately and uses it itself.
const A_HOST_PRIVATELY_IMPORTING_THE_KERNEL: &str = concat!(
    "use kernel::util;\n",
    "\n",
    "pub mod observer;\n",
    "\n",
    "pub fn make() -> u32 {\n",
    "    util::answer()\n",
    "}\n",
);

/// An `app` whose `host` reads `host_text` above `pub mod worker;`, `host/worker.rs` reading
/// `worker_text`, with `types::Config` defined, `split` empty, and `a` re-exporting `b`'s `Config`.
fn an_app_whose_host_reads(host_text: &str, worker_text: &str) -> harness::AFixtureWorkspace {
    an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\npub mod host;\npub mod split;\npub mod types;\n",
        ),
        ("src/a.rs", "pub use crate::b::*;\n"),
        ("src/b.rs", "pub struct Elsewhere(pub u32);\n"),
        ("src/types.rs", "pub struct Config(pub u32);\n"),
        ("src/host.rs", &format!("{host_text}\npub mod worker;\n")),
        ("src/host/worker.rs", worker_text),
        ("src/split.rs", "pub fn marker() {}\n"),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_into_a_sibling_keeps_a_path_through_the_parents_facade_of_another_crate() {
    // Given a `host` that re-exports `kernel::util` and a child that reaches it through `super::`
    let workspace =
        an_app_whose_host_imports_the_kernel_as("pub use kernel::util;\n\npub mod observer;\n");

    // When the child's function moves into a new sibling module `host::wiring`
    moving_items_into_a_new_module(
        &workspace,
        "app/src/host/observer.rs",
        &["describe"],
        "app::host",
        "wiring",
        None,
    )
    .await
    .expect("the move applies");

    // Then the path is the one written, never `super::kernel::`, and the tree compiles
    let wiring = workspace.read("app/src/host/wiring.rs");
    assert!(
        wiring.contains("    super::util::answer()\n"),
        "the path through the facade was respelled:\n{wiring}"
    );
    assert!(
        !wiring.contains("super::kernel::"),
        "the facade's crate was spelled as a child module:\n{wiring}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_out_of_a_module_that_privately_imports_another_crate_writes_the_crates_path() {
    // Given a `host` that imports `kernel::util` privately, and a child that reaches it through it
    let workspace = an_app_whose_host_imports_the_kernel_as(A_HOST_PRIVATELY_IMPORTING_THE_KERNEL);

    // When the child's function moves to `split`, outside `host`
    moving_items(
        &workspace,
        "app/src/host/observer.rs",
        &["describe"],
        "app::split",
        None,
    )
    .await
    .expect("the move applies");

    // Then the path names the crate, not `host`'s private import of it, and the tree compiles
    let split = workspace.read("app/src/split.rs");
    assert!(
        split.contains("\n    kernel::util::answer()\n"),
        "the path does not name the crate:\n{split}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reparented_module_names_another_crate_its_old_parent_imported_by_the_crates_path() {
    // Given the same `host`, whose child module `observer` reaches `kernel::util` through it
    let workspace = an_app_whose_host_imports_the_kernel_as(A_HOST_PRIVATELY_IMPORTING_THE_KERNEL);

    // When `observer` is re-parented under `split`
    reparenting_module(
        &workspace,
        "app/src/host.rs",
        "observer",
        "app::split",
        None,
    )
    .await
    .expect("the move applies");

    // Then its path names the crate, and the tree compiles
    let observer = workspace.read("app/src/split/observer.rs");
    assert!(
        observer.contains("\n    kernel::util::answer()\n"),
        "the path does not name the crate:\n{observer}"
    );
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_through_the_old_modules_aliased_import_names_the_item_it_brings_in() {
    // Given a `host` that imports `Config` as `Settings`, and a child that names `super::Settings`
    let workspace = an_app_whose_host_reads(
        "use crate::types::Config as Settings;\n\npub fn make() -> Settings {\n    Settings(1)\n}\n",
        "pub fn describe(settings: &super::Settings) -> u32 {\n    settings.0\n}\n",
    );

    // When the child's function moves to `split`
    moving_items(
        &workspace,
        "src/host/worker.rs",
        &["describe"],
        "app::split",
        None,
    )
    .await
    .expect("the move applies");

    // Then the path names `Config` where it is defined, and the tree compiles
    let split = workspace.read("src/split.rs");
    assert!(
        split.contains("&super::types::Config"),
        "the path still names `host`'s private alias:\n{split}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_through_the_old_modules_glob_import_names_the_module_that_holds_the_item() {
    // Given a `host` that globs `types`, and a child that names `super::Config`
    let workspace = an_app_whose_host_reads(
        "use crate::types::*;\n\npub fn make() -> Config {\n    Config(1)\n}\n",
        "pub fn describe(config: &super::Config) -> u32 {\n    config.0\n}\n",
    );

    // When the child's function moves to `split`
    moving_items(
        &workspace,
        "src/host/worker.rs",
        &["describe"],
        "app::split",
        None,
    )
    .await
    .expect("the move applies");

    // Then the path names the module the glob brings `Config` from, and the tree compiles
    let split = workspace.read("src/split.rs");
    assert!(
        split.contains("&super::types::Config"),
        "the path still names `host`'s private glob import:\n{split}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_through_a_glob_that_cannot_confirm_the_name_is_refused_by_check_deep_and_by_apply_and_nothing_is_written(
) {
    // Given a `host` that globs `a`, which only globs `b` in turn, and a child naming
    // `super::Elsewhere`
    let workspace = an_app_whose_host_reads(
        "use crate::a::*;\n\npub fn make() -> Elsewhere {\n    Elsewhere(1)\n}\n",
        "pub fn describe(found: &super::Elsewhere) -> u32 {\n    found.0\n}\n",
    );
    let files = ["src/host.rs", "src/host/worker.rs", "src/split.rs"];
    let before: Vec<String> = files.iter().map(|file| workspace.read(file)).collect();
    let anchor = the_anchor_over(&workspace, "src/host/worker.rs", &["describe"]).await;
    let plan = workspace.a_plan_of(&[a_move_item_op(&anchor, "app::split", None)]);

    // When the move is checked deeply, and then applied
    let findings = checking_the_plan(&workspace, plan, true)
        .await
        .expect("a deep check runs");
    let refusal = moving_items(
        &workspace,
        "src/host/worker.rs",
        &["describe"],
        "app::split",
        None,
    )
    .await
    .expect_err("the move refuses");

    // Then both name the file, the line and the path as written, and every file is untouched
    let names_it =
        |said: &str| said.contains("src/host/worker.rs:1") && said.contains("`super::Elsewhere`");
    assert!(
        findings.iter().any(|finding| names_it(finding)),
        "the deep check does not name the path: {findings:?}"
    );
    assert!(names_it(&refusal), "unexpected refusal: {refusal}");
    let after: Vec<String> = files.iter().map(|file| workspace.read(file)).collect();
    assert_eq!(after, before, "a refused move wrote something");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_destination_that_imports_the_moving_item_under_an_alias_keeps_the_alias_bound() {
    // Given a destination that calls the moving item through a renamed import of it
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod answers;\npub mod pairing;\n"),
        (
            "src/pairing.rs",
            "pub fn peer(code: u32) -> bool {\n    code == 404\n}\n",
        ),
        (
            "src/answers.rs",
            "use crate::pairing::peer as check;\n\npub fn answer(code: u32) -> bool {\n    check(code)\n}\n",
        ),
    ]);

    // When the item moves into that destination
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the item is defined there once, the alias still binds it, and the tree compiles
    let answers = workspace.read("src/answers.rs");
    assert_eq!(
        answers.matches("pub fn peer(").count(),
        1,
        "the item is not defined exactly once:\n{answers}"
    );
    assert!(
        answers.contains("use self::peer as check;"),
        "the renamed import was not kept:\n{answers}"
    );
    assert_compiles(&workspace);
}
