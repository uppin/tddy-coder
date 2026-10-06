//! `move_item` and `reparent_module` write what a reader of the result would have written, against a
//! live rust-analyzer.
//!
//! Three things a move got right and still wrote badly, each found moving a whole crate's worth of
//! items by hand-free plans: a caller that reached the item through `use crate::old;` gets a full
//! `crate::new::f()` inlined instead of the short form and an import (B1); a path in the moved text
//! that goes through a `pub use` facade travels as written, so the module cannot later leave its
//! crate (B2); and an intra-doc link to the moved item keeps pointing at the old path (B3).
//!
//! `cargo check` and `cargo doc` are the assertions no edit that merely looks right can satisfy.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;
mod same_crate;

use harness::{assert_compiles, assert_lints_clean, AFixtureWorkspace};
use same_crate::{
    an_app_holding, an_app_over_a_kernel, assert_docs_resolve, moving_items,
    moving_items_with_canonical_paths, reparenting_module,
};

const THE_PREDICATE: &str = "peer_has_no_such_session";
const LIB: &str = "pub mod answers;\npub mod audit;\npub mod handler;\npub mod pairing;\n";
const AN_EMPTY_ANSWERS_MODULE: &str = "//! What a peer answered about a session.\n";
const PAIRING: &str = concat!(
    "pub fn peer_has_no_such_session(code: u32) -> bool {\n",
    "    code == 404\n",
    "}\n",
    "\n",
    "pub fn unrelated() -> u32 {\n",
    "    7\n",
    "}\n",
);
const AN_AUDIT_WITHOUT_DOCS: &str = "pub fn audit(code: u32) -> bool {\n    code == 0\n}\n";

/// A crate whose `handler` reads `handler_text` and whose `audit` reads `audit_text`; the predicate
/// sits in `pairing` and `answers` is empty.
fn a_crate_with_a_handler_and_an_audit(handler_text: &str, audit_text: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", LIB),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        ("src/pairing.rs", PAIRING),
        ("src/handler.rs", handler_text),
        ("src/audit.rs", audit_text),
    ])
}

/// Move the predicate out of `pairing` into `answers`, with callers re-pointed.
async fn moving_the_predicate_into_answers(workspace: &AFixtureWorkspace) {
    moving_items(
        workspace,
        "src/pairing.rs",
        &[THE_PREDICATE],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");
}

// B1 — a module-qualified call keeps its qualifier

/// B1 — the short form survives, and the module it names is imported.
#[tokio::test(flavor = "multi_thread")]
async fn a_module_qualified_call_keeps_its_qualifier_and_gains_an_import_of_the_new_module() {
    // Given a caller that reaches the predicate as `pairing::…` through `use crate::pairing;`
    let workspace = a_crate_with_a_handler_and_an_audit(
        concat!(
            "use crate::pairing;\n",
            "\n",
            "pub fn handle(code: u32) -> bool {\n",
            "    pairing::peer_has_no_such_session(code)\n",
            "}\n",
        ),
        AN_AUDIT_WITHOUT_DOCS,
    );

    // When the predicate moves into `answers`
    moving_the_predicate_into_answers(&workspace).await;

    // Then the caller binds `answers` and calls through it, with no inlined full path
    let handler = workspace.read("src/handler.rs");
    assert!(
        handler.contains("use crate::answers;"),
        "the caller does not import the new module:\n{handler}"
    );
    assert!(
        handler.contains("answers::peer_has_no_such_session(code)"),
        "the caller does not call through the new module:\n{handler}"
    );
    assert!(
        !handler.contains("crate::answers::peer_has_no_such_session("),
        "the caller got a full path inlined where it wrote a short one:\n{handler}"
    );
    assert_compiles(&workspace);
}

/// B1 — an import the caller already had is not written twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_caller_that_already_imports_the_new_module_gets_no_second_import() {
    // Given a caller that imports both `pairing` and `answers`, and calls through `pairing`
    let workspace = an_app_holding(&[
        ("src/lib.rs", LIB),
        (
            "src/answers.rs",
            "pub fn module_name() -> &'static str {\n    \"answers\"\n}\n",
        ),
        ("src/pairing.rs", PAIRING),
        (
            "src/handler.rs",
            concat!(
                "use crate::answers;\n",
                "use crate::pairing;\n",
                "\n",
                "pub fn handle(code: u32) -> bool {\n",
                "    answers::module_name().is_empty() || pairing::peer_has_no_such_session(code)\n",
                "}\n",
            ),
        ),
        ("src/audit.rs", AN_AUDIT_WITHOUT_DOCS),
    ]);

    // When the predicate moves into `answers`
    moving_the_predicate_into_answers(&workspace).await;

    // Then `answers` is imported exactly once, and the call is the short form
    let handler = workspace.read("src/handler.rs");
    assert_eq!(
        handler.matches("use crate::answers;").count(),
        1,
        "the new module is imported more than once:\n{handler}"
    );
    assert!(
        handler.contains("answers::peer_has_no_such_session(code)")
            && !handler.contains("crate::answers::peer_has_no_such_session("),
        "the call is not the short form:\n{handler}"
    );
    assert_compiles(&workspace);
}

