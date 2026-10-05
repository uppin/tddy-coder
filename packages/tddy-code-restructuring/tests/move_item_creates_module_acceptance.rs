//! `move_item` can create the module it moves into, when the plan line says what to call it.
//!
//! A module that did not exist yet could only be made by `extract_module`, which cannot re-point the
//! callers of what it extracts (it refuses, or leaves a facade only a hand edit can remove), so a
//! topic module gathered from items in several files could not be built from the operations the
//! engine has. With `name` on a `move_item` line, `to` is the **parent** and a new, empty module
//! called `name` is declared in it before the items move in — the same meaning `name` has on
//! `extract_module`, the new module's name.
//!
//! It is explicit on purpose: a mistyped destination in `to` alone still refuses as a module that
//! does not exist, so a typo cannot silently grow a new file.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{assert_compiles, AFixtureWorkspace};
use same_crate::{
    a_move_item_into_a_new_module_op, an_app_holding, moving_items_into_a_new_module,
    the_anchor_over, what_a_static_check_finds_in,
};

const LIB: &str = "pub mod handler;\npub mod other;\npub mod pairing;\n";
const PAIRING: &str = "pub fn peer_has_no_such_session(code: u32) -> bool {\n    code == 404\n}\n";
const OTHER: &str = "pub fn split_pairing(code: u32) -> u32 {\n    code / 2\n}\n";
const HANDLER: &str = concat!(
    "use crate::pairing::peer_has_no_such_session;\n",
    "\n",
    "pub fn handle(code: u32) -> bool {\n",
    "    peer_has_no_such_session(code)\n",
    "}\n",
);

fn a_crate_with_two_items_in_two_files() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/pairing.rs", PAIRING),
        ("src/other.rs", OTHER),
        ("src/handler.rs", HANDLER),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn creates_the_module_under_the_crate_root_and_moves_the_item_into_it() {
    // Given an item, and no module called `answers`
    let workspace = a_crate_with_two_items_in_two_files();

    // When it is moved into a new module `answers` under the crate root
    moving_items_into_a_new_module(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app",
        "answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the module exists, is declared by the root, holds the item, and the caller follows
    assert!(
        workspace.holds("src/answers.rs"),
        "the new module's file was not created"
    );
    assert!(
        workspace.read("src/lib.rs").contains("mod answers;"),
        "the parent does not declare the new module:\n{}",
        workspace.read("src/lib.rs")
    );
    assert!(
        workspace
            .read("src/answers.rs")
            .contains("fn peer_has_no_such_session"),
        "the item did not arrive in the new module"
    );
    assert!(
        workspace
            .read("src/handler.rs")
            .contains("answers::peer_has_no_such_session"),
        "the caller still names the old module:\n{}",
        workspace.read("src/handler.rs")
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn creates_the_module_beside_the_files_of_a_parent_that_is_a_plain_file() {
    // Given a parent `host` declared by `src/host.rs`
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod handler;\npub mod host;\npub mod pairing;\n",
        ),
        (
            "src/host.rs",
            "pub fn host_name() -> &'static str {\n    \"host\"\n}\n",
        ),
        ("src/pairing.rs", PAIRING),
        ("src/handler.rs", HANDLER),
    ]);

    // When the item moves into a new module `answers` under `host`
    moving_items_into_a_new_module(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::host",
        "answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the new file sits where `host` would look for a child, and `host` declares it
    assert!(workspace.holds("src/host/answers.rs"));
    assert!(workspace.read("src/host.rs").contains("mod answers;"));
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn creates_the_module_inside_a_parent_that_is_a_mod_rs() {
    // Given a parent `host` declared by `src/host/mod.rs`
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod handler;\npub mod host;\npub mod pairing;\n",
        ),
        (
            "src/host/mod.rs",
            "pub fn host_name() -> &'static str {\n    \"host\"\n}\n",
        ),
        ("src/pairing.rs", PAIRING),
        ("src/handler.rs", HANDLER),
    ]);

    // When the item moves into a new module `answers` under `host`
    moving_items_into_a_new_module(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::host",
        "answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the `mod.rs` declares it and its file sits beside it
    assert!(workspace.holds("src/host/answers.rs"));
    assert!(workspace.read("src/host/mod.rs").contains("mod answers;"));
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn gathers_items_from_two_files_into_the_one_module_it_created() {
    // Given an item in `pairing.rs` and another in `other.rs`
    let workspace = a_crate_with_two_items_in_two_files();
    let first = the_anchor_over(&workspace, "src/pairing.rs", &["peer_has_no_such_session"]).await;
    let second = the_anchor_over(&workspace, "src/other.rs", &["split_pairing"]).await;

    // When the first creates `answers` and the second moves into it, in one plan
    harness::applying_a_plan_of(
        &workspace,
        &[
            a_move_item_into_a_new_module_op(&first, "app", "answers", None),
            same_crate::a_move_item_op(&second, "app::answers", None),
        ],
    )
    .await
    .expect("the plan applies");

    // Then both are in the one module, and neither file keeps its item
    let gathered = workspace.read("src/answers.rs");
    assert!(
        gathered.contains("fn peer_has_no_such_session") && gathered.contains("fn split_pairing"),
        "the module did not gather both items:\n{gathered}"
    );
    assert!(!workspace
        .read("src/pairing.rs")
        .contains("fn peer_has_no_such_session"));
    assert!(!workspace.read("src/other.rs").contains("fn split_pairing"));
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_name_the_parent_already_declares() {
    // Given a parent that already declares a module called `pairing`
    let workspace = a_crate_with_two_items_in_two_files();
    let anchor = the_anchor_over(&workspace, "src/other.rs", &["split_pairing"]).await;

    // When a plan asks to create a module of that name under it
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_move_item_into_a_new_module_op(&anchor, "app", "pairing", None),
    )
    .await;

    // Then it is refused, saying the name is taken
    let said = findings.join("\n");
    assert!(
        said.contains("pairing") && said.contains("already"),
        "the refusal did not name the clash: {said}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_parent_that_does_not_exist() {
    // Given a parent path that names nothing
    let workspace = a_crate_with_two_items_in_two_files();
    let anchor = the_anchor_over(&workspace, "src/other.rs", &["split_pairing"]).await;

    // When a plan asks to create a module under it
    let findings = what_a_static_check_finds_in(
        &workspace,
        a_move_item_into_a_new_module_op(&anchor, "app::nowhere", "answers", None),
    )
    .await;

    // Then it says which parent is missing
    let said = findings.join("\n");
    assert!(
        said.contains("app::nowhere") && said.contains("does not exist"),
        "the refusal did not name the missing parent: {said}"
    );
}
