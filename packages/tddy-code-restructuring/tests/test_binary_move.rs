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

/// A path written inside a function body is an extern-crate path once the file leaves the crate.
///
/// This is where a test binary parts company with a module move. A module keeps its own `crate::`,
/// so an in-body path means the same thing after the move and is deliberately left alone. A test
/// binary takes the *whole* file out, and `daemon::project_storage` in a body names a crate the
/// destination need not depend on at all — 110 such paths survived the first pass over this
/// workspace, in files whose `use` headers were re-pointed perfectly.
#[test]
fn re_points_a_crate_path_the_moved_test_writes_inside_a_function_body() {
    // Given a suite reaching the daemon from a body, where there is no `use` declaration to rewrite
    let directory = a_workspace_whose_moved_test_reads(
        r#"use daemon::claude_cli::PermissionMode;

#[test]
fn records_the_project_it_was_launched_in() {
    daemon::project_storage::add_project("demo");
    assert_eq!(PermissionMode::Plan, PermissionMode::Plan);
}
"#,
    );

    // When the move is resolved
    let moved = the_moved_test_after_resolving(&directory);

    // Then the body path names the crate that defines what it reaches, as the header already did
    assert_eq!(
        moved,
        r#"use session_lifecycle::claude_cli::PermissionMode;

#[test]
fn records_the_project_it_was_launched_in() {
    session_lifecycle::project_storage::add_project("demo");
    assert_eq!(PermissionMode::Plan, PermissionMode::Plan);
}
"#
    );
}

/// A `use` inside `mod tests { … }` is indented, which is the one thing the header scanner rejects.
///
/// `packages/tddy-daemon-kernel/tests/relay_mode_acceptance.rs` opens its inline module with
/// `use tddy_daemon::config::RelayConfig;`. Left alone it is a declaration naming a crate the
/// destination does not depend on — the same defect as the header case, hidden one indent deeper.
#[test]
fn re_points_a_use_declaration_inside_a_nested_module() {
    // Given a `use` indented inside an inline module, which the header pass does not see
    let directory = a_workspace_whose_moved_test_reads(
        r#"mod tests {
    use daemon::claude_cli::PermissionMode;

    #[test]
    fn reports_the_mode_it_was_launched_with() {
        assert_eq!(PermissionMode::Plan, PermissionMode::Plan);
    }
}
"#,
    );

    // When the move is resolved
    let moved = the_moved_test_after_resolving(&directory);

    // Then it is re-pointed exactly as a top-level one would be
    assert_eq!(
        moved,
        r#"mod tests {
    use session_lifecycle::claude_cli::PermissionMode;

    #[test]
    fn reports_the_mode_it_was_launched_with() {
        assert_eq!(PermissionMode::Plan, PermissionMode::Plan);
    }
}
"#
    );
}

/// Prose naming a crate the file no longer uses is the debt this operation is paying off.
///
/// A moved suite's `//!` header describes what it exercises, and after the move the daemon is not
/// it. The path resolves through the same walk as a declaration, so the comment and the code cannot
/// drift apart.
#[test]
fn re_points_a_crate_path_named_in_a_doc_comment() {
    // Given a module comment naming the facade the suite reached its subject through
    let directory = a_workspace_whose_moved_test_reads(
        r#"//! Exercising `daemon::claude_cli`, a module the daemon only passes on.

#[test]
fn reports_the_mode_it_was_launched_with() {}
"#,
    );

    // When the move is resolved
    let moved = the_moved_test_after_resolving(&directory);

    // Then the prose names the crate that defines it
    assert_eq!(
        moved,
        r#"//! Exercising `session_lifecycle::claude_cli`, a module the daemon only passes on.

#[test]
fn reports_the_mode_it_was_launched_with() {}
"#
    );
}

