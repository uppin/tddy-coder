//! `restructure snapshot` writes the header a plan of bare operations lacks.
//!
//! `restructure anchors` emits an anchor, and wrapping it in a one-line plan is the obvious next
//! step. Until `snapshot` wrote line 1 for such a plan, the author computed a `sha256` by hand to
//! get it past `check`. What these suites pin: the header names exactly the files the operations
//! anchor, the operations come through as the bytes they arrived as, a refusal leaves the plan
//! untouched, and every *other* reader is told which command writes the header.

use std::path::{Path, PathBuf};

use tddy_code_restructuring::apply::hash_file;
use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_code_restructuring::Plan;
use tokio_util::sync::CancellationToken;

/// A git worktree holding three source files, at a path that is not the process directory.
fn a_workspace_of_three_files() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    for file in ["lib.rs", "other.rs", "third.rs"] {
        std::fs::write(
            workspace.path().join("src").join(file),
            "pub fn foo() -> u32 {\n    1\n}\n",
        )
        .expect("a source file");
    }
    workspace
}

/// A plan of bare operations: no header, `text` verbatim.
fn a_headerless_plan(root: &Path, text: &str) -> PathBuf {
    let plan = root.join("plan.jsonl");
    std::fs::write(&plan, text).expect("write the plan");
    plan
}

fn a_snapshot_of(plan: &Path) -> Options {
    Options {
        command: Command::Snapshot,
        target: Some(plan.to_path_buf()),
        ..Options::default()
    }
}

fn a_check_of(plan: &Path) -> Options {
    Options {
        command: Command::Check,
        target: Some(plan.to_path_buf()),
        ..Options::default()
    }
}

fn the_header_line_of(plan: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(plan).expect("read the plan");
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("a first line");
    serde_json::from_str(first).expect("line 1 is JSON")
}

fn the_digest_of(root: &Path, file: &str) -> String {
    hash_file(&root.join(file)).expect("hash the source")
}

fn an_extraction_of_items(file: &str, item: &str) -> String {
    format!(
        r#"{{"op":"extract_module","anchor":{{"kind":"items","file":"{file}","items":["{item}"],"fingerprints":["sha256:aa"]}},"name":"grouped"}}"#
    )
}

fn an_extraction_of_items_with_also(file: &str, also_file: &str) -> String {
    format!(
        r#"{{"op":"move_cluster_to_crate","anchor":{{"kind":"items","file":"{file}","items":["demo::foo"],"fingerprints":["sha256:aa"]}},"also":[{{"kind":"items","file":"{also_file}","items":["demo::foo"],"fingerprints":["sha256:bb"]}}],"to":"packages/destination"}}"#
    )
}

fn an_extraction_of_a_range(file: &str) -> String {
    format!(
        r#"{{"op":"extract_module","anchor":{{"kind":"range","file":"{file}","start":{{"line":1,"col":1}},"end":{{"line":3,"col":2}}}},"name":"grouped","reexport":"glob"}}"#
    )
}

/// An extraction whose anchor names `file` — which may be absolute, escaping, or empty.
fn an_extraction_naming(file: &str) -> String {
    an_extraction_of_items(file, "demo::foo")
}

#[test]
fn a_written_header_names_each_file_by_its_hash_alone() {
    // Given a plan of one operation over `src/lib.rs`, with no header
    let workspace = a_workspace_of_three_files();
    let plan = a_headerless_plan(
        workspace.path(),
        &format!("{}\n", an_extraction_of_items("src/lib.rs", "demo::foo")),
    );

    // When the plan is snapshotted
    runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the file's hint is its hash and nothing else
    assert_eq!(
        the_header_line_of(&plan)["files"]["src/lib.rs"],
        serde_json::json!({ "sha256": the_digest_of(workspace.path(), "src/lib.rs") })
    );
}

#[test]
fn a_plan_whose_header_carries_modified_is_checked_and_its_rewrite_drops_it() {
    // Given a v2 plan whose header still carries a `modified` time, and a stale hash
    let workspace = a_workspace_of_three_files();
    let plan = a_headerless_plan(
        workspace.path(),
        &format!(
            "{}\n{}\n",
            r#"{"v":2,"files":{"src/lib.rs":{"sha256":"sha256:old","modified":"2026-09-26T00:00:00Z"}}}"#,
            an_extraction_of_a_range("src/lib.rs")
        ),
    );

    // When it is checked, and then snapshotted
    let checked = runner::check(
        workspace.path(),
        a_check_of(&plan),
        None,
        CancellationToken::new(),
    );
    runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the header did not stop the check, and the rewritten hint is the hash alone
    assert!(checked.is_ok(), "{checked:?}");
    assert_eq!(
        the_header_line_of(&plan)["files"]["src/lib.rs"],
        serde_json::json!({ "sha256": the_digest_of(workspace.path(), "src/lib.rs") })
    );
}

