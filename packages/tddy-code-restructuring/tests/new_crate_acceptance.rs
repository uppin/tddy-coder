//! A crate move whose line carries `name` creates its destination: what the plan may say, and what
//! a plain `check` reports about it before any server starts.
//!
//! `#carve` built every new crate's skeleton by hand, because a `to` without a `Cargo.toml` was
//! refused. These tests pin the plan surface of the creation and its static refusals, through the
//! public API a `check` uses: `Plan::parse` and `unrunnable_moves`. None of them needs a server.

use std::path::Path;

use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{unrunnable_moves, Overlay, Plan, RestructureError};

const HEADER: &str = r#"{"v":1,"snapshot":{}}"#;

const ROOT_MANIFEST: &str =
    "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/origin\",\n    \"crates/other\",\n]\n";

/// `move_module_to_crate` of `crates/origin`'s `host_registry` into `to`, with `extra` fields
/// spliced into the line (`"name":"fresh"`).
fn a_move_of_the_host_registry(to: &str, extra: &str) -> String {
    format!(
        r#"{{"op":"move_module_to_crate","anchor":{{"kind":"symbol","file":"crates/origin/src/host_registry.rs","path":"host_registry"}},"to":"{to}","reexport":"glob"{extra}}}"#
    )
}

/// `move_module_to_crate` of `crates/origin`'s `clock` into `to`.
fn a_move_of_the_clock(to: &str, extra: &str) -> String {
    format!(
        r#"{{"op":"move_module_to_crate","anchor":{{"kind":"symbol","file":"crates/origin/src/clock.rs","path":"clock"}},"to":"{to}","reexport":"glob"{extra}}}"#
    )
}

/// `move_test_binary_to_crate` of `crates/origin/tests/boot.rs` into `to`.
fn a_move_of_the_boot_test(to: &str, extra: &str) -> String {
    format!(
        r#"{{"op":"move_test_binary_to_crate","anchor":{{"kind":"symbol","file":"crates/origin/tests/boot.rs","path":"boot"}},"to":"{to}"{extra}}}"#
    )
}

fn a_plan_of(lines: &[String]) -> String {
    format!("{HEADER}\n{}\n", lines.join("\n"))
}

fn the_refusal_of(plan: &str) -> String {
    match Plan::parse(plan) {
        Err(RestructureError::MalformedPlan(reason)) => reason,
        other => panic!("expected the plan to be refused as malformed, got {other:?}"),
    }
}

/// A workspace listing `crates/origin` (modules `host_registry` and `clock`) and `crates/other`
/// (package `other`).
struct AWorkspace {
    directory: tempfile::TempDir,
}

