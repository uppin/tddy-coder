//! Signature changes and the call-site operations that repair their callers, against a live
//! rust-analyzer.
//!
//! `change_param_type`, `add_param`, `reorder_params` and `change_return_type` edit the declaration
//! only, so on their own they break every caller. Each caller is then its own operation —
//! `add_call_arg`, `remove_call_arg`, `change_call_arg`, `reorder_call_args`, each anchored on one
//! call expression — and a transactional group makes the set one unit that only has to compile at
//! its end. That is the claim under test: every group here is a signature change plus one call-site
//! operation per caller, applied through the runner, and judged on the files and the build after.
//!
//! The group's gate and rollback are `transactional-groups`'; these tests consume them.
//!
//! Load-sensitive: one server at a time, enforced by the harness.

mod harness;

use harness::{a_workspace_holding_files, an_item_anchor, applying_a_plan_of, at};
use tddy_code_restructuring::{Anchor, OrderKey, RefactorKind, RefactorOp};

const WORKSPACE_MANIFEST: &str = "[workspace]\nresolver = \"2\"\nmembers = [\"crates/ledger\"]\n";
const LEDGER_MANIFEST: &str =
    "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LEDGER_LIB: &str = "crates/ledger/src/lib.rs";
const PRICING: &str = "crates/ledger/src/pricing.rs";
const CHECKOUT: &str = "crates/ledger/src/checkout.rs";

/// `label`, taking a count.
const LABEL: &str = "pub fn label(count: u32) -> String {\n    format!(\"{count} items\")\n}";

/// `checkout::basket`, calling `label` on its second line, columns 5–12.
const BASKET_CALLING_LABEL: &str = "pub fn basket() -> String {\n    label(3)\n}";

/// The crate root's `sample`, calling `pricing::label` on its second line, columns 5–21.
const SAMPLE_CALLING_LABEL: &str = "pub fn sample() -> String {\n    pricing::label(1)\n}";

/// `total`, whose three parameters are all `u32` — a reorder that forgot a caller would still
/// compile, so the files are what tells.
const TOTAL: &str =
    "pub fn total(price: u32, quantity: u32, discount: u32) -> u32 {\n    price * quantity - discount\n}";

/// `checkout::basket`, calling `total` on its second line, columns 5–18.
const BASKET_CALLING_TOTAL: &str = "pub fn basket() -> u32 {\n    total(3, 4, 1)\n}";

/// The crate root's `sample`, calling `pricing::total` on its second line, columns 5–27.
const SAMPLE_CALLING_TOTAL: &str = "pub fn sample() -> u32 {\n    pricing::total(2, 5, 3)\n}";

/// `product`, returning a bare `u32`.
const PRODUCT: &str = "pub fn product(price: u32, quantity: u32) -> u32 {\n    price * quantity\n}";

/// A `ledger` crate holding `pricing`, `checkout` and a root that declares both, followed by `root`
/// when it holds anything.
fn a_ledger(pricing: &str, checkout: &str, root: &str) -> harness::AFixtureWorkspace {
    let mut lib = "pub mod checkout;\npub mod pricing;\n".to_string();
    if !root.is_empty() {
        lib.push_str(&format!("\n{root}\n"));
    }
    a_workspace_holding_files(&[
        ("Cargo.toml", WORKSPACE_MANIFEST),
        ("crates/ledger/Cargo.toml", LEDGER_MANIFEST),
        (LEDGER_LIB, &lib),
        (PRICING, &format!("{pricing}\n")),
        (CHECKOUT, &format!("{checkout}\n")),
    ])
}

/// `pricing::label`, called from `checkout::basket` and the crate root's `sample`.
fn a_ledger_whose_label_is_called_twice() -> harness::AFixtureWorkspace {
    a_ledger(
        LABEL,
        &format!("use crate::pricing::label;\n\n{BASKET_CALLING_LABEL}"),
        SAMPLE_CALLING_LABEL,
    )
}

/// `pricing::total`, called from `checkout::basket` and the crate root's `sample`.
fn a_ledger_whose_total_is_called_twice() -> harness::AFixtureWorkspace {
    a_ledger(
        TOTAL,
        &format!("use crate::pricing::total;\n\n{BASKET_CALLING_TOTAL}"),
        SAMPLE_CALLING_TOTAL,
    )
}