/// A group in prose is not a declaration, so it neither refuses the move nor is rewritten.
///
/// `use daemon::{a, b};` is refused because the two members need not be defined by the same crate
/// and splitting the declaration is the plan author's call. A sentence saying the suite once
/// consumed `daemon::{sandbox_session, tool_engine}` asks nothing of the operation: there is no
/// declaration to split, and one name cannot stand for both answers.
#[test]
fn leaves_a_group_named_in_a_comment_alone_without_refusing_the_move() {
    // Given a comment naming a group, above a declaration that does resolve
    let directory = a_workspace_whose_moved_test_reads(
        r#"//! It consumed `daemon::{claude_cli, project_storage}` before the carve.

use daemon::claude_cli::PermissionMode;
"#,
    );

    // When the move is resolved
    let moved = the_moved_test_after_resolving(&directory);

    // Then the declaration is re-pointed and the sentence is left as written
    assert_eq!(
        moved,
        r#"//! It consumed `daemon::{claude_cli, project_storage}` before the carve.

use session_lifecycle::claude_cli::PermissionMode;
"#
    );
}

/// Text inside a string literal is data the suite asserts on, not a path the compiler resolves.
///
/// A suite that checks an error message naming a module would start failing on its own fixture if
/// the crate name in it were rewritten, and the rewrite would be wrong: the message is produced by
/// whatever emits it, not by this file's imports.
#[test]
fn leaves_a_crate_path_inside_a_string_literal_alone() {
    // Given a suite asserting on a message that names the daemon
    let directory = a_workspace_whose_moved_test_reads(
        r#"use daemon::claude_cli::PermissionMode;

#[test]
fn names_the_crate_the_mode_was_read_from() {
    assert_eq!(PermissionMode::Plan.source(), "daemon::claude_cli");
}
"#,
    );

    // When the move is resolved
    let moved = the_moved_test_after_resolving(&directory);

    // Then only the declaration moves
    assert_eq!(
        moved,
        r#"use session_lifecycle::claude_cli::PermissionMode;

#[test]
fn names_the_crate_the_mode_was_read_from() {
    assert_eq!(PermissionMode::Plan.source(), "daemon::claude_cli");
}
"#
    );
}

/// A crate only a body path names still has to be declared, or the moved suite does not compile.
///
/// The dependency question and the rewrite question have the same answer for every occurrence, so
/// they are answered in the same pass: a path re-pointed at `host-service` is a path that needs
/// `host-service` in the destination's `[dev-dependencies]`.
#[test]
fn gives_the_destination_a_dependency_for_a_crate_only_a_body_path_names() {
    // Given a suite whose only reference to the host registry is inside a body
    let directory = a_workspace_whose_moved_test_reads(
        r#"#[test]
fn registers_the_host_it_was_given() {
    daemon::host_registry::register("localhost");
}
"#,
    );

    // When the move is resolved
    let edit = the_resolved_move_in(&directory);

    // Then the destination declares the crate that body path now names
    assert_eq!(
        added_to(&edit, &format!("{DESTINATION}/Cargo.toml")),
        "\n[dev-dependencies]\nhost-service = { path = \"../host-service\" }\n"
    );
}

/// The grouped-declaration refusal is deliberate, and reaching further into the file does not end
/// it: a group is still a group wherever it is declared.
#[test]
fn refuses_a_grouped_declaration_reaching_several_modules_of_the_origin() {
    // Given a suite that imports two of the daemon's modules at once
    let directory =
        a_workspace_whose_moved_test_reads("use daemon::{claude_cli, project_storage};\n");
    let overlay = Overlay::new();
    let workspace = Workspace {
        root: directory.path(),
        overlay: &overlay,
    };
    let moving = read_test_binary_move(&workspace, &a_move_of(MOVED_TEST, DESTINATION))
        .expect("the anchor is a test binary");

    // When the move is resolved
    let refusal = resolve_test_binary_move(&workspace, &moving)
        .expect_err("a group reaching several modules at once is refused");

    // Then the refusal asks for the declaration to be split
    assert!(
        refusal.to_string().contains("write one `use` per path"),
        "the refusal does not say what to do about the group: {refusal}"
    );
}

