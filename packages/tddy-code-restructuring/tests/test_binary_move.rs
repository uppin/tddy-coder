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
    read_test_binary_move, resolve_test_binary_move, Anchor, FileEdit, Overlay, Plan, Reexport,
    RefactorKind, WorkspaceEdit,
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

/// A `mod` the moved test declares is that binary's own module, not a crate it depends on.
///
/// Six suites under `packages/tddy-daemon/tests/` open with `mod common;` and then reach their
/// helpers as `common::…`. Cargo compiles each `tests/*.rs` as a crate root of its own, so that
/// path names an item of the binary itself — `tests/common/mod.rs` — exactly as `crate::` does.
/// Resolving it as an extern crate is a category error, and it refused a correct plan: `common` is
/// declared in no manifest, so the carry-across had nothing to find.
#[test]
fn leaves_a_module_the_moved_test_declares_itself_out_of_the_destination() {
    // Given a test binary that declares `mod common;` and reaches both it and a real crate
    let directory = a_workspace_whose_moved_test_declares_a_module();
    let overlay = Overlay::new();
    let workspace = Workspace {
        root: directory.path(),
        overlay: &overlay,
    };
    let moving = read_test_binary_move(&workspace, &a_move_of(MOVED_TEST, DESTINATION))
        .expect("the anchor is a test binary");

    // When the move is resolved
    let edit = resolve_test_binary_move(&workspace, &moving)
        .expect("a test declaring a module of its own resolves");

    // Then the destination gains only the crate the test genuinely names
    assert_eq!(
        added_to(&edit, &format!("{DESTINATION}/Cargo.toml")),
        "\n[dev-dependencies]\ntokio = { version = \"1\" }\n",
        "the module the moved test declares itself was read as an extern crate"
    );
}

const MOVED_TEST: &str = "packages/daemon/tests/claude_cli_permission_mode_acceptance.rs";
const DESTINATION: &str = "packages/session-lifecycle";

/// Two crates and the test moving between them: the daemon whose library merely re-exports
/// `claude_cli`, and the crate that defines it.
///
/// The moved test names three things, and the pass owes each a different answer: `common`, its own
/// module, which is not a dependency at all; `daemon::claude_cli`, a facade to be re-pointed at the
/// destination; and `tokio`, a dev-dependency that has to travel.
fn a_workspace_whose_moved_test_declares_a_module() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");

    for (relative, text) in [
        (
            "packages/daemon/Cargo.toml",
            "[package]\nname = \"daemon\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nsession-lifecycle = { path = \"../session-lifecycle\" }\n\n\
             [dev-dependencies]\ntokio = { version = \"1\" }\n",
        ),
        (
            "packages/daemon/src/lib.rs",
            "//! The crate the suite is leaving, which only passes `claude_cli` on.\n\n\
             pub use session_lifecycle::claude_cli;\n",
        ),
        (
            "packages/daemon/tests/common/mod.rs",
            "//! Helpers the suites share, compiled into each one that declares it.\n\n\
             pub const PTY_STUB_OUTPUT: &str = \"ready\";\n\n\
             pub fn a_capture_showing(output: &str) -> String {\n    output.to_string()\n}\n",
        ),
        (
            MOVED_TEST,
            "mod common;\n\n\
             use common::{a_capture_showing, PTY_STUB_OUTPUT};\n\
             use daemon::claude_cli::PermissionMode;\n\
             use tokio::time::Duration;\n\n\
             #[test]\nfn reports_the_mode_it_was_launched_with() {\n    \
             assert_eq!(\n        \
             PermissionMode::of(&a_capture_showing(PTY_STUB_OUTPUT), Duration::ZERO),\n        \
             PermissionMode::Plan,\n    );\n}\n",
        ),
        (
            "packages/session-lifecycle/Cargo.toml",
            "[package]\nname = \"session-lifecycle\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "packages/session-lifecycle/src/lib.rs",
            "//! The crate that defines what the suite exercises.\n\npub mod claude_cli;\n",
        ),
    ] {
        let absolute = directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent directory"))
            .expect("the directory is created");
        std::fs::write(absolute, text).expect("the file is written");
    }
    directory
}

fn a_move_of(source: &str, to: &str) -> tddy_code_restructuring::RefactorOp {
    tddy_code_restructuring::RefactorOp {
        op: RefactorKind::MoveTestBinaryToCrate,
        anchor: Anchor::Symbol {
            file: source.to_string(),
            path: "whatever".to_string(),
        },
        name: None,
        to: Some(to.to_string()),
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
    }
}

/// The text one file gains from a resolved edit, which for a manifest is its new dependency lines.
fn added_to(edit: &WorkspaceEdit, path: &str) -> String {
    edit.changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Change {
                path: changed,
                edits,
            } if changed == path => Some(edits),
            _ => None,
        })
        .flatten()
        .map(|edit| edit.new_text.clone())
        .collect()
}
