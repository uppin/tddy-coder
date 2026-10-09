//! `extract_method` over a range holding a `return`, against a live rust-analyzer.
//!
//! A run of `return Err(..)` guards in the middle of a function becomes a function returning the
//! caller's `Result<()>`, called with `?`; a run of `return None` guards becomes `Option<()>` the
//! same way. A range that is a tail-position arm's body keeps its `return`s, which still return
//! from the caller there. Every other range holding a `return` is refused as before, naming it.
//! These are the shapes `#reshape` 16 needs for `plan/codec.rs::parse_op` and
//! `plan_store/refresh.rs::refreshed`, reproduced against the dev shell's rust-analyzer while
//! planning `#reshape` 4/19.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_workspace_holding_files, an_extract_method_of, applying_a_plan_of, assert_compiles,
    assert_lints_clean, performing, refusal_from, the_function_named, ORIGIN_LIB,
};

/// A one-crate workspace whose library is `lines`.
fn one_crate_holding(lines: &[&str]) -> harness::AFixtureWorkspace {
    a_workspace_holding_files(&[
        (
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/origin\"]\n",
        ),
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (ORIGIN_LIB, &(lines.join("\n") + "\n")),
    ])
}

/// Lines 5–12 are two error guards — an `if`, and an `if` inside a `for` — between a binding and
/// the tail, in a function returning a one-argument alias the file defines.
fn a_function_guarding_its_input() -> harness::AFixtureWorkspace {
    one_crate_holding(&[
        "pub type Res<T> = std::result::Result<T, String>;",
        "",
        "pub fn guards(a: u32, b: &str) -> Res<u32> {",
        "    let n = a + 1;",
        "    if n > 10 {",
        "        return Err(format!(\"too big {n}\"));",
        "    }",
        "    for c in b.chars() {",
        "        if c == 'x' {",
        "            return Err(\"x\".into());",
        "        }",
        "    }",
        "    Ok(n * 2)",
        "}",
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_error_guards_in_the_middle_of_a_function_becomes_a_function_returning_result_unit_called_with_a_question_mark(
) {
    // Given two error guards in the middle of a function
    let workspace = a_function_guarding_its_input();

    // When they are extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 5..=12, "guards_checked"),
    )
    .await;

    // Then they are a function returning the caller's `Res<()>`, called with `?`
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("    guards_checked(b, n)?;\n    Ok(n * 2)\n"),
        "the guards are not called with `?`:\n{lib}"
    );
    let guards_checked = the_function_named(&lib, "guards_checked");
    assert!(
        guards_checked.starts_with("fn guards_checked(b: &str, n: u32) -> Res<()> {"),
        "the lifted function does not return `Res<()>`:\n{guards_checked}"
    );
    assert!(
        guards_checked.ends_with("    Ok(())\n}"),
        "the lifted function does not end in `Ok(())`:\n{guards_checked}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lifted_function_is_spelled_with_the_callers_one_argument_result_alias() {
    // Given an error guard in a file importing a one-argument `Result` alias
    let workspace = one_crate_holding(&[
        "pub mod failures {",
        "    #[derive(Debug)]",
        "    pub struct Failure(pub String);",
        "    pub type Result<T> = std::result::Result<T, Failure>;",
        "}",
        "",
        "use crate::failures::{Failure, Result};",
        "",
        "pub fn sized(n: u32) -> Result<u32> {",
        "    let doubled = n * 2;",
        "    if n > 10 {",
        "        return Err(Failure(\"too big\".to_string()));",
        "    }",
        "    Ok(doubled)",
        "}",
    ]);

    // When the guard is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 11..=13, "small_enough"),
    )
    .await;

    // Then the lifted function returns the alias, and the tree compiles
    let small_enough = the_function_named(&workspace.read(ORIGIN_LIB), "small_enough");
    assert!(
        small_enough.starts_with("fn small_enough(n: u32) -> Result<()> {"),
        "the lifted function is not spelled through the alias:\n{small_enough}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_error_guard_inside_a_loop_is_lifted_with_its_loop() {
    // Given the guard inside the `for`
    let workspace = a_function_guarding_its_input();

    // When the loop alone is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 8..=12, "no_x"),
    )
    .await;

    // Then the loop is a function returning `Res<()>`, called with `?`
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("    no_x(b)?;\n"),
        "the loop is not called with `?`:\n{lib}"
    );
    assert!(
        the_function_named(&lib, "no_x").starts_with("fn no_x(b: &str) -> Res<()> {"),
        "the loop's function does not return `Res<()>`:\n{lib}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_none_guards_in_an_option_function_is_lifted_to_option_unit() {
    // Given two `return None` guards in a function returning `Option`
    let workspace = one_crate_holding(&[
        "pub fn firsts(a: Option<u32>, b: Option<u32>) -> Option<u32> {",
        "    let n = a?;",
        "    if n == 0 {",
        "        return None;",
        "    }",
        "    if b == Some(n) {",
        "        return None;",
        "    }",
        "    Some(n + 1)",
        "}",
    ]);

    // When they are extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 3..=8, "usable"),
    )
    .await;

    // Then they are a function returning `Option<()>`, called with `?`
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("    usable(n, b)?;\n"),
        "the guards are not called with `?`:\n{lib}"
    );
    let usable = the_function_named(&lib, "usable");
    assert!(
        usable.starts_with("fn usable(n: u32, b: Option<u32>) -> Option<()> {")
            && usable.ends_with("    Some(())\n}"),
        "the lifted function is not `Option<()>`:\n{usable}"
    );
    assert_compiles(&workspace);
}