#[test]
fn writes_a_header_naming_every_file_the_operations_anchor() {
    // Given a plan of two item-anchored operations over two files, with no header
    let workspace = a_workspace_of_three_files();
    let lib = an_extraction_of_items("src/lib.rs", "demo::foo");
    let other = an_extraction_of_items("src/other.rs", "demo::foo");
    let plan = a_headerless_plan(workspace.path(), &format!("{lib}\n{other}\n"));

    // When the plan is snapshotted
    let rewrite = runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then line 1 is a v2 header naming exactly those two files, hashed as the tree has them
    let header = the_header_line_of(&plan);
    let files = header["files"].as_object().expect("a files map");
    assert_eq!(header["v"], 2);
    assert_eq!(
        files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["src/lib.rs", "src/other.rs"]
    );
    assert_eq!(
        files["src/lib.rs"]["sha256"],
        the_digest_of(workspace.path(), "src/lib.rs")
    );
    assert_eq!(
        files["src/other.rs"]["sha256"],
        the_digest_of(workspace.path(), "src/other.rs")
    );
    assert_eq!(rewrite.paths, 2);
    assert!(rewrite.rewritten);
}

#[test]
fn names_the_files_of_an_also_anchor_too() {
    // Given one operation whose `also` anchor is in a third file
    let workspace = a_workspace_of_three_files();
    let operation = an_extraction_of_items_with_also("src/lib.rs", "src/third.rs");
    let plan = a_headerless_plan(workspace.path(), &format!("{operation}\n"));

    // When the plan is snapshotted
    runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the header names both the primary file and the `also` file
    let header = the_header_line_of(&plan);
    let files = header["files"].as_object().expect("a files map");
    assert_eq!(
        files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["src/lib.rs", "src/third.rs"]
    );
}

#[test]
fn names_a_file_once_when_two_operations_anchor_it() {
    // Given two operations over the same file
    let workspace = a_workspace_of_three_files();
    let first = an_extraction_of_items("src/lib.rs", "demo::foo");
    let second = an_extraction_of_items("src/lib.rs", "demo::bar");
    let plan = a_headerless_plan(workspace.path(), &format!("{first}\n{second}\n"));

    // When the plan is snapshotted
    let rewrite = runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the file is one key of the header, and one path of the report
    let header = the_header_line_of(&plan);
    assert_eq!(header["files"].as_object().expect("files").len(), 1);
    assert_eq!(rewrite.paths, 1);
}