/// B1 pin — the old import stays while it still serves something that did not move.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_old_import_when_it_still_serves_an_item_that_stayed() {
    // Given a caller that also calls an item of `pairing` that stays
    let workspace = a_crate_with_a_handler_and_an_audit(
        concat!(
            "use crate::pairing;\n",
            "\n",
            "pub fn handle(code: u32) -> bool {\n",
            "    pairing::unrelated() > 0 && pairing::peer_has_no_such_session(code)\n",
            "}\n",
        ),
        AN_AUDIT_WITHOUT_DOCS,
    );

    // When the predicate moves into `answers`
    moving_the_predicate_into_answers(&workspace).await;

    // Then `pairing` is still imported, and the tree compiles and lints clean
    let handler = workspace.read("src/handler.rs");
    assert!(
        handler.contains("use crate::pairing;"),
        "the old import went although `pairing::unrelated` still needs it:\n{handler}"
    );
    assert_compiles(&workspace);
    assert_lints_clean(&workspace);
}

/// B1 pin — a name the caller already uses for something else is not taken over.
#[tokio::test(flavor = "multi_thread")]
async fn writes_the_full_path_where_the_new_modules_name_is_already_taken() {
    // Given a caller that declares a module of its own named `answers`
    let workspace = a_crate_with_a_handler_and_an_audit(
        concat!(
            "use crate::pairing;\n",
            "\n",
            "mod answers {\n",
            "    pub fn local() -> u32 {\n",
            "        1\n",
            "    }\n",
            "}\n",
            "\n",
            "pub fn handle(code: u32) -> bool {\n",
            "    answers::local() > 0 && pairing::peer_has_no_such_session(code)\n",
            "}\n",
        ),
        AN_AUDIT_WITHOUT_DOCS,
    );

    // When the predicate moves into the crate's `answers`
    moving_the_predicate_into_answers(&workspace).await;

    // Then the call is the full path, which cannot be confused with the local module, and compiles
    let handler = workspace.read("src/handler.rs");
    assert!(
        handler.contains("crate::answers::peer_has_no_such_session(code)"),
        "the call does not name the crate's `answers` in full:\n{handler}"
    );
    assert_compiles(&workspace);
}

// B2 — canonical paths

const THE_KERNEL_CONFIG: &str = concat!(
    "pub mod config {\n",
    "    pub struct Settings {\n",
    "        pub level: u32,\n",
    "    }\n",
    "    pub struct Level;\n",
    "    pub struct Mode;\n",
    "}\n",
);
const AN_APP_LIB_WITH_A_FACADE: &str = concat!(
    "pub mod a;\n",
    "pub mod b;\n",
    "pub mod own;\n",
    "pub use kernel::config;\n",
);
const AN_OWN_MODULE: &str = "pub struct Thing;\npub struct Marker;\n";
const AN_EMPTY_B_MODULE: &str = "//! Where the item lands.\n";

/// An app that re-exports `kernel::config` and holds `a_text` in `a`, over a kernel whose root reads
/// `kernel_lib`.
fn an_app_whose_a_module_reads(a_text: &str, kernel_lib: &str) -> AFixtureWorkspace {
    an_app_over_a_kernel(
        &[
            ("src/lib.rs", AN_APP_LIB_WITH_A_FACADE),
            ("src/a.rs", a_text),
            ("src/b.rs", AN_EMPTY_B_MODULE),
            ("src/own.rs", AN_OWN_MODULE),
        ],
        &[("src/lib.rs", kernel_lib)],
    )
}

const F_OVER_A_FACADE_PATH: &str =
    "pub fn f(c: &crate::config::Settings) -> u32 {\n    c.level\n}\n";

/// B2 — the defining path replaces the facade path.
#[tokio::test(flavor = "multi_thread")]
async fn writes_the_defining_path_for_a_facade_path_in_the_moved_signature() {
    // Given `f` whose signature names `crate::config::Settings`, a facade over `kernel::config`
    let workspace = an_app_whose_a_module_reads(F_OVER_A_FACADE_PATH, THE_KERNEL_CONFIG);

    // When it moves to `b` with `canonical_paths`
    let (moved, _) =
        moving_items_with_canonical_paths(&workspace, "app/src/a.rs", &["f"], "app::b").await;
    moved.expect("the move applies");

    // Then `b` names the defining path, and not the facade, and the workspace compiles
    let arrived = workspace.read("app/src/b.rs");
    assert!(
        arrived.contains("kernel::config::Settings"),
        "the moved signature does not name the defining path:\n{arrived}"
    );
    assert!(
        !arrived.contains("crate::config::Settings"),
        "the moved signature still goes through the facade:\n{arrived}"
    );
    assert_compiles(&workspace);
}