/// A crate whose name merely opens with the origin's is another crate, and not this pass's to touch.
///
/// `tddy-daemon-kernel` is a real crate in this workspace and 8 of the 95 suites in the first plan
/// name it. Read as a prefix of `tddy_daemon`, a grouped `tddy_daemon_kernel::{…}` refuses the move
/// on a rule about a declaration it is not — and the quieter half is worse: `daemon_kernel::x` is
/// re-pointed at whatever defines the *daemon's* module `x`, which is a crate it never named.
#[test]
fn leaves_a_crate_whose_name_merely_opens_with_the_origins_alone() {
    // Given a suite naming the kernel, whose extern name has the daemon's as a prefix
    let directory = a_workspace_whose_moved_test_reads(
        r#"use daemon_kernel::{config, relay};

#[test]
fn reads_the_relay_it_was_configured_with() {
    daemon_kernel::claude_cli::PermissionMode::Plan;
}
"#,
    );

    // When the move is resolved
    let moved = the_moved_test_after_resolving(&directory);

    // Then every path it writes is left exactly as written
    assert_eq!(
        moved,
        r#"use daemon_kernel::{config, relay};

#[test]
fn reads_the_relay_it_was_configured_with() {
    daemon_kernel::claude_cli::PermissionMode::Plan;
}
"#
    );
}

/// Three crates and the suite moving between them: the daemon, which defines none of what the test
/// reaches; the crate it passes `claude_cli` and `project_storage` on from, which is where the test
/// is going; and a third that defines `host_registry`, which the destination has to gain a
/// dependency on.
///
/// The moved test's own text is the scenario, so each test writes the file it is about.
fn a_workspace_whose_moved_test_reads(text: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");

    for (relative, contents) in [
        (
            "packages/daemon/Cargo.toml",
            "[package]\nname = \"daemon\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nsession-lifecycle = { path = \"../session-lifecycle\" }\n\
             host-service = { path = \"../host-service\" }\n\
             daemon-kernel = { path = \"../daemon-kernel\" }\n",
        ),
        (
            "packages/daemon/src/lib.rs",
            "//! The crate the suite is leaving, which defines none of what it reaches.\n\n\
             pub use host_service::host_registry;\n\
             pub use session_lifecycle::{claude_cli, project_storage};\n",
        ),
        (MOVED_TEST, text),
        (
            "packages/session-lifecycle/Cargo.toml",
            "[package]\nname = \"session-lifecycle\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "packages/session-lifecycle/src/lib.rs",
            "//! The crate that defines what the suite exercises.\n\n\
             pub mod claude_cli;\npub mod project_storage;\n",
        ),
        (
            "packages/host-service/Cargo.toml",
            "[package]\nname = \"host-service\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "packages/host-service/src/lib.rs",
            "//! The crate that defines the host registry.\n\npub mod host_registry;\n",
        ),
        (
            "packages/daemon-kernel/Cargo.toml",
            "[package]\nname = \"daemon-kernel\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "packages/daemon-kernel/src/lib.rs",
            "//! A crate whose extern name opens with the daemon's, and is not the daemon.\n\n\
             pub mod claude_cli;\npub mod config;\npub mod relay;\n",
        ),
    ] {
        let absolute = directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent directory"))
            .expect("the directory is created");
        std::fs::write(absolute, contents).expect("the file is written");
    }
    directory
}

/// The resolved move of the workspace's test binary to the crate that defines what it exercises.
fn the_resolved_move_in(directory: &tempfile::TempDir) -> WorkspaceEdit {
    let overlay = Overlay::new();
    let workspace = Workspace {
        root: directory.path(),
        overlay: &overlay,
    };
    let moving = read_test_binary_move(&workspace, &a_move_of(MOVED_TEST, DESTINATION))
        .expect("the anchor is a test binary");

    resolve_test_binary_move(&workspace, &moving).expect("the move resolves")
}

/// The moved test's own text with the resolved edits folded into it — what lands in the
/// destination's `tests/`.
fn the_moved_test_after_resolving(directory: &tempfile::TempDir) -> String {
    let overlay = Overlay::new();
    let workspace = Workspace {
        root: directory.path(),
        overlay: &overlay,
    };
    let edit = the_resolved_move_in(directory);
    let before = workspace.read(MOVED_TEST).expect("the moved test reads");

    tddy_code_restructuring::apply::edited(before, &edits_to(&edit, MOVED_TEST))
        .expect("the edits fold into the text they were measured against")
}

/// Every text edit a resolved move addresses to one file.
fn edits_to(edit: &WorkspaceEdit, path: &str) -> Vec<tddy_code_restructuring::TextEdit> {
    edit.changes
        .iter()
        .filter_map(|change| match change {
            FileEdit::Change {
                path: changed,
                edits,
            } if changed == path => Some(edits.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}