#[test]
fn leaves_every_operation_line_byte_identical_and_in_order_and_keeps_leading_blank_lines() {
    // Given two operations behind two blank lines, with no trailing newline
    let workspace = a_workspace_of_three_files();
    let lib = an_extraction_of_items("src/lib.rs", "demo::foo");
    let other = an_extraction_of_items("src/other.rs", "demo::foo");
    let plan = a_headerless_plan(workspace.path(), &format!("\n\n{lib}\n{other}"));

    // When the plan is snapshotted
    runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the blank lines stay first, the header sits above the first operation, and the
    // operations follow exactly as written
    let produced = std::fs::read_to_string(&plan).expect("read the plan");
    let lines: Vec<&str> = produced.split('\n').collect();
    assert_eq!((lines[0], lines[1]), ("", ""));
    assert!(lines[2].starts_with(r#"{"v":2"#), "{}", lines[2]);
    assert_eq!(&lines[3..], [lib.as_str(), other.as_str()]);
}

#[test]
fn a_second_snapshot_of_the_plan_it_wrote_changes_nothing() {
    // Given a plan that was snapshotted once, over an unchanged tree
    let workspace = a_workspace_of_three_files();
    let operation = an_extraction_of_items("src/lib.rs", "demo::foo");
    let plan = a_headerless_plan(workspace.path(), &format!("{operation}\n"));
    runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("the first snapshot");
    let after_the_first = std::fs::read(&plan).expect("read the plan");

    // When it is snapshotted again
    let second = runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the file is byte-identical and the run says it rewrote nothing
    assert_eq!(
        std::fs::read(&plan).expect("read the plan"),
        after_the_first
    );
    assert!(!second.rewritten);
}

#[test]
fn refuses_an_operation_that_anchors_a_file_that_is_not_there_and_writes_nothing() {
    // Given an operation anchored in a file the workspace does not hold
    let workspace = a_workspace_of_three_files();
    let text = format!(
        "{}\n",
        an_extraction_of_items("src/missing.rs", "demo::foo")
    );
    let plan = a_headerless_plan(workspace.path(), &text);

    // When the plan is snapshotted
    let refusal = runner::snapshot(workspace.path(), a_snapshot_of(&plan))
        .expect_err("a file that is not there is refused")
        .to_string();

    // Then the refusal names the file, and the plan is as it was
    assert!(refusal.contains("src/missing.rs"), "{refusal}");
    assert_eq!(std::fs::read_to_string(&plan).expect("read the plan"), text);
}

/// The plan under `file` snapshotted: the refusal it earned, and that the plan was left alone.
fn the_refusal_of_snapshotting_a_plan_naming(file: &str) -> String {
    let workspace = a_workspace_of_three_files();
    let text = format!("{}\n", an_extraction_naming(file));
    let plan = a_headerless_plan(workspace.path(), &text);

    let refusal = runner::snapshot(workspace.path(), a_snapshot_of(&plan))
        .expect_err("an anchor that names no workspace file is refused")
        .to_string();

    assert_eq!(std::fs::read_to_string(&plan).expect("read the plan"), text);
    refusal
}

#[test]
fn refuses_an_anchor_outside_the_workspace_and_a_plan_naming_no_file() {
    // Given plans anchored at an absolute path, at a path that climbs out, and at no file
    // When each is snapshotted
    let absolute = the_refusal_of_snapshotting_a_plan_naming("/etc/hosts");
    let climbing = the_refusal_of_snapshotting_a_plan_naming("../x.rs");
    let nameless = the_refusal_of_snapshotting_a_plan_naming("");

    // Then each is refused for its own reason, and none wrote anything
    assert!(absolute.contains("outside the workspace"), "{absolute}");
    assert!(climbing.contains("outside the workspace"), "{climbing}");
    assert!(nameless.contains("names no file"), "{nameless}");
}

#[test]
fn snapshot_resolving_writes_the_header_with_no_language_server() {
    // Given an item-anchored plan with no header, and no language server to ask
    let workspace = a_workspace_of_three_files();
    let operation = an_extraction_of_items("src/lib.rs", "demo::foo");
    let plan = a_headerless_plan(workspace.path(), &format!("{operation}\n"));

    // When it is snapshotted through the router
    let rewrite = runner::snapshot_resolving(
        workspace.path(),
        a_snapshot_of(&plan),
        None,
        CancellationToken::new(),
    )
    .expect("a headerless plan is snapshotted without a server");

    // Then the header was written
    assert_eq!(the_header_line_of(&plan)["v"], 2);
    assert!(rewrite.rewritten);
}

#[test]
fn every_other_reader_of_a_headerless_plan_is_told_to_run_snapshot() {
    // Given a plan of bare operations, and a plan whose first line is neither header nor operation
    let workspace = a_workspace_of_three_files();
    let operation = an_extraction_of_items("src/lib.rs", "demo::foo");
    let headerless = a_headerless_plan(workspace.path(), &format!("{operation}\n"));
    let garbage = workspace.path().join("garbage.jsonl");
    std::fs::write(&garbage, format!("not json\n{operation}\n")).expect("write the plan");

    // When each is read by `check` and by `Plan::parse`
    let checked = runner::check(
        workspace.path(),
        a_check_of(&headerless),
        None,
        CancellationToken::new(),
    )
    .expect_err("a headerless plan is refused by check")
    .to_string();
    let parsed = Plan::parse(&std::fs::read_to_string(&headerless).expect("read"))
        .expect_err("a headerless plan is refused by parse")
        .to_string();
    let garbled = Plan::parse(&std::fs::read_to_string(&garbage).expect("read"))
        .expect_err("a garbage first line is refused by parse")
        .to_string();

    // Then the headerless refusals name the remedy, and the garbage one does not
    assert!(checked.contains("restructure snapshot"), "{checked}");
    assert!(parsed.contains("restructure snapshot"), "{parsed}");
    assert!(!garbled.contains("restructure snapshot"), "{garbled}");
}

#[test]
fn writes_a_snapshot_header_for_a_plan_that_anchors_by_range() {
    // Given a headerless plan whose one operation anchors by range
    let workspace = a_workspace_of_three_files();
    let plan = a_headerless_plan(
        workspace.path(),
        &format!("{}\n", an_extraction_of_a_range("src/lib.rs")),
    );

    // When it is snapshotted
    runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then line 1 is the v1 header, which pins the file's digest
    let header = the_header_line_of(&plan);
    assert_eq!(header["v"], 1);
    assert_eq!(
        header["snapshot"]["src/lib.rs"],
        the_digest_of(workspace.path(), "src/lib.rs")
    );

    // And a check after the file is edited refuses, as a coordinate anchor must
    std::fs::write(workspace.path().join("src/lib.rs"), "pub fn foo() {}\n").expect("edit");
    let refusal = runner::check(
        workspace.path(),
        a_check_of(&plan),
        None,
        CancellationToken::new(),
    )
    .expect_err("a coordinate plan refuses on drift")
    .to_string();
    assert!(refusal.contains("snapshot mismatch"), "{refusal}");
}
