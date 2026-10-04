//! `move_item` moves items into another module of the **same crate**, against a live rust-analyzer.
//!
//! Until now a Rust item could leave its file in two ways: grouped into a module of the file it
//! already sat in (`extract_module`), or out of the crate with its whole module
//! (`move_module_to_crate`). A crate split by topic stops at every item that sits in the wrong
//! file, and the engine's guarantees (the compile gate, comments kept, callers re-pointed from the
//! server's own reference set) did not reach the hand edit that followed.
//!
//! `cargo check` is the assertion no edit that merely looks right can satisfy: a caller left
//! pointing at the old path, or a moved item nothing can see, fails it.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{
    assert_compiles, assert_compiles_with_its_tests, assert_lints_clean, at, refusal_from,
    the_anchor_command_emits, AFixtureWorkspace,
};
use same_crate::{
    a_move_item_op, an_app_holding, moving_items, the_anchor_over, what_a_static_check_finds_in,
};
use tddy_code_restructuring::Range;

const LIB: &str = "pub mod answers;\npub mod audit;\npub mod handler;\npub mod pairing;\n";
const AN_EMPTY_ANSWERS_MODULE: &str = "//! What a peer answered about a session.\n";

const PAIRING_WITH_ONE_PREDICATE: &str = concat!(
    "/// Whether the peer answered that it has no such session.\n",
    "#[must_use]\n",
    "pub fn peer_has_no_such_session(code: u32) -> bool {\n",
    "    // the peer's wire code for \"not found\"\n",
    "    code == 404\n",
    "}\n",
    "\n",
    "pub fn unrelated() -> u32 {\n",
    "    7\n",
    "}\n",
);
const A_HANDLER_THAT_IMPORTS_IT: &str = concat!(
    "use crate::pairing::peer_has_no_such_session;\n",
    "\n",
    "pub fn handle(code: u32) -> bool {\n",
    "    peer_has_no_such_session(code)\n",
    "}\n",
);
const AN_AUDIT_THAT_NAMES_IT_INLINE: &str = concat!(
    "pub fn audit(code: u32) -> bool {\n",
    "    crate::pairing::peer_has_no_such_session(code)\n",
    "}\n",
);

/// One predicate in `pairing`, reached by one `use` import and one inline-qualified path.
fn a_crate_whose_predicate_has_two_callers() -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        ("src/pairing.rs", PAIRING_WITH_ONE_PREDICATE),
        ("src/handler.rs", A_HANDLER_THAT_IMPORTS_IT),
        ("src/audit.rs", AN_AUDIT_THAT_NAMES_IT_INLINE),
    ])
}

/// A crate whose `pairing` module holds `pairing_text`, with the two callers of the predicate in
/// `pairing_text` and an empty `answers` module to move into.
fn a_crate_whose_pairing_module_reads(pairing_text: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        ("src/pairing.rs", pairing_text),
        ("src/handler.rs", A_HANDLER_THAT_IMPORTS_IT),
        ("src/audit.rs", AN_AUDIT_THAT_NAMES_IT_INLINE),
    ])
}

/// AC1 — an item leaves its file, arrives in the other module, and its caller follows.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_function_into_a_module_of_another_file_and_its_caller_follows() {
    // Given a predicate in `pairing`, called from `handler`
    let workspace = a_crate_whose_predicate_has_two_callers();

    // When it is moved into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then it is defined in `answers` and no longer in `pairing`, and the tree compiles
    assert!(
        workspace
            .read("src/answers.rs")
            .contains("fn peer_has_no_such_session(code: u32) -> bool"),
        "the predicate did not arrive in `answers`"
    );
    assert!(
        !workspace
            .read("src/pairing.rs")
            .contains("fn peer_has_no_such_session"),
        "the predicate is still defined in `pairing`"
    );
    assert!(
        workspace.read("src/pairing.rs").contains("fn unrelated"),
        "an item that was not named left with it"
    );
    assert_compiles(&workspace);
}