/// B2 pin — without the field a move writes what it always did.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_a_facade_path_as_written_when_canonical_paths_is_off() {
    // Given the same `f`
    let workspace = an_app_whose_a_module_reads(F_OVER_A_FACADE_PATH, THE_KERNEL_CONFIG);

    // When it moves to `b` without asking for canonical paths
    moving_items(&workspace, "app/src/a.rs", &["f"], "app::b", None)
        .await
        .expect("the move applies");

    // Then the facade path travels as written
    let arrived = workspace.read("app/src/b.rs");
    assert!(
        arrived.contains("&crate::config::Settings") && !arrived.contains("kernel::config"),
        "a plan without `canonical_paths` changed the moved text:\n{arrived}"
    );
    assert_compiles(&workspace);
}

/// B2 — a path through a module the crate defines is already canonical.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_path_through_a_module_the_crate_defines_alone() {
    // Given `f` whose signature names `crate::own::Thing`, a module `app` defines itself
    let workspace = an_app_whose_a_module_reads(
        "pub fn f(_thing: &crate::own::Thing) -> u32 {\n    0\n}\n",
        THE_KERNEL_CONFIG,
    );

    // When it moves to `b` with `canonical_paths`
    let (moved, _) =
        moving_items_with_canonical_paths(&workspace, "app/src/a.rs", &["f"], "app::b").await;
    moved.expect("the move applies");

    // Then the path is as written
    let arrived = workspace.read("app/src/b.rs");
    assert!(
        arrived.contains("&crate::own::Thing"),
        "a path that was already canonical was rewritten:\n{arrived}"
    );
    assert_compiles(&workspace);
}

/// B2 (O3) — a defining module the destination may not name is not a path to write.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_facade_path_as_written_when_the_defining_module_is_private_and_says_so() {
    // Given a kernel whose `Thing` is defined in a private module and re-exported at its root, and
    // an app that re-exports it in turn
    let workspace = an_app_over_a_kernel(
        &[
            (
                "src/lib.rs",
                "pub mod a;\npub mod b;\npub use kernel::Thing;\n",
            ),
            (
                "src/a.rs",
                "pub fn f(_thing: &crate::Thing) -> u32 {\n    0\n}\n",
            ),
            ("src/b.rs", AN_EMPTY_B_MODULE),
        ],
        &[(
            "src/lib.rs",
            "mod inner {\n    pub struct Thing;\n}\npub use inner::Thing;\n",
        )],
    );

    // When `f` moves to `b` with `canonical_paths`
    let (moved, heard) =
        moving_items_with_canonical_paths(&workspace, "app/src/a.rs", &["f"], "app::b").await;
    moved.expect("the move applies");

    // Then the path is as written, the workspace compiles, and the run says why it was left
    let arrived = workspace.read("app/src/b.rs");
    assert!(
        arrived.contains("&crate::Thing"),
        "a path to a private defining module was rewritten:\n{arrived}"
    );
    assert_compiles(&workspace);
    let said = heard.join("\n");
    assert!(
        said.contains("crate::Thing") && said.contains("private"),
        "the run does not say a path was left because its defining module is private:\n{said}"
    );
}

/// B2 — the account of the run names every path, rewritten or left.
#[tokio::test(flavor = "multi_thread")]
async fn names_every_path_it_rewrote_and_every_one_it_left() {
    // Given an `f` with one rewritable facade path, one inside a grouped `use`, and one whose
    // spelling the span does not read back as written
    let workspace = an_app_whose_a_module_reads(
        concat!(
            "pub fn f(c: &crate::config::Settings) -> u32 {\n",
            "    use crate::{config::Level, own::Marker};\n",
            "    let _level: Option<Level> = None;\n",
            "    let _marker: Option<Marker> = None;\n",
            "    let _mode: Option<crate :: config :: Mode> = None;\n",
            "    c.level\n",
            "}\n",
        ),
        THE_KERNEL_CONFIG,
    );

    // When it moves to `b` with `canonical_paths`
    let (moved, heard) =
        moving_items_with_canonical_paths(&workspace, "app/src/a.rs", &["f"], "app::b").await;
    moved.expect("the move applies");

    // Then there is a line for each path, naming what was written and what defines it
    let said = heard.join("\n");
    for (written, defined_at) in [
        ("crate::config::Settings", "kernel::config::Settings"),
        ("crate::config::Level", "kernel::config::Level"),
        ("crate::config::Mode", "kernel::config::Mode"),
    ] {
        assert!(
            heard
                .iter()
                .any(|line| line.contains(written) && line.contains(defined_at)),
            "no line of the run names `{written}` with its defining path `{defined_at}`:\n{said}"
        );
    }
}

