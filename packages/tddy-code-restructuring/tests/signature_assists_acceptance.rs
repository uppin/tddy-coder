//! Signature operations that rewrite their own callers, against a live rust-analyzer:
//! `remove_unused_param` and `convert_tuple_return_to_struct`.
//!
//! Both are rust-analyzer assists whose edit reaches every call site, so each leaves a compiling
//! tree on its own. That is the claim under test, and only a real server and a real `cargo check`
//! can settle it: every test here applies an item-anchored plan through the runner and asserts on
//! the files and the build afterwards.
//!
//! Load-sensitive: one server at a time, enforced by the harness.

mod harness;

use harness::{a_workspace_holding_files, an_item_anchor, applying_a_plan_of};
use tddy_code_restructuring::{Plan, RefactorKind, RefactorOp};

const WORKSPACE_MANIFEST: &str = "[workspace]\nresolver = \"2\"\nmembers = [\"crates/ledger\"]\n";
const LEDGER_MANIFEST: &str =
    "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LEDGER_LIB: &str = "crates/ledger/src/lib.rs";
const PRICING: &str = "crates/ledger/src/pricing.rs";
const CHECKOUT: &str = "crates/ledger/src/checkout.rs";

/// `total`, whose `note` the body never reads.
const TOTAL_WITH_AN_UNUSED_NOTE: &str =
    "pub fn total(price: u32, quantity: u32, note: &str) -> u32 {\n    price * quantity\n}";

/// `total`, whose `discount` the body reads.
const TOTAL_WITH_A_USED_DISCOUNT: &str =
    "pub fn total(price: u32, quantity: u32, discount: u32) -> u32 {\n    price * quantity - discount\n}";

/// `split`, returning a bare tuple.
const SPLIT_RETURNING_A_TUPLE: &str =
    "pub fn split(total: u32) -> (u32, u32) {\n    (total / 2, total - total / 2)\n}";

/// A crate whose `pricing::total` is called from two other files: `checkout` and the crate root.
fn a_ledger_whose_total_ignores_its_note() -> harness::AFixtureWorkspace {
    a_workspace_holding_files(&[
        ("Cargo.toml", WORKSPACE_MANIFEST),
        ("crates/ledger/Cargo.toml", LEDGER_MANIFEST),
        (
            LEDGER_LIB,
            "pub mod checkout;\npub mod pricing;\n\npub fn sample() -> u32 {\n    \
             pricing::total(1, 2, \"sample\")\n}\n",
        ),
        (PRICING, &format!("{TOTAL_WITH_AN_UNUSED_NOTE}\n")),
        (
            CHECKOUT,
            "use crate::pricing::total;\n\npub fn basket() -> u32 {\n    total(3, 4, \"gift\")\n}\n",
        ),
    ])
}

/// The same crate, with a `total` whose third parameter is read by its body.
fn a_ledger_whose_total_reads_its_discount() -> harness::AFixtureWorkspace {
    a_workspace_holding_files(&[
        ("Cargo.toml", WORKSPACE_MANIFEST),
        ("crates/ledger/Cargo.toml", LEDGER_MANIFEST),
        (LEDGER_LIB, "pub mod checkout;\npub mod pricing;\n"),
        (PRICING, &format!("{TOTAL_WITH_A_USED_DISCOUNT}\n")),
        (
            CHECKOUT,
            "use crate::pricing::total;\n\npub fn basket() -> u32 {\n    total(3, 4, 1)\n}\n",
        ),
    ])
}

/// A crate whose `pricing::split` returns a tuple that `checkout` destructures.
fn a_ledger_whose_split_returns_a_tuple() -> harness::AFixtureWorkspace {
    a_workspace_holding_files(&[
        ("Cargo.toml", WORKSPACE_MANIFEST),
        ("crates/ledger/Cargo.toml", LEDGER_MANIFEST),
        (LEDGER_LIB, "pub mod checkout;\npub mod pricing;\n"),
        (PRICING, &format!("{SPLIT_RETURNING_A_TUPLE}\n")),
        (
            CHECKOUT,
            "use crate::pricing::split;\n\npub fn halves() -> u32 {\n    \
             let (first, second) = split(7);\n    first * second\n}\n",
        ),
    ])
}