fn a_workspace_with_an_origin_and_another_crate() -> AWorkspace {
    AWorkspace {
        directory: tempfile::tempdir().expect("a temporary directory"),
    }
    .with("Cargo.toml", ROOT_MANIFEST)
    .with(
        "crates/origin/Cargo.toml",
        "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with(
        "crates/origin/src/lib.rs",
        "pub mod clock;\npub mod host_registry;\n",
    )
    .with(
        "crates/origin/src/host_registry.rs",
        "pub struct HostRegistry;\n",
    )
    .with("crates/origin/src/clock.rs", "pub struct Clock;\n")
    .with(
        "crates/other/Cargo.toml",
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .with("crates/other/src/lib.rs", "\n")
}

impl AWorkspace {
    fn with(self, relative: &str, text: &str) -> Self {
        let absolute = self.directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
        self
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    /// What a plain `check` reports for `plan`, one finding per unrunnable operation.
    fn findings_for(&self, plan: &str) -> Vec<String> {
        let plan = Plan::parse(plan).expect("the plan parses");
        let overlay = Overlay::new();
        let workspace = Workspace {
            root: self.root(),
            overlay: &overlay,
        };
        unrunnable_moves(&workspace, &plan.ops).expect("the preconditions read the workspace")
    }
}

fn assert_one_finding_naming(findings: &[String], expected: &[&str]) {
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    for fragment in expected {
        assert!(
            findings[0].contains(fragment),
            "the finding must name {fragment:?}: {findings:?}"
        );
    }
}

// --- the plan codec ------------------------------------------------------------------------------

/// Test 1 — a test binary moves into a crate that exists; `name` creates one only for a module move.
#[test]
fn name_on_a_test_binary_move_is_refused_naming_the_field() {
    // Given a test-binary move whose line carries `name`
    let plan = a_plan_of(&[a_move_of_the_boot_test(
        "crates/fresh",
        r#","name":"fresh""#,
    )]);

    // When it is parsed
    let refusal = the_refusal_of(&plan);

    // Then the refusal names the field and the operation
    assert!(refusal.contains("`name`"), "{refusal}");
    assert!(refusal.contains("move_test_binary_to_crate"), "{refusal}");
}

/// Test 2 — `name` becomes a `[package] name`, so it must be one cargo accepts.
#[test]
fn a_name_that_is_not_a_cargo_package_name_is_refused_naming_it() {
    for name in ["tddy x", "1x", "test", ""] {
        // Given a module move creating a crate under that name
        let plan = a_plan_of(&[a_move_of_the_host_registry(
            "crates/fresh",
            &format!(r#","name":"{name}""#),
        )]);

        // When it is parsed
        let refusal = the_refusal_of(&plan);

        // Then the refusal names it
        assert!(
            refusal.contains(&format!("`{name}`")),
            "the refusal of {name:?} must name it: {refusal}"
        );
    }
}

/// Test 3 — one crate, created once: the second creation of the same `to` is refused, naming the
/// operation that creates it first.
#[test]
fn a_plan_that_creates_the_same_crate_twice_is_refused_naming_the_earlier_operation() {
    // Given two operations creating `crates/fresh`
    let plan = a_plan_of(&[
        a_move_of_the_host_registry("crates/fresh", r#","name":"fresh""#),
        a_move_of_the_clock("crates/fresh", r#","name":"fresh""#),
    ]);

    // When it is parsed
    let refusal = the_refusal_of(&plan);

    // Then the refusal names the directory and the operation creating it already
    assert!(refusal.contains("crates/fresh"), "{refusal}");
    assert!(refusal.contains("operation 0"), "{refusal}");
}

// --- the static check ----------------------------------------------------------------------------

/// Test 4 — a crate created by an earlier operation is there for a later one, as `apply` finds it.
#[test]
fn a_static_check_of_a_plan_that_creates_a_crate_and_then_moves_into_it_reports_nothing() {
    // Given a plan creating `crates/fresh`, then moving a second module into it
    let workspace = a_workspace_with_an_origin_and_another_crate();
    let plan = a_plan_of(&[
        a_move_of_the_host_registry("crates/fresh", r#","name":"fresh""#),
        a_move_of_the_clock("crates/fresh", ""),
    ]);

    // When it is checked
    let findings = workspace.findings_for(&plan);

    // Then nothing is unrunnable
    assert_eq!(findings, Vec::<String>::new());
}

/// Test 5 — `name` creates; a crate that exists is moved into without it.
#[test]
fn a_static_check_reports_name_on_a_destination_that_is_already_a_crate() {
    // Given a plan creating a crate where `crates/other` already is one
    let workspace = a_workspace_with_an_origin_and_another_crate();
    let plan = a_plan_of(&[a_move_of_the_host_registry(
        "crates/other",
        r#","name":"fresh""#,
    )]);

    // When it is checked
    let findings = workspace.findings_for(&plan);

    // Then the finding says it is a crate already and how to move into it
    assert_one_finding_naming(
        &findings,
        &["crates/other", "already a crate", "drop `name`"],
    );
}

/// Test 6 — a creation never writes over files that are already there.
#[test]
fn a_static_check_reports_name_on_a_directory_that_is_not_empty() {
    // Given a plan creating a crate in a directory that holds a stray file
    let workspace =
        a_workspace_with_an_origin_and_another_crate().with("crates/fresh/notes.md", "notes\n");
    let plan = a_plan_of(&[a_move_of_the_host_registry(
        "crates/fresh",
        r#","name":"fresh""#,
    )]);

    // When it is checked
    let findings = workspace.findings_for(&plan);

    // Then the finding names the directory as not empty
    assert_one_finding_naming(&findings, &["crates/fresh", "not empty"]);
}

/// Test 7 — two crates under one extern name cannot be told apart by a `use` path.
#[test]
fn a_static_check_reports_a_name_whose_extern_name_the_origin_or_a_listed_member_already_has() {
    for (name, holder) in [("origin", "crates/origin"), ("other", "crates/other")] {
        // Given a plan creating a crate whose extern name a listed crate already has
        let workspace = a_workspace_with_an_origin_and_another_crate();
        let plan = a_plan_of(&[a_move_of_the_host_registry(
            "crates/fresh",
            &format!(r#","name":"{name}""#),
        )]);

        // When it is checked
        let findings = workspace.findings_for(&plan);

        // Then the finding names the extern name and the crate holding it
        assert_one_finding_naming(
            &findings,
            &[&format!("`{name}` is already the extern name of"), holder],
        );
    }
}

/// Test 8 — cargo builds only the crates the workspace lists; creating one it would not build is
/// refused rather than written.
#[test]
fn a_static_check_reports_a_crate_creation_in_a_workspace_with_no_members_list() {
    // Given a root manifest with no `members` array
    let workspace = a_workspace_with_an_origin_and_another_crate()
        .with("Cargo.toml", "[workspace]\nresolver = \"2\"\n");
    let plan = a_plan_of(&[a_move_of_the_host_registry(
        "crates/fresh",
        r#","name":"fresh""#,
    )]);

    // When it is checked
    let findings = workspace.findings_for(&plan);

    // Then the finding says cargo would not build the new crate
    assert_one_finding_naming(&findings, &["members", "fresh"]);
}

/// Test 9 — a missing crate without `name` is still a plan defect, and the refusal says what
/// creates it.
#[test]
fn a_move_into_a_missing_crate_without_name_is_refused_and_says_name_creates_it() {
    // Given a move into a directory that holds no crate, with no `name`
    let workspace = a_workspace_with_an_origin_and_another_crate();
    let plan = a_plan_of(&[a_move_of_the_host_registry("crates/fresh", "")]);

    // When it is checked
    let findings = workspace.findings_for(&plan);

    // Then it is refused as not a crate, naming the remedy
    assert_one_finding_naming(
        &findings,
        &["crates/fresh", "is not a crate", "add `name` to create it"],
    );
}