// B3 — doc links follow the item

const A_LIB_FOR_DOCS: &str = "pub mod answers;\npub mod audit;\npub mod pairing;\n";

/// A crate whose `audit` module reads `audit_text`, with the predicate in `pairing`.
fn a_crate_whose_audit_module_reads(audit_text: &str) -> AFixtureWorkspace {
    an_app_holding(&[
        ("src/lib.rs", A_LIB_FOR_DOCS),
        ("src/answers.rs", AN_EMPTY_ANSWERS_MODULE),
        ("src/pairing.rs", PAIRING),
        ("src/audit.rs", audit_text),
    ])
}

/// B3 — a link in an item's doc comment follows the moved item.
#[tokio::test(flavor = "multi_thread")]
async fn a_doc_link_to_the_moved_item_follows_it() {
    // Given a doc comment linking `crate::pairing::peer_has_no_such_session`
    let workspace = a_crate_whose_audit_module_reads(concat!(
        "/// See [`crate::pairing::peer_has_no_such_session`].\n",
        "pub fn audit(code: u32) -> bool {\n",
        "    code == 0\n",
        "}\n",
    ));

    // When the predicate moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &[THE_PREDICATE],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the link names the new path, and it resolves
    let audit = workspace.read("src/audit.rs");
    assert!(
        audit.contains("[`crate::answers::peer_has_no_such_session`]"),
        "the doc link still names the old path:\n{audit}"
    );
    assert_docs_resolve(&workspace);
}

/// B3 — the same, in a module's own `//!` documentation.
#[tokio::test(flavor = "multi_thread")]
async fn a_module_doc_link_follows_it() {
    // Given a module doc comment linking the predicate
    let workspace = a_crate_whose_audit_module_reads(concat!(
        "//! See [`crate::pairing::peer_has_no_such_session`].\n",
        "\n",
        "pub fn audit(code: u32) -> bool {\n",
        "    code == 0\n",
        "}\n",
    ));

    // When the predicate moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &[THE_PREDICATE],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then the link names the new path, and it resolves
    let audit = workspace.read("src/audit.rs");
    assert!(
        audit.contains("[`crate::answers::peer_has_no_such_session`]"),
        "the module doc link still names the old path:\n{audit}"
    );
    assert_docs_resolve(&workspace);
}

/// B3 pin — the old path in prose, or in an example, is not a link.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_prose_and_fenced_examples_alone() {
    // Given the old path written as plain prose and inside a fenced example
    let audit_text = concat!(
        "/// This lived at crate::pairing::peer_has_no_such_session until it moved.\n",
        "///\n",
        "/// ```text\n",
        "/// crate::pairing::peer_has_no_such_session(404)\n",
        "/// ```\n",
        "pub fn audit(code: u32) -> bool {\n",
        "    code == 0\n",
        "}\n",
    );
    let workspace = a_crate_whose_audit_module_reads(audit_text);

    // When the predicate moves into `answers`
    moving_items(
        &workspace,
        "src/pairing.rs",
        &[THE_PREDICATE],
        "app::answers",
        None,
    )
    .await
    .expect("the move applies");

    // Then neither is touched
    assert_eq!(
        workspace.read("src/audit.rs"),
        audit_text,
        "prose or an example was rewritten as if it were a link"
    );
}

/// B3 — `reparent_module` shares the behaviour.
#[tokio::test(flavor = "multi_thread")]
async fn a_reparented_modules_doc_link_follows_it() {
    // Given a doc comment linking `crate::host::attachments::materialize`
    let workspace = an_app_holding(&[
        (
            "src/lib.rs",
            "pub mod audit;\npub mod host;\npub mod split;\n",
        ),
        ("src/host.rs", "pub mod attachments;\n"),
        (
            "src/host/attachments.rs",
            "pub fn materialize() -> u32 {\n    1\n}\n",
        ),
        ("src/split.rs", "pub fn start() -> u32 {\n    0\n}\n"),
        (
            "src/audit.rs",
            concat!(
                "/// See [`crate::host::attachments::materialize`].\n",
                "pub fn audit() -> u32 {\n",
                "    0\n",
                "}\n",
            ),
        ),
    ]);

    // When `attachments` is re-parented under `split`
    reparenting_module(&workspace, "src/host.rs", "attachments", "app::split", None)
        .await
        .expect("the re-parent applies");

    // Then the link names the module's new home, and it resolves
    let audit = workspace.read("src/audit.rs");
    assert!(
        audit.contains("[`crate::split::attachments::materialize`]"),
        "the doc link still names the old parent:\n{audit}"
    );
    assert_docs_resolve(&workspace);
}
