//! Moving a test binary to the crate whose code it exercises.
//!
//! `tddy-daemon` is 58,693 lines, of which 2,377 are production code; the rest is 139 integration
//! test binaries, and **122 of them do not test that crate**. They stayed behind through ten
//! `#unbundle` nodes because `tddy-daemon/src/lib.rs` re-exports 82 modules from
//! `tddy-session-lifecycle` — which re-exports 49 of those from ten further crates — so every suite
//! goes on compiling wherever it sits.
//!
//! `move_module_to_crate` cannot help: `source_crate_of` requires `<crate>/src/<module>.rs`. This
//! operation admits the other shape, and is simpler for two reasons — cargo auto-discovers
//! `tests/*.rs`, so there is no `mod` declaration to remove, and nothing can reference a test
//! binary, so there is no facade.

use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    read_test_binary_move, Anchor, Overlay, Plan, Reexport, RefactorKind,
};

fn a_plan_line(op: &str, file: &str, extra: &str) -> String {
    format!(
        r#"{{"op":"{op}","anchor":{{"kind":"symbol","file":"{file}","path":"whatever"}}{extra}}}"#
    )
}

fn parsed(line: &str) -> Result<Plan, String> {
    let text = format!("{{\"v\":1,\"snapshot\":{{}}}}\n{line}\n");
    Plan::parse(&text).map_err(|e| e.to_string())
}

/// AC3 — a facade is refused, and the refusal says why it is meaningless rather than unsupported.
///
/// A plan author who has just written five `move_module_to_crate` operations with
/// `reexport: "glob"` will reach for it here by analogy. The distinction is not that the operation
/// declines to write one — it is that **nothing can reference a test binary**, so there is no caller
/// a facade could keep resolving.
#[test]
fn refuses_a_facade_for_a_test_binary() {
    // Given a plan asking for one
    let line = a_plan_line(
        "move_test_binary_to_crate",
        "packages/tddy-daemon/tests/session_room_acceptance.rs",
        r#","to":"packages/tddy-session-lifecycle","reexport":"glob""#,
    );

    // When it is parsed
    let refusal = parsed(&line).expect_err("a facade for a test binary is refused");

    // Then the refusal explains the shape, not just the rule
    assert!(
        refusal.contains("nothing can reference a test binary"),
        "the refusal does not say why a facade is meaningless here: {refusal}"
    );
}

/// The destination is the whole point of the operation, so its absence is a plan defect.
#[test]
fn refuses_a_move_with_no_destination() {
    // Given a plan that names no `to`
    let line = a_plan_line(
        "move_test_binary_to_crate",
        "packages/tddy-daemon/tests/session_room_acceptance.rs",
        "",
    );

    // When it is parsed
    let refusal = parsed(&line).expect_err("a move with no destination is refused");

    // Then it names the field
    assert!(
        refusal.contains("needs `to`"),
        "the refusal does not name the missing field: {refusal}"
    );
}

/// A plan naming a real test binary parses, so the refusals above are about their own cases.
#[test]
fn accepts_a_test_binary_with_a_destination() {
    // Given a well-formed move
    let line = a_plan_line(
        "move_test_binary_to_crate",
        "packages/tddy-daemon/tests/session_room_acceptance.rs",
        r#","to":"packages/tddy-session-lifecycle""#,
    );

    // When it is parsed
    let plan = parsed(&line).expect("a well-formed test-binary move parses");

    // Then the operation survives into the plan
    assert_eq!(plan.ops[0].op, RefactorKind::MoveTestBinaryToCrate);
    assert_eq!(plan.ops[0].reexport, None::<Reexport>);
}

/// AC4 — an anchor under `src/` is a module move, and is refused rather than approximated.
///
/// The two operations exist separately *because* the path shapes differ; admitting the wrong one
/// would produce an edit neither operation's rules cover — a module left declared by a crate root
/// that no longer holds it.
#[test]
fn refuses_an_anchor_that_is_not_a_test_binary() {
    // Given an anchor under `src/`
    let directory = tempfile::tempdir().expect("a temporary directory");
    let overlay = Overlay::new();
    let workspace = Workspace {
        root: directory.path(),
        overlay: &overlay,
    };
    let op = tddy_code_restructuring::RefactorOp {
        op: RefactorKind::MoveTestBinaryToCrate,
        anchor: Anchor::Symbol {
            file: "packages/tddy-daemon/src/runtime.rs".to_string(),
            path: "runtime".to_string(),
        },
        name: None,
        to: Some("packages/tddy-session-lifecycle".to_string()),
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
    };

    // When the move is read
    let refusal = read_test_binary_move(&workspace, &op)
        .expect_err("an anchor under `src/` is not a test binary");

    // Then it names the shape it wanted
    assert!(
        refusal.to_string().contains("tests/"),
        "the refusal does not name the shape it expected: {refusal}"
    );
}