/// `pricing::label` and `pricing::product`, whose one caller discards what each returns — so a new
/// return type breaks no caller.
fn a_ledger_whose_callers_discard_what_they_get() -> harness::AFixtureWorkspace {
    a_ledger(
        &format!("{LABEL}\n\n{PRODUCT}"),
        "use crate::pricing::{label, product};\n\npub fn basket() {\n    \
         let _ = label(3);\n    let _ = product(3, 4);\n}",
        "",
    )
}

/// An operation of `kind` at `anchor`, in `group` when it names one; every other field absent.
fn an_op(kind: RefactorKind, anchor: Anchor, group: Option<&str>) -> RefactorOp {
    RefactorOp {
        id: None,
        op: kind,
        anchor,
        name: None,
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
        group: group.map(str::to_string),
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
    }
}

/// An anchor on the function `item` in `PRICING` itself, fingerprinted over `item_text`.
fn the_function(item: &str, item_text: &str) -> Anchor {
    an_item_anchor(PRICING, item, item_text, None, None)
}

/// An anchor on the call on line 2 of `item` (in `file`), columns `from` up to `to`.
fn the_call_in(file: &str, item: &str, item_text: &str, from: u32, to: u32) -> Anchor {
    an_item_anchor(file, item, item_text, Some((at(2, from), at(2, to))), None)
}

/// The call to `label` in `checkout::basket`.
fn the_label_call_in_basket() -> Anchor {
    the_call_in(
        CHECKOUT,
        "ledger::checkout::basket",
        BASKET_CALLING_LABEL,
        5,
        13,
    )
}

/// The call to `pricing::label` in the crate root's `sample`.
fn the_label_call_in_sample() -> Anchor {
    the_call_in(LEDGER_LIB, "ledger::sample", SAMPLE_CALLING_LABEL, 5, 22)
}

/// Retyping `label`'s `count` to `type_`, in `group`.
fn retyping_count_to(type_: &str, group: &str) -> RefactorOp {
    RefactorOp {
        name: Some("count".to_string()),
        type_: Some(type_.to_string()),
        ..an_op(
            RefactorKind::ChangeParamType,
            the_function("ledger::pricing::label", LABEL),
            Some(group),
        )
    }
}

/// Replacing the first argument of the call at `call` with `expr`, in `group`.
fn passing_first(call: Anchor, expr: &str, group: &str) -> RefactorOp {
    RefactorOp {
        variant: Some("first".to_string()),
        expr: Some(expr.to_string()),
        ..an_op(RefactorKind::ChangeCallArg, call, Some(group))
    }
}

