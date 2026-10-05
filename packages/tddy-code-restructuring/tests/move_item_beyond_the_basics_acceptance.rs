//! `move_item` where the moved code carries more than a function body: names it imported, callers
//! that import it through a group, an inline destination, and a test module that reaches it
//! through `use super::*`.
//!
//! Each of these compiles before the move and has to compile — with its tests, and under clippy —
//! after it, which is what an edit that merely looks right cannot satisfy.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{applying_a_plan_of, assert_compiles_with_its_tests, assert_lints_clean};
use same_crate::{a_move_item_op, an_app_holding, moving_items, the_anchor_over};

const AN_EMPTY_ANSWERS_MODULE: &str = "//! What a peer answered.\n";

/// AC1 — what the moved code imported follows it, and what the source no longer needs is dropped.
#[tokio::test(flavor = "multi_thread")]
async fn carries_the_imports_the_moved_code_needs_and_leaves_none_unused_behind() {
    // Given a function that uses a trait and a type its file imports, and a caller that imports it
    // through a group beside another name
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod answers;\npub mod audit;\npub mod handler;\npub mod pairing;\n",
        ),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        ("src/audit.rs", "pub struct Level(pub u32);\n"),
        (
            "src/pairing.rs",
            concat!(
                "use std::fmt::Write;\n",
                "\n",
                "use crate::audit::Level;\n",
                "\n",
                "pub fn render(level: Level) -> String {\n",
                "    let mut text = String::new();\n",
                "    write!(text, \"{}\", level.0).unwrap();\n",
                "    text\n",
                "}\n",
                "\n",
                "pub fn unrelated() -> u32 {\n",
                "    7\n",
                "}\n",
            ),
        ),
        (
            "src/handler.rs",
            concat!(
                "use crate::audit::Level;\n",
                "use crate::pairing::{render, unrelated};\n",
                "\n",
                "pub fn handle() -> String {\n",
                "    format!(\"{}{}\", render(Level(1)), unrelated())\n",
                "}\n",
            ),
        ),
    ]);

    // When it moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["render"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the tree compiles and is clean under clippy, the group lost the name, and `pairing`
    // keeps no import only the moved code read
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
    let handler = workspace.read("src/handler.rs");
    assert!(
        handler.contains("use crate::answers::render;") && handler.contains("pairing::"),
        "the moved name was not taken out of the group into an import of its own:\n{handler}"
    );
    assert!(
        !handler.contains("pairing::{render"),
        "the group still names the moved item:\n{handler}"
    );
    assert!(
        !workspace.read("src/pairing.rs").contains("fmt::Write"),
        "an import only the moved code used was left in `pairing`:\n{}",
        workspace.read("src/pairing.rs")
    );
}

/// AC2 — an inline module is a destination as much as a file is.
#[tokio::test(flavor = "multi_thread")]
async fn moves_items_into_an_inline_module() {
    // Given an inline `answers` in the crate root
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod pairing;\n\npub mod answers {\n    pub fn existing() -> u32 {\n        1\n    }\n}\n"),
        (
            "src/pairing.rs",
            "pub fn moved(code: u32) -> bool {\n    code == 404\n}\n\npub fn unrelated() -> u32 {\n    7\n}\n",
        ),
    ]);

    // When a function moves into it
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["moved"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then it is declared inside the inline module, and the tree compiles
    let lib = workspace.read("src/lib.rs");
    let inside = lib.split("pub mod answers {").nth(1).unwrap_or_default();
    assert!(
        inside.contains("fn moved(code: u32) -> bool"),
        "the function is not inside the inline module:\n{lib}"
    );
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

/// AC3 — a bare name that a glob import resolved is imported where it is used.
#[tokio::test(flavor = "multi_thread")]
async fn imports_a_moved_name_where_the_source_and_its_test_module_use_it_bare() {
    // Given a function used bare by its own file and by a test module that imports `super::*`
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod answers;\npub mod pairing;\n"),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        (
            "src/pairing.rs",
            concat!(
                "pub fn missing(code: u32) -> bool {\n",
                "    code == 404\n",
                "}\n",
                "\n",
                "pub fn answer(code: u32) -> &'static str {\n",
                "    if missing(code) {\n",
                "        \"gone\"\n",
                "    } else {\n",
                "        \"here\"\n",
                "    }\n",
                "}\n",
                "\n",
                "#[cfg(test)]\n",
                "mod tests {\n",
                "    use super::*;\n",
                "\n",
                "    #[test]\n",
                "    fn a_404_is_missing() {\n",
                "        assert!(missing(404));\n",
                "    }\n",
                "}\n",
            ),
        ),
    ]);

    // When `missing` moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["missing"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then both uses still resolve, with the tests built and clippy satisfied
    assert_compiles_with_its_tests(&workspace);
    assert_lints_clean(&workspace);
}

/// AC4 — a single `item` anchor on a name moves the whole item, and a named facade leaves the callers be.
#[tokio::test(flavor = "multi_thread")]
async fn moves_one_item_named_by_an_item_anchor_behind_a_named_facade() {
    // Given a predicate, its caller, and an `item` anchor that names it without a range
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod answers;\npub mod handler;\npub mod pairing;\n"),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        (
            "src/pairing.rs",
            "/// Whether a peer has no such session.\npub fn missing(code: u32) -> bool {\n    code == 404\n}\n",
        ),
        (
            "src/handler.rs",
            "use crate::pairing::missing;\n\npub fn handle(code: u32) -> bool {\n    missing(code)\n}\n",
        ),
    ]);
    let items = the_anchor_over(&workspace, "src/pairing.rs", &["missing"]).await;
    let one_item: tddy_code_restructuring::Anchor = {
        let mut anchor = serde_json::to_value(&items).expect("the anchor serializes");
        anchor["kind"] = serde_json::json!("item");
        anchor["item"] = anchor["items"][0].clone();
        anchor["fingerprint"] = anchor["fingerprints"][0].clone();
        let object = anchor.as_object_mut().expect("an anchor is an object");
        object.remove("items");
        object.remove("fingerprints");
        serde_json::from_value(anchor).expect("an `item` anchor")
    };

    // When it moves into `answers` with a named facade
    applying_a_plan_of(
        &workspace,
        &[a_move_item_op(&one_item, "app::answers", Some("named"))],
    )
    .await
    .expect("the move applies");

    // Then the item, doc comment included, is in `answers`, `pairing` re-exports it, and the caller is as it was
    assert!(
        workspace
            .read("src/answers.rs")
            .contains("/// Whether a peer has no such session.\npub fn missing"),
        "the item did not arrive whole:\n{}",
        workspace.read("src/answers.rs")
    );
    assert!(
        workspace
            .read("src/pairing.rs")
            .contains("pub use crate::answers::missing;"),
        "no named facade was left:\n{}",
        workspace.read("src/pairing.rs")
    );
    assert!(
        workspace
            .read("src/handler.rs")
            .starts_with("use crate::pairing::missing;"),
        "a caller was edited although a facade stands in for the old path"
    );
    assert_compiles_with_its_tests(&workspace);
}
