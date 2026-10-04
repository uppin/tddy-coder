//! `move_item` of an item that the destination module already imports.
//!
//! Found moving `ExecToolRoute` beside `LocalExecTools`: the destination file used the type through
//! `use super::seeded_clone_guard::ExecToolRoute;`, and the move was refused because the destination
//! "already declares" the name. That import is not a second declaration: it is the very item that is
//! moving, and the move's own re-pointing removes it.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::assert_compiles;
use same_crate::{an_app_holding, moving_items};

const LIB: &str = "pub mod answers;\npub mod pairing;\n";
const PAIRING: &str = "pub fn peer_has_no_such_session(code: u32) -> bool {\n    code == 404\n}\n";
const ANSWERS_THAT_IMPORT_IT: &str = concat!(
    "use crate::pairing::peer_has_no_such_session;\n",
    "\n",
    "pub fn answer(code: u32) -> bool {\n",
    "    peer_has_no_such_session(code)\n",
    "}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn moves_an_item_into_the_module_that_imports_it() {
    // Given a destination module that uses the item through an import of it
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/pairing.rs", PAIRING),
        ("src/answers.rs", ANSWERS_THAT_IMPORT_IT),
    ]);

    // When the item moves into that module
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the item is defined there once, the import of it is gone, and the tree compiles
    let arrived = workspace.read("src/answers.rs");
    assert_eq!(
        arrived
            .matches("peer_has_no_such_session(code: u32)")
            .count(),
        1,
        "the item is not defined exactly once:\n{arrived}"
    );
    assert!(
        !arrived.contains("use crate::pairing::peer_has_no_such_session"),
        "the import of the moved item was left behind:\n{arrived}"
    );
    assert_compiles(&workspace);
}
