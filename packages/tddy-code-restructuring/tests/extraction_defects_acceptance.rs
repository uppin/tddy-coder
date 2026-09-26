//! Four extraction defects, each with the fixture its backlog entry sketched: a probe that waits for
//! ever on a range opening with `&`, a borrowed field hoisted by value, an early `return` before a
//! unit tail rewritten into `ControlFlow`, and a function-local `use` left behind.
//!
//! Every one is either refused before any edit — with the class that names the cause — or applied
//! and compiled. Against a live rust-analyzer; one server at a time.

mod harness;

use std::time::{Duration, Instant};

use harness::{
    a_workspace_holding_files, an_extract_method_of, an_extract_variable_of, assert_compiles,
    performing, resolving, ORIGIN_LIB,
};

/// A ready index answers a hover in seconds; this is far under the harness's own three-minute
/// ceiling, so a probe that waits for ever fails here as a hang rather than as the ceiling.
const A_READY_INDEX_ANSWERS_WITHIN: Duration = Duration::from_secs(60);

fn one_crate_holding(lib: &str) -> harness::AFixtureWorkspace {
    a_workspace_holding_files(&[
        (
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/origin\"]\n",
        ),
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (ORIGIN_LIB, lib),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn extracting_a_range_that_opens_with_a_borrow_resolves_within_the_ready_bound() {
    // Given a method whose `let` binds `&self.v`
    let workspace = one_crate_holding(
        "pub struct S {\n    v: Option<u32>,\n}\n\nimpl S {\n    pub fn f(&self) -> bool {\n        \
         let r = &self.v;\n        r.is_some()\n    }\n}\n",
    );
    let started = Instant::now();

    // When `&self.v` is extracted into a variable
    let resolved = resolving(
        &workspace,
        an_extract_variable_of(&workspace, ORIGIN_LIB, 7, "&self.v", "held"),
    )
    .await;

    // Then it resolves, and within the time a ready index takes
    assert!(resolved.is_ok(), "the extraction was refused: {resolved:?}");
    assert!(
        started.elapsed() < A_READY_INDEX_ANSWERS_WITHIN,
        "the extraction took {:?}",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extracting_a_borrowed_field_binds_the_borrow_and_compiles() {
    // Given a method reading `self.v` under `&` in an `if let`
    let workspace = one_crate_holding(
        "pub struct S {\n    v: Option<String>,\n}\n\nimpl S {\n    pub fn f(&self) -> usize {\n        \
         if let Some(s) = &self.v {\n            s.len()\n        } else {\n            0\n        }\n    \
         }\n}\n",
    );

    // When `self.v` is extracted into a variable
    performing(
        &workspace,
        an_extract_variable_of(&workspace, ORIGIN_LIB, 7, "self.v", "value"),
    )
    .await;

    // Then the binding is the borrow, and the tree compiles
    assert!(
        workspace.read(ORIGIN_LIB).contains("let value = &self.v;"),
        "the field was not bound by reference:\n{}",
        workspace.read(ORIGIN_LIB)
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_with_an_early_return_ending_in_a_unit_if_is_refused_before_any_edit() {
    // Given a `()` function whose `let … else { return; }` is followed by a unit `if` tail
    let lib =
        "pub fn f(x: Option<u32>) {\n    let Some(v) = x else {\n        return;\n    };\n    \
               if v > 1 {\n        println!(\"{v}\");\n    }\n}\n";
    let workspace = one_crate_holding(lib);

    // When the range from the `let` to the end is extracted
    let refused = resolving(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 2..=7, "report"),
    )
    .await;

    // Then the early-return refusal that already guards a mid-function range reaches this one too,
    // naming the `return`, and nothing is written — a prefix, because the rest is its remedy text
    assert!(
        refused
            .expect_err("a unit tail after an early return is refused")
            .starts_with(
                "this seam cannot be cut here: the range returns early from the function around \
                 it, on line 3 (`return;`)"
            ),
        "the refusal did not name the early return"
    );
    assert_eq!(workspace.read(ORIGIN_LIB), lib);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_function_local_use_is_carried_into_the_extracted_function() {
    // Given a function that imports `BTreeMap` inside its own body
    let workspace = one_crate_holding(
        "pub fn build() -> usize {\n    use std::collections::BTreeMap;\n    \
         let map: BTreeMap<u32, u32> = BTreeMap::new();\n    map.len()\n}\n",
    );

    // When the line naming it is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 3..=4, "counted"),
    )
    .await;

    // Then the extracted function can name `BTreeMap`, and the tree compiles
    assert_compiles(&workspace);
}