/// Every source file of the ledger with its exact text.
fn the_ledger(workspace: &harness::AFixtureWorkspace) -> [String; 3] {
    [
        workspace.read(PRICING),
        workspace.read(CHECKOUT),
        workspace.read(LEDGER_LIB),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_param_type_change_and_its_call_site_changes_compile_as_one_group() {
    // Given `label(count: u32)`, called as `label(3)` and `pricing::label(1)`
    let workspace = a_ledger_whose_label_is_called_twice();

    // When `count` becomes `&str` and both calls pass a string, as one group
    let summary = applying_a_plan_of(
        &workspace,
        &[
            retyping_count_to("&str", "retype"),
            passing_first(the_label_call_in_basket(), "\"three\"", "retype"),
            passing_first(the_label_call_in_sample(), "\"one\"", "retype"),
        ],
    )
    .await;

    // Then the declaration and both calls change, and the tree compiles at the group's end
    assert_eq!(summary.map(|run| run.applied), Ok(3));
    assert_eq!(
        the_ledger(&workspace),
        [
            "pub fn label(count: &str) -> String {\n    format!(\"{count} items\")\n}\n"
                .to_string(),
            "use crate::pricing::label;\n\npub fn basket() -> String {\n    label(\"three\")\n}\n"
                .to_string(),
            "pub mod checkout;\npub mod pricing;\n\npub fn sample() -> String {\n    \
             pricing::label(\"one\")\n}\n"
                .to_string(),
        ]
    );
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_missing_one_call_site_is_rolled_back_naming_its_error() {
    // Given `label(count: u32)`, called from `checkout` and from the crate root
    let workspace = a_ledger_whose_label_is_called_twice();
    let before = the_ledger(&workspace);

    // When `count` becomes `&str` and only `checkout`'s call is repaired, as one group
    let refusal = applying_a_plan_of(
        &workspace,
        &[
            retyping_count_to("&str", "retype"),
            passing_first(the_label_call_in_basket(), "\"three\"", "retype"),
        ],
    )
    .await
    .map(|run| run.applied)
    .expect_err("a group whose end does not compile fails the run");

    // Then the group is rolled back, and the refusal carries the unrepaired caller's error
    assert!(
        refusal.starts_with("group `retype` does not compile at its end, so it was rolled back:"),
        "the refusal does not name the group that was rolled back:\n{refusal}"
    );
    // rustc's own wording, quoted from the check: the mismatch, in the file holding the caller.
    assert!(
        refusal.contains("mismatched types") && refusal.contains("src/lib.rs"),
        "the refusal does not carry the crate root's mismatched-types error:\n{refusal}"
    );
    assert_eq!(the_ledger(&workspace), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn reorder_params_and_reorder_call_args_compile_as_one_group() {
    // Given `total(price, quantity, discount)`, called as `total(3, 4, 1)` and
    // `pricing::total(2, 5, 3)`
    let workspace = a_ledger_whose_total_is_called_twice();
    let moving_discount_first = RefactorOp {
        order: ["discount", "price", "quantity"]
            .map(|name| OrderKey::Name(name.to_string()))
            .to_vec(),
        ..an_op(
            RefactorKind::ReorderParams,
            the_function("ledger::pricing::total", TOTAL),
            Some("reorder"),
        )
    };
    let moving_the_third_argument_first = |call: Anchor| RefactorOp {
        order: vec![
            OrderKey::Position(3),
            OrderKey::Position(1),
            OrderKey::Position(2),
        ],
        ..an_op(RefactorKind::ReorderCallArgs, call, Some("reorder"))
    };

    // When `discount` moves first, and both calls move their third argument first, as one group
    let summary = applying_a_plan_of(
        &workspace,
        &[
            moving_discount_first,
            moving_the_third_argument_first(the_call_in(
                CHECKOUT,
                "ledger::checkout::basket",
                BASKET_CALLING_TOTAL,
                5,
                19,
            )),
            moving_the_third_argument_first(the_call_in(
                LEDGER_LIB,
                "ledger::sample",
                SAMPLE_CALLING_TOTAL,
                5,
                28,
            )),
        ],
    )
    .await;

    // Then the declaration and both calls are reordered alike, and the tree compiles
    assert_eq!(summary.map(|run| run.applied), Ok(3));
    assert_eq!(
        the_ledger(&workspace),
        [
            "pub fn total(discount: u32, price: u32, quantity: u32) -> u32 {\n    \
             price * quantity - discount\n}\n"
                .to_string(),
            "use crate::pricing::total;\n\npub fn basket() -> u32 {\n    total(1, 3, 4)\n}\n"
                .to_string(),
            "pub mod checkout;\npub mod pricing;\n\npub fn sample() -> u32 {\n    \
             pricing::total(3, 2, 5)\n}\n"
                .to_string(),
        ]
    );
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn add_param_and_add_call_arg_compile_as_one_group() {
    // Given `label(count: u32)`, called as `label(3)` and `pricing::label(1)`
    let workspace = a_ledger_whose_label_is_called_twice();
    let adding_unit = RefactorOp {
        name: Some("unit".to_string()),
        type_: Some("&str".to_string()),
        variant: Some("last".to_string()),
        ..an_op(
            RefactorKind::AddParam,
            the_function("ledger::pricing::label", LABEL),
            Some("unit"),
        )
    };
    let passing_items_last = |call: Anchor| RefactorOp {
        variant: Some("last".to_string()),
        expr: Some("\"items\"".to_string()),
        ..an_op(RefactorKind::AddCallArg, call, Some("unit"))
    };

    // When `unit: &str` is added last, and both calls pass `"items"` last, as one group
    let summary = applying_a_plan_of(
        &workspace,
        &[
            adding_unit,
            passing_items_last(the_label_call_in_basket()),
            passing_items_last(the_label_call_in_sample()),
        ],
    )
    .await;

    // Then the declaration gains the parameter, each call the argument, and the tree compiles
    assert_eq!(summary.map(|run| run.applied), Ok(3));
    assert_eq!(
        the_ledger(&workspace),
        [
            "pub fn label(count: u32, unit: &str) -> String {\n    format!(\"{count} items\")\n}\n"
                .to_string(),
            "use crate::pricing::label;\n\npub fn basket() -> String {\n    \
             label(3, \"items\")\n}\n"
                .to_string(),
            "pub mod checkout;\npub mod pricing;\n\npub fn sample() -> String {\n    \
             pricing::label(1, \"items\")\n}\n"
                .to_string(),
        ]
    );
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn change_return_type_wrap_result_uses_the_assist() {
    // Given `product` returning `u32`, whose caller discards what it gets
    let workspace = a_ledger_whose_callers_discard_what_they_get();
    let the_product_function = || the_function("ledger::pricing::product", PRODUCT);
    let wrapping_in_result = RefactorOp {
        variant: Some("wrap_result".to_string()),
        ..an_op(
            RefactorKind::ChangeReturnType,
            the_product_function(),
            Some("fallible"),
        )
    };
    // The assist leaves the error type for the author to choose; the group chooses it.
    let naming_the_error = RefactorOp {
        type_: Some("Result<u32, String>".to_string()),
        ..an_op(
            RefactorKind::ChangeReturnType,
            the_product_function(),
            Some("fallible"),
        )
    };

    // When its return is wrapped in `Result` by the assist, and the error type named, as one group
    let summary = applying_a_plan_of(&workspace, &[wrapping_in_result, naming_the_error]).await;

    // Then the function's own tail is wrapped in `Ok` — the assist's work, which a declaration-only
    // edit never does — and the tree compiles
    assert_eq!(summary.map(|run| run.applied), Ok(2));
    assert_eq!(
        workspace.read(PRICING),
        format!(
            "{LABEL}\n\npub fn product(price: u32, quantity: u32) -> Result<u32, String> {{\n    \
             Ok(price * quantity)\n}}\n"
        )
    );
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn change_return_type_with_a_type_edits_the_declaration() {
    // Given `label` returning `String`, whose caller discards what it gets
    let workspace = a_ledger_whose_callers_discard_what_they_get();
    let callers_before = workspace.read(CHECKOUT);
    let returning_anything_displayable = RefactorOp {
        type_: Some("impl std::fmt::Display".to_string()),
        ..an_op(
            RefactorKind::ChangeReturnType,
            the_function("ledger::pricing::label", LABEL),
            None,
        )
    };

    // When its return type becomes `impl std::fmt::Display`
    let summary = applying_a_plan_of(&workspace, &[returning_anything_displayable]).await;

    // Then only the declaration's `-> …` changes: the body and the caller are as they were
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    assert_eq!(
        workspace.read(PRICING),
        format!(
            "pub fn label(count: u32) -> impl std::fmt::Display {{\n    \
             format!(\"{{count}} items\")\n}}\n\n{PRODUCT}\n"
        )
    );
    assert_eq!(workspace.read(CHECKOUT), callers_before);
    harness::assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_site_op_whose_range_is_not_a_call_is_refused() {
    // Given `checkout::basket`, whose first line is its signature and holds no call
    let workspace = a_ledger_whose_label_is_called_twice();
    let before = the_ledger(&workspace);
    let on_the_name_of_basket = an_item_anchor(
        CHECKOUT,
        "ledger::checkout::basket",
        BASKET_CALLING_LABEL,
        Some((at(1, 8), at(1, 14))),
        None,
    );

    // When a call argument is changed at the function's name
    let refusal = applying_a_plan_of(
        &workspace,
        &[RefactorOp {
            variant: Some("first".to_string()),
            expr: Some("\"three\"".to_string()),
            ..an_op(RefactorKind::ChangeCallArg, on_the_name_of_basket, None)
        }],
    )
    .await
    .map(|run| run.applied)
    .expect_err("an operation on something that is not a call is refused");

    // Then it is refused saying so, and nothing is written
    assert!(
        refusal.contains("is not a call expression"),
        "the refusal does not say the range is not a call expression:\n{refusal}"
    );
    assert_eq!(the_ledger(&workspace), before);
}
