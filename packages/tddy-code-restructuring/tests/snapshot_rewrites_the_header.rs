//! `restructure snapshot` rewrites a plan's header from the working tree, and touches nothing else.
//!
//! A plan's first line pins the `sha256:` of every file its anchors name, and every edit to one of
//! those files invalidates it. Recomputing it was a shell pipeline each author had to invent, and
//! the cost of getting it wrong is a refusal at the end of a cold index rather than at the start.
//!
//! What these suites pin is the "and touches nothing else" half. The operations after line 1 are
//! the plan; a command that rewrote a header and reflowed a body would be a plan editor, and no
//! author would trust it with a file they had hand-written anchors into.

use std::path::Path;

use tddy_code_restructuring::apply::hash_file;
use tddy_code_restructuring::runner::{self, Command, Options};

/// A git worktree holding one source file, at a path that is not the process directory.
fn a_workspace_holding(source: &str) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(workspace.path().join("src/lib.rs"), source).expect("a source file");
    workspace
}

/// A plan under `root` whose header pins `src/lib.rs` at `digest`, carrying `ops` verbatim.
fn a_plan_pinning(root: &Path, digest: &str, ops: &[&str]) -> std::path::PathBuf {
    let plan = root.join("plan.jsonl");
    let mut lines = vec![format!(
        "{{\"v\":1,\"snapshot\":{{\"src/lib.rs\":\"{digest}\"}}}}"
    )];
    lines.extend(ops.iter().map(|op| (*op).to_string()));
    std::fs::write(&plan, format!("{}\n", lines.join("\n"))).expect("write the plan");
    plan
}

fn a_snapshot_of(plan: &Path) -> Options {
    Options {
        command: Command::Snapshot,
        target: Some(plan.to_path_buf()),
        ..Options::default()
    }
}

const A_SOURCE: &str = "pub fn foo() -> u32 {\n    1\n}\n";

/// Two operations, so "every other line survives" is a claim about more than one.
const AN_EXTRACTION: &str = r#"{"op":"extract_module","anchor":{"kind":"range","file":"src/lib.rs","start":{"line":1,"col":1},"end":{"line":3,"col":2}},"name":"grouped","reexport":"glob"}"#;
const A_RENAME: &str = r#"{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/lib.rs","path":"foo"},"name":"bar"}"#;

#[test]
fn rewrites_a_stale_header_to_match_the_working_tree() {
    // Given a plan whose header pins a digest the file on disk no longer has
    let workspace = a_workspace_holding(A_SOURCE);
    let plan = a_plan_pinning(workspace.path(), "sha256:stale", &[AN_EXTRACTION]);

    // When the plan is snapshotted
    let rewrite = runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the header carries the digest the tree actually has
    let on_disk = hash_file(&workspace.path().join("src/lib.rs")).expect("hash the source");
    let header = std::fs::read_to_string(&plan).expect("read the plan");
    let header = header.lines().next().expect("a header line").to_string();

    assert!(header.contains(&on_disk), "{header}");
    assert!(!header.contains("sha256:stale"), "{header}");
    assert!(rewrite.rewritten);
    assert_eq!(rewrite.paths, 1);
}

#[test]
fn leaves_every_operation_line_byte_identical() {
    // Given a plan carrying two operations under a stale header
    let workspace = a_workspace_holding(A_SOURCE);
    let plan = a_plan_pinning(workspace.path(), "sha256:stale", &[AN_EXTRACTION, A_RENAME]);

    // When the plan is snapshotted
    runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the operations come through exactly as they were written
    let produced = std::fs::read_to_string(&plan).expect("read the plan");
    let operations: Vec<&str> = produced.lines().skip(1).collect();

    assert_eq!(operations, vec![AN_EXTRACTION, A_RENAME]);
}

#[test]
fn is_a_no_op_on_a_plan_whose_header_already_matches() {
    // Given a plan whose header already pins the tree as it stands
    let workspace = a_workspace_holding(A_SOURCE);
    let digest = hash_file(&workspace.path().join("src/lib.rs")).expect("hash the source");
    let plan = a_plan_pinning(workspace.path(), &digest, &[AN_EXTRACTION]);
    let before = std::fs::read_to_string(&plan).expect("read the plan");

    // When it is snapshotted
    let rewrite = runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the file is untouched, and the run says so rather than claiming a rewrite
    assert_eq!(
        std::fs::read_to_string(&plan).expect("read the plan"),
        before
    );
    assert!(!rewrite.rewritten);
}

/// A digest is of the bytes, so a file that changed under a header pinning its *previous* contents
/// is the case this command exists for — and the new digest must differ from the old one.
#[test]
fn follows_the_file_when_it_changes_under_a_header_that_pinned_it() {
    // Given a plan snapshotted against the tree, and then an edit to the file it pins
    let workspace = a_workspace_holding(A_SOURCE);
    let digest = hash_file(&workspace.path().join("src/lib.rs")).expect("hash the source");
    let plan = a_plan_pinning(workspace.path(), &digest, &[AN_EXTRACTION]);
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn foo() -> u32 {\n    2\n}\n",
    )
    .expect("edit the source");

    // When the plan is snapshotted again
    let rewrite = runner::snapshot(workspace.path(), a_snapshot_of(&plan)).expect("a snapshot");

    // Then the header moved to the edited file's digest
    let edited = hash_file(&workspace.path().join("src/lib.rs")).expect("hash the source");
    let header = std::fs::read_to_string(&plan).expect("read the plan");
    let header = header.lines().next().expect("a header line").to_string();

    assert_ne!(edited, digest);
    assert!(header.contains(&edited), "{header}");
    assert!(rewrite.rewritten);
}
