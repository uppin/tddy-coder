//! `move_item` into a module that already exists, and the four ways the first real use of it broke.
//!
//! Gathering a topic module from items in several files is one `move_item` that creates the module and
//! then more that move into it. The first lifecycle plan of that shape applied, then failed its compile
//! gate with seven errors, each a defect of a *later* move meeting what the earlier one left. Every
//! suite until then moved one item into an empty module, which is why none of them saw it.
//!
//! `cargo check` is the assertion no edit that merely looks right can satisfy.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{assert_compiles, assert_compiles_with_its_tests};
use same_crate::{an_app_holding, an_app_with_a_consumer, moving_items};

/// A module that already exists is declared as narrowly as its first user needed, and a later move
/// can bring it a caller further out.
#[tokio::test(flavor = "multi_thread")]
async fn widens_the_declaration_of_an_existing_module_that_a_caller_further_out_now_needs() {
    // Given `host::answers` declared privately inside `host`, and a caller at the crate root
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod handler;\npub mod host;\npub mod pairing;\n"),
        (
            "src/host.rs",
            "mod answers;\n\npub fn host_name() -> &'static str {\n    \"host\"\n}\n",
        ),
        ("src/host/answers.rs", "//! What a peer answered.\n"),
        (
            "src/pairing.rs",
            "pub fn peer_has_no_such_session(code: u32) -> bool {\n    code == 404\n}\n",
        ),
        (
            "src/handler.rs",
            "use crate::pairing::peer_has_no_such_session;\n\npub fn handle(code: u32) -> bool {\n    peer_has_no_such_session(code)\n}\n",
        ),
    ]);

    // When the predicate moves into it, and the handler is re-pointed
    moving_items(
        &workspace,
        "src/pairing.rs",
        &["peer_has_no_such_session"],
        "app::host::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the module is visible to the handler that now names it
    assert!(
        !workspace.read("src/host.rs").starts_with("mod answers;"),
        "the declaration was left private although a caller outside `host` now names it:\n{}",
        workspace.read("src/host.rs")
    );
    assert_compiles(&workspace);
}

/// A glob import the moved code needs, of a module the source's own parent keeps private.
#[tokio::test(flavor = "multi_thread")]
async fn does_not_copy_a_glob_import_it_cannot_reach_from_the_destination() {
    // Given a function that reaches a private sibling module through a glob import
    let workspace = an_app_holding(&[
        ("src/lib.rs", "pub mod answers;\npub mod host;\n"),
        ("src/answers.rs", "//! Answers.\n"),
        ("src/host.rs", "mod secret;\n\npub mod pairing;\n"),
        (
            "src/host/secret.rs",
            "pub fn limit() -> u32 {\n    404\n}\n",
        ),
        (
            "src/host/pairing.rs",
            "use super::secret::*;\n\npub fn answer(code: u32) -> bool {\n    code == limit()\n}\n",
        ),
    ]);

    // When it moves out of `host` into `answers`
    moving_items(
        &workspace,
        "src/host/pairing.rs",
        &["answer"],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then what it needs from `secret` is reachable, and the tree compiles
    assert_compiles(&workspace);
}

/// Two moves that both bring the same import.
#[tokio::test(flavor = "multi_thread")]
async fn imports_a_name_once_when_two_moves_into_the_same_module_both_need_it() {
    // Given two functions in two files, each importing the same type
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod answers;\npub mod one;\npub mod two;\npub mod types;\n",
        ),
        ("src/answers.rs", "//! Answers.\n"),
        ("src/types.rs", "pub struct Status(pub u32);\n"),
        (
            "src/one.rs",
            "use crate::types::Status;\n\npub fn first(s: Status) -> u32 {\n    s.0\n}\n",
        ),
        (
            "src/two.rs",
            "use crate::types::Status;\n\npub fn second(s: Status) -> u32 {\n    s.0 + 1\n}\n",
        ),
    ]);

    // When both move into `answers`, one after the other
    moving_items(&workspace, "src/one.rs", &["first"], "app::answers", None)
        .await
        .expect("the first move applies");
    moving_items(&workspace, "src/two.rs", &["second"], "app::answers", None)
        .await
        .expect("the second move applies");

    // Then the type is imported once, above the items, and the tree compiles
    let gathered = workspace.read("src/answers.rs");
    assert_eq!(
        gathered.matches("types::Status").count(),
        1,
        "the same import was written twice:\n{gathered}"
    );
    let first_import = gathered.find("use ").expect("an import");
    let first_item = gathered.find("pub fn").expect("an item");
    assert!(
        first_import < first_item,
        "an import was written after the first item:\n{gathered}"
    );
    assert_compiles(&workspace);
}

/// The file a free function leaves is also the file that calls it.
///
/// A re-pointed `use` for the caller inside the file and the facade a consumer needs bind the same
/// name twice there.
#[tokio::test(flavor = "multi_thread")]
async fn binds_the_moved_name_once_in_the_file_it_left_when_a_facade_is_also_needed() {
    // Given a free function that its own file also calls through a method, and a consumer elsewhere
    let workspace = an_app_with_a_consumer(
        &[
            ("src/lib.rs", "pub mod answers;\npub mod svc;\n"),
            ("src/answers.rs", "//! Answers.\n"),
            (
                "src/svc.rs",
                concat!(
                    "pub struct Host;\n",
                    "\n",
                    "pub fn resolve(code: u32) -> u32 {\n",
                    "    code + 1\n",
                    "}\n",
                    "\n",
                    "impl Host {\n",
                    "    pub fn resolve_here(&self, code: u32) -> u32 {\n",
                    "        resolve(code)\n",
                    "    }\n",
                    "}\n",
                ),
            ),
        ],
        &[(
            "src/lib.rs",
            "pub fn run() -> u32 {\n    app::svc::resolve(1)\n}\n",
        )],
    );

    // When the function moves into `answers` with a facade for the consumer
    moving_items(
        &workspace,
        "app/src/svc.rs",
        &["resolve"],
        "app::answers",
        Some("outside"),
    )
    .await
    .expect("the move applies");

    // Then the file it left binds the name once, and the workspace compiles with its tests
    assert_compiles_with_its_tests(&workspace);
}