/// AC2 — every way another file named the item is re-pointed, not only the `use` line.
///
/// The inline-qualified path is what a text search for the `use` line would miss, and what makes the
/// reference set — not a grep — the source of every rewrite.
#[tokio::test(flavor = "multi_thread")]
async fn re_points_a_use_import_and_an_inline_qualified_path_in_other_files() {
    // Given one caller that imports the predicate and one that names it inline
    let workspace = a_crate_whose_predicate_has_two_callers();

    // When it moves into `answers`, with callers re-pointed
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then both callers name the new module
    assert!(
        workspace
            .read("src/handler.rs")
            .contains("answers::peer_has_no_such_session"),
        "the importing caller still points at `pairing`:\n{}",
        workspace.read("src/handler.rs")
    );
    assert!(
        workspace
            .read("src/audit.rs")
            .contains("answers::peer_has_no_such_session"),
        "the inline-qualified caller still points at `pairing`:\n{}",
        workspace.read("src/audit.rs")
    );
}

/// AC3 — what an author wrote around the item travels with it.
///
/// `extract_module` drops comments from the code it moves (a backlog entry of its own). A move this
/// package authors copies the item's bytes, so the doc comment, the attribute and the comment inside
/// the body all arrive — and a regression here is invisible to the compiler and to every other test.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_doc_comment_the_attribute_and_the_inner_comment_of_the_moved_item() {
    // Given a predicate with a doc comment, an attribute and a comment in its body
    let workspace = a_crate_whose_predicate_has_two_callers();

    // When it moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then all three arrive in `answers`
    let arrived = workspace.read("src/answers.rs");
    assert!(
        arrived.contains("/// Whether the peer answered that it has no such session."),
        "the doc comment was dropped:\n{arrived}"
    );
    assert!(
        arrived.contains("#[must_use]"),
        "the attribute was dropped:\n{arrived}"
    );
    assert!(
        arrived.contains("// the peer's wire code for \"not found\""),
        "the comment in the body was dropped:\n{arrived}"
    );
}

/// AC4 — a contiguous run moves as one operation, in order.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_contiguous_run_of_items_in_one_operation() {
    // Given a constant and the predicate that reads it, side by side
    let workspace = a_crate_whose_pairing_module_reads(concat!(
        "pub const NOT_FOUND: u32 = 404;\n",
        "\n",
        "pub fn peer_has_no_such_session(code: u32) -> bool {\n",
        "    code == NOT_FOUND\n",
        "}\n",
        "\n",
        "pub fn unrelated() -> u32 {\n",
        "    7\n",
        "}\n",
    ));

    // When the two are moved together
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["NOT_FOUND", "peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then both are in `answers`, neither is left in `pairing`, and the tree compiles
    let arrived = workspace.read("src/answers.rs");
    assert!(
        arrived.contains("const NOT_FOUND") && arrived.contains("fn peer_has_no_such_session"),
        "the run did not arrive whole:\n{arrived}"
    );
    let left = workspace.read("src/pairing.rs");
    assert!(
        !left.contains("NOT_FOUND") && !left.contains("peer_has_no_such_session"),
        "part of the run is still in `pairing`:\n{left}"
    );
    assert_compiles(&workspace);
}

/// AC5 — with a facade, no caller changes at all.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_glob_facade_that_keeps_every_caller_unchanged() {
    // Given the predicate and its two callers
    let workspace = a_crate_whose_predicate_has_two_callers();

    // When it moves with `reexport: glob`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        Some("glob"),
    )
    .await
    .expect("the move applies");

    // Then `pairing` keeps a `pub use`, the callers are byte for byte what they were, and it compiles
    assert!(
        workspace.read("src/pairing.rs").contains("pub use"),
        "no facade was left in `pairing`:\n{}",
        workspace.read("src/pairing.rs")
    );
    assert_eq!(
        workspace.read("src/handler.rs"),
        A_HANDLER_THAT_IMPORTS_IT,
        "a caller was edited although a facade stands in for the old path"
    );
    assert_eq!(
        workspace.read("src/audit.rs"),
        AN_AUDIT_THAT_NAMES_IT_INLINE,
        "a caller was edited although a facade stands in for the old path"
    );
    assert_compiles(&workspace);
}