/// Line 4 returns a value from the middle of the function; lines 3–5 are the range.
fn a_function_returning_a_value_early() -> harness::AFixtureWorkspace {
    one_crate_holding(&[
        "pub fn valued(x: bool) -> Result<u32, String> {",
        "    let base = 2;",
        "    if x {",
        "        return Ok(1);",
        "    }",
        "    Ok(base)",
        "}",
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mid_function_range_returning_a_value_is_still_refused_naming_the_return() {
    // Given a range returning a value from the middle of the function
    let workspace = a_function_returning_a_value_early();

    // When it is extracted
    let refusal = refusal_from(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 3..=5, "early"),
    )
    .await;

    // Then it is refused as before, naming the return
    assert!(
        refusal.starts_with(
            "this seam cannot be cut here: the range returns early from the function around it, \
             on line 4 (`return Ok(1);`)."
        ),
        "the refusal does not name the value return:\n{refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_mixing_error_guards_and_a_value_return_is_refused_naming_the_value_return_and_nothing_is_written(
) {
    // Given an error guard followed by a value return
    let workspace = one_crate_holding(&[
        "pub fn mixed(x: bool, y: bool) -> Result<u32, String> {",
        "    let base = 2;",
        "    if x {",
        "        return Err(\"x\".into());",
        "    }",
        "    if y {",
        "        return Ok(3);",
        "    }",
        "    Ok(base)",
        "}",
    ]);
    let before = workspace.read(ORIGIN_LIB);

    // When both are extracted
    let refusal = refusal_from(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 3..=8, "mixed_exits"),
    )
    .await;

    // Then the refusal names only the value return, and nothing is written
    assert_eq!(
        refusal,
        "this seam cannot be cut here: the range holds error guards and a return of a value, on \
         line 7 (`return Ok(3);`). Only a run whose every return is `return Err(..)` can become a \
         function called with `?`; end the range before line 7."
    );
    assert_eq!(workspace.read(ORIGIN_LIB), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guard_run_whose_binding_is_read_after_it_is_refused_naming_the_binding() {
    // Given a run that binds `doubled`, which the tail reads
    let workspace = one_crate_holding(&[
        "pub fn bound(x: u32) -> Result<u32, String> {",
        "    let doubled = x * 2;",
        "    if doubled > 9 {",
        "        return Err(\"big\".into());",
        "    }",
        "    Ok(doubled)",
        "}",
    ]);

    // When the binding and its guard are extracted
    let refusal = refusal_from(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 2..=5, "doubled_checked"),
    )
    .await;

    // Then the refusal names the binding
    assert_eq!(
        refusal,
        "this seam cannot be cut here: the range's guards are followed by code that reads \
         `doubled`, which the range declares. Cut the run so it ends before `let doubled`."
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_that_is_a_tail_match_arms_body_is_extracted_with_its_returns_verbatim() {
    // Given a tail `match` whose arm returns early twice — the shape of `refreshed`'s `Item` arm
    let workspace = one_crate_holding(&[
        "pub type Res<T> = std::result::Result<T, String>;",
        "",
        "pub fn arms(k: u8, v: Option<u32>) -> Res<u32> {",
        "    match k {",
        "        0 => Ok(1),",
        "        _ => {",
        "            let base = k as u32;",
        "            let Some(found) = v else {",
        "                return Ok(base);",
        "            };",
        "            if found > 9 {",
        "                return Err(\"big\".into());",
        "            }",
        "            Ok(base + found)",
        "        }",
        "    }",
        "}",
    ]);

    // When the arm's body is extracted
    performing(
        &workspace,
        an_extract_method_of(&workspace, ORIGIN_LIB, 7..=14, "arm_value"),
    )
    .await;

    // Then the call is the arm's value and the returns are as written, under the caller's alias
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("        _ => {\n            arm_value(k, v)\n        }"),
        "the call is not the arm's value:\n{lib}"
    );
    let arm_value = the_function_named(&lib, "arm_value");
    assert!(
        arm_value.starts_with("fn arm_value(k: u8, v: Option<u32>) -> Res<u32> {")
            && arm_value.contains("return Ok(base);")
            && arm_value.contains("return Err(\"big\".into());"),
        "the returns were not kept as written:\n{arm_value}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parse_op_shaped_guard_run_passes_clippy_with_warnings_denied() {
    // Given `parse_op`'s shape: a guard inside a `for`, then two `if` guards, under the crate's alias
    let workspace = one_crate_holding(&[
        "#[derive(Debug)]",
        "pub struct Failure(pub String);",
        "",
        "pub type Result<T> = std::result::Result<T, Failure>;",
        "",
        "fn malformed(why: &str) -> Failure {",
        "    Failure(why.to_string())",
        "}",
        "",
        "const CODE_FIELDS: [&str; 2] = [\"code\", \"text\"];",
        "",
        "pub fn parse_op(fields: &[&str], name: Option<&str>, to_file: bool) -> Result<usize> {",
        "    for field in CODE_FIELDS {",
        "        if fields.contains(&field) {",
        "            return Err(malformed(\"code in the plan\"));",
        "        }",
        "    }",
        "    if name.is_none() {",
        "        return Err(malformed(\"needs a name\"));",
        "    }",
        "    if to_file && fields.is_empty() {",
        "        return Err(malformed(\"to_file needs fields\"));",
        "    }",
        "    Ok(fields.len())",
        "}",
    ]);

    // When the guards are applied through the runner, tidy included
    let applied = applying_a_plan_of(
        &workspace,
        &[an_extract_method_of(
            &workspace,
            ORIGIN_LIB,
            13..=23,
            "refuse_unhonourable",
        )],
    )
    .await;

    // Then the run succeeded, the guards are one call with `?`, and the workspace lints clean
    assert!(applied.is_ok(), "the apply failed: {applied:?}");
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("    refuse_unhonourable(fields, name, to_file)?;\n    Ok(fields.len())\n"),
        "the guards are not one call with `?`:\n{lib}"
    );
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guard_run_that_also_propagates_with_a_question_mark_is_lifted_and_called_with_a_question_mark(
) {
    // Given `?` calls around an error guard, in a file importing a one-argument `Result` alias
    let workspace = one_crate_holding(&[
        "pub mod aliased {",
        "    pub type Result<T> = std::result::Result<T, String>;",
        "}",
        "",
        "use crate::aliased::Result;",
        "",
        "fn check_len(b: &str) -> Result<()> {",
        "    if b.is_empty() { Err(\"empty\".to_string()) } else { Ok(()) }",
        "}",
        "",
        "pub fn mixed_q(a: u32, b: &str) -> Result<u32> {",
        "    let n = a + 1;",
        "    check_len(b)?;",
        "    if n > 10 {",
        "        return Err(\"big\".into());",
        "    }",
        "    check_len(&b[1..])?;",
        "    Ok(n)",
        "}",
    ]);

    // When the run is applied through the runner, tidy included
    let applied = applying_a_plan_of(
        &workspace,
        &[an_extract_method_of(
            &workspace,
            ORIGIN_LIB,
            13..=17,
            "checked_twice",
        )],
    )
    .await;

    // Then it is one call with `?` to a function returning the caller's alias, and lints clean
    assert!(applied.is_ok(), "the apply failed: {applied:?}");
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("    checked_twice(b, n)?;\n    Ok(n)\n"),
        "the run is not one call with `?`:\n{lib}"
    );
    assert!(
        the_function_named(&lib, "checked_twice")
            .starts_with("fn checked_twice(b: &str, n: u32) -> Result<()> {"),
        "the lifted function is not spelled through the alias:\n{lib}"
    );
    assert_lints_clean(&workspace);
}