/// A signature operation of `kind` anchored on the item `item` (fingerprinted over `item_text`),
/// naming `name`.
fn a_signature_op(kind: RefactorKind, item: &str, item_text: &str, name: &str) -> RefactorOp {
    RefactorOp {
        id: None,
        op: kind,
        anchor: an_item_anchor(PRICING, item, item_text, None, None),
        name: Some(name.to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
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

#[tokio::test(flavor = "multi_thread")]
async fn removing_an_unused_parameter_rewrites_every_call_site_and_compiles() {
    // Given `total(price, quantity, note)`, whose body never reads `note`, called from two files
    let workspace = a_ledger_whose_total_ignores_its_note();
    let removal = a_signature_op(
        RefactorKind::RemoveUnusedParam,
        "ledger::pricing::total",
        TOTAL_WITH_AN_UNUSED_NOTE,
        "note",
    );

    // When `note` is removed
    let summary = applying_a_plan_of(&workspace, &[removal]).await;

    // Then the declaration and both callers lose it, and the tree compiles
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    assert_eq!(
        workspace.read(PRICING),
        "pub fn total(price: u32, quantity: u32) -> u32 {\n    price * quantity\n}\n"
    );
    assert_eq!(
        workspace.read(CHECKOUT),
        "use crate::pricing::total;\n\npub fn basket() -> u32 {\n    total(3, 4)\n}\n"
    );
    assert_eq!(
        workspace.read(LEDGER_LIB),
        "pub mod checkout;\npub mod pricing;\n\npub fn sample() -> u32 {\n    \
         pricing::total(1, 2)\n}\n"
    );
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_used_parameter_is_refused_naming_it() {
    // Given `total(price, quantity, discount)`, whose body reads `discount`
    let workspace = a_ledger_whose_total_reads_its_discount();
    let removal = a_signature_op(
        RefactorKind::RemoveUnusedParam,
        "ledger::pricing::total",
        TOTAL_WITH_A_USED_DISCOUNT,
        "discount",
    );

    // When `discount` is asked to be removed
    let refusal = applying_a_plan_of(&workspace, &[removal])
        .await
        .map(|run| run.applied)
        .unwrap_err();

    // Then the run is refused naming the parameter and that it is used, and nothing is written
    assert!(
        refusal.contains("`discount`") && refusal.contains("is used"),
        "the refusal must name `discount` and say it is used: {refusal}"
    );
    assert_eq!(
        workspace.read(PRICING),
        format!("{TOTAL_WITH_A_USED_DISCOUNT}\n")
    );
    assert_eq!(
        workspace.read(CHECKOUT),
        "use crate::pricing::total;\n\npub fn basket() -> u32 {\n    total(3, 4, 1)\n}\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn converting_a_tuple_return_to_a_struct_rewrites_destructuring_callers_and_compiles() {
    // Given `split` returning `(u32, u32)`, destructured by `checkout::halves`
    let workspace = a_ledger_whose_split_returns_a_tuple();
    let conversion = a_signature_op(
        RefactorKind::ConvertTupleReturnToStruct,
        "ledger::pricing::split",
        SPLIT_RETURNING_A_TUPLE,
        "Halves",
    );

    // When its return is converted into a struct called `Halves`
    let summary = applying_a_plan_of(&workspace, &[conversion]).await;

    // Then `split` returns the new `Halves`, the caller destructures `Halves`, and the tree compiles
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    let pricing = workspace.read(PRICING);
    assert!(
        pricing.contains("pub struct Halves(pub u32, pub u32);"),
        "no `Halves` tuple struct was declared:\n{pricing}"
    );
    assert!(
        pricing.contains("pub fn split(total: u32) -> Halves {"),
        "`split` does not return `Halves`:\n{pricing}"
    );
    let checkout = workspace.read(CHECKOUT);
    assert!(
        checkout.contains("let Halves(first, second) = split(7);"),
        "the caller does not destructure `Halves`:\n{checkout}"
    );
    harness::assert_compiles(&workspace);
}

#[test]
fn a_missing_name_is_refused_as_malformed() {
    // Given a parameter removal that does not say which parameter
    let plan = "{\"v\":1,\"snapshot\":{}}\n\
                {\"op\":\"remove_unused_param\",\"anchor\":{\"kind\":\"symbol\",\
                \"file\":\"crates/ledger/src/pricing.rs\",\"path\":\"total\"}}\n";

    // When the plan is parsed
    let refusal = Plan::parse(plan)
        .map(|parsed| parsed.ops.len())
        .unwrap_err();

    // Then it is refused as malformed, naming the missing field
    assert_eq!(
        refusal.to_string(),
        "plan is malformed: `remove_unused_param` needs `name`: the parameter to remove"
    );
}