/// AC6 — a `#[cfg(test)]` module that names the moved item keeps compiling.
///
/// `cargo check` without `--all-targets` never builds it, so a move that broke the module's `use`
/// would pass every other test here.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_a_test_module_that_names_the_moved_item_compiling() {
    // Given the predicate with an inline test module beside it, importing it through `super`
    let workspace = a_crate_whose_pairing_module_reads(concat!(
        "pub fn peer_has_no_such_session(code: u32) -> bool {\n",
        "    code == 404\n",
        "}\n",
        "\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    use super::peer_has_no_such_session;\n",
        "\n",
        "    #[test]\n",
        "    fn a_404_is_a_missing_session() {\n",
        "        assert!(peer_has_no_such_session(404));\n",
        "    }\n",
        "}\n",
    ));

    // When the predicate moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the test module is still there, and the tree compiles with its tests
    assert!(
        workspace.read("src/pairing.rs").contains("mod tests"),
        "the test module was removed with the item"
    );
    assert_compiles_with_its_tests(&workspace);
}

/// AC7 — a private item is widened only as far as its callers need, and not to `pub`.
#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_item_only_as_far_as_its_callers_need() {
    // Given a private predicate that a function left behind in `pairing` calls
    let workspace = a_crate_whose_pairing_module_reads(concat!(
        "fn is_missing(code: u32) -> bool {\n",
        "    code == 404\n",
        "}\n",
        "\n",
        "pub fn peer_has_no_such_session(code: u32) -> bool {\n",
        "    is_missing(code)\n",
        "}\n",
    ));

    // When the private predicate moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["is_missing"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then it is visible to `pairing` without having become part of the crate's public surface
    let arrived = workspace.read("src/answers.rs");
    assert!(
        arrived.contains("fn is_missing"),
        "the predicate did not arrive:\n{arrived}"
    );
    assert!(
        !arrived.contains("pub fn is_missing"),
        "the item was widened to `pub`, further than its caller needs:\n{arrived}"
    );
    assert_compiles(&workspace);
}

/// AC8 — the other direction: the moved code reaches a private item that stays behind.
///
/// A sibling module cannot see `pairing`'s private items, so the item left behind is widened, and
/// the move is not refused for it.
#[tokio::test(flavor = "multi_thread")]
async fn widens_a_private_item_left_behind_that_the_moved_code_reaches() {
    // Given a predicate that reads a private limit defined beside it
    let workspace = a_crate_whose_pairing_module_reads(concat!(
        "fn limit() -> u32 {\n",
        "    404\n",
        "}\n",
        "\n",
        "pub fn peer_has_no_such_session(code: u32) -> bool {\n",
        "    code == limit()\n",
        "}\n",
    ));

    // When only the predicate moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the limit left behind is visible to it, and the tree compiles
    let left = workspace.read("src/pairing.rs");
    assert!(
        left.contains("fn limit") && !left.starts_with("fn limit"),
        "the item left behind is still private, so the moved code cannot reach it:\n{left}"
    );
    assert_compiles(&workspace);
}

/// AC9 — the result is as lint-clean as the code it came from.
///
/// A move that compiles can still leave CI's lint job red (a backlog entry of its own).
#[tokio::test(flavor = "multi_thread")]
async fn leaves_the_workspace_clean_under_clippy() {
    // Given the predicate and its callers
    let workspace = a_crate_whose_predicate_has_two_callers();

    // When it moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then `cargo clippy -D warnings` over the workspace passes
    assert_lints_clean(&workspace);
}

