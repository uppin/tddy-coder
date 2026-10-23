//! `apply_code_action` against a live rust-analyzer: Rust takes the same generic code-action path
//! an extension language's server does (`#reshape` extension-code-actions, PRD F2 a).
//!
//! One test on purpose (the 2026-10-17 cap on live suites): every rule of the path is pinned
//! against a server double in `backends::code_action`'s unit tests and against the fake LSP in
//! `tddy-zed-extensions`; this proves only that rust-analyzer's own action, chosen by its title,
//! comes out as an edit the run applies and that compiles.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{a_crate_whose_method_reads_a_field, assert_compiles, performing, ORIGIN_LIB};
use tddy_code_restructuring::RefactorOp;

/// `self.clones.iter().copied().max().unwrap_or(0)`: line 9 of the fixture, columns 9 to 55.
const THE_EXPRESSION_LINE: u32 = 9;

/// What line 9 of the fixture reads once the expression is extracted: rust-analyzer extracts an
/// expression that reads `self` into a method, and calls it in the expression's place.
const THE_CALL_SITE: &str = "        self.fun_name()";

/// `apply_code_action` of rust-analyzer's `Extract into function` over the whole expression.
fn an_extract_into_function_by_title() -> RefactorOp {
    serde_json::from_value(serde_json::json!({
        "op": "apply_code_action",
        "anchor": {
            "kind": "range",
            "file": ORIGIN_LIB,
            "start": { "line": THE_EXPRESSION_LINE, "col": 9 },
            "end": { "line": THE_EXPRESSION_LINE, "col": 55 },
        },
        "action": { "title": "Extract into function" },
    }))
    .expect("the op parses")
}

#[tokio::test(flavor = "multi_thread")]
async fn extract_function_by_title_on_a_rust_file() {
    // Given a crate whose method body is one expression
    let workspace = a_crate_whose_method_reads_a_field();

    // When rust-analyzer's `Extract into function` is applied by its title
    performing(&workspace, an_extract_into_function_by_title()).await;

    // Then the method calls a new function holding the expression, and the crate compiles
    let lib = workspace.read(ORIGIN_LIB);
    assert_eq!(
        lib.matches("fn fun_name(").count(),
        1,
        "the expression was not extracted into exactly one function:\n{lib}"
    );
    assert_eq!(
        lib.matches("fun_name(").count(),
        2,
        "the extracted function is not called exactly once:\n{lib}"
    );
    assert_eq!(
        lib.lines().nth(THE_EXPRESSION_LINE as usize - 1),
        Some(THE_CALL_SITE),
        "the method's body is not the call:\n{lib}"
    );
    assert_compiles(&workspace);
}