/// AC10 — a destination that does not exist is refused before a server exists.
///
/// A move never creates a module: a file with arbitrary content is what `create_file` was kept out
/// of the vocabulary to prevent. The module is made first, by `extract_module` or `reparent_module`.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_destination_module_that_does_not_exist() {
    // Given a predicate and a destination path that names no module
    let workspace = a_crate_whose_predicate_has_two_callers();
    let anchor = the_anchor_over(&workspace, "src/pairing.rs", &["peer_has_no_such_session"]).await;

    // When a static check reads the plan
    let findings =
        what_a_static_check_finds_in(&workspace, a_move_item_op(&anchor, "app::nowhere", None))
            .await;

    // Then it says which module is missing
    let said = findings.join("\n");
    assert!(
        said.contains("app::nowhere") && said.contains("does not exist"),
        "the refusal did not name the missing destination: {said}"
    );
}

/// AC11 — a name the destination already declares is refused, and says whose it is.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_name_the_destination_already_declares() {
    // Given `answers` already defining a function of the same name
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/answers.rs",
            "pub fn peer_has_no_such_session(_code: u32) -> bool {\n    false\n}\n",
        ),
        ("src/pairing.rs", PAIRING_WITH_ONE_PREDICATE),
        ("src/handler.rs", A_HANDLER_THAT_IMPORTS_IT),
        ("src/audit.rs", AN_AUDIT_THAT_NAMES_IT_INLINE),
    ]);
    let anchor = the_anchor_over(&workspace, "src/pairing.rs", &["peer_has_no_such_session"]).await;

    // When a static check reads the plan
    let findings =
        what_a_static_check_finds_in(&workspace, a_move_item_op(&anchor, "app::answers", None))
            .await;

    // Then the clash is named, with the destination that holds it
    let said = findings.join("\n");
    assert!(
        said.contains("peer_has_no_such_session") && said.contains("already"),
        "the refusal did not name the clash: {said}"
    );
}

/// AC12 — moving an item to the module it is already in is a plan defect, not a no-op.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_destination_that_is_the_items_own_module() {
    // Given the predicate in `pairing`
    let workspace = a_crate_whose_predicate_has_two_callers();
    let anchor = the_anchor_over(&workspace, "src/pairing.rs", &["peer_has_no_such_session"]).await;

    // When the plan names `pairing` as the destination
    let findings =
        what_a_static_check_finds_in(&workspace, a_move_item_op(&anchor, "app::pairing", None))
            .await;

    // Then it is refused, saying the item is already there
    let said = findings.join("\n");
    assert!(
        said.contains("already in") && said.contains("app::pairing"),
        "the refusal did not say the item is already in the destination: {said}"
    );
}

/// AC13 — an `impl` member is not an item that can move alone.
///
/// The new module would hold half of an `impl`, which is its own transformation (the engine's
/// impl-seam refusals stand); an `impl` block moves as an item of its own, with its type.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_item_that_sits_inside_an_impl_block() {
    // Given a method of an inherent `impl`
    let workspace = a_crate_whose_pairing_module_reads(concat!(
        "pub struct Counter(u32);\n",
        "\n",
        "impl Counter {\n",
        "    pub fn bump(&mut self) -> u32 {\n",
        "        self.0 += 1;\n",
        "        self.0\n",
        "    }\n",
        "}\n",
    ));
    let method = Range {
        start: at(5, 9),
        end: at(6, 15),
    };
    let anchor = the_anchor_command_emits(&workspace, "src/pairing.rs", &[], Some(method))
        .await
        .expect("an item anchor over the method body");

    // When it is moved into `answers`
    let refusal = refusal_from(&workspace, a_move_item_op(&anchor, "app::answers", None)).await;

    // Then it is refused as a member of an `impl`
    assert!(
        refusal.contains("impl"),
        "the refusal did not say the item sits inside an `impl`: {refusal}"
    );
}
