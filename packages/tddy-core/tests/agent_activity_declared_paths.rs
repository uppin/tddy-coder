//! What a tool call declares it will touch — AC2 of `docs/ft/daemon/session-worktree-sync.md`.
//!
//! These paths are what scope a call's delta to its own files, so the cost of getting them wrong is
//! not a missing notification but a patch attributed to the wrong call. Every case here is either
//! "the call said which file" or "the call said nothing, and saying nothing must not become a
//! guess".

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::agent_activity::declared_paths;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The worktree every declared path is resolved against. A real path on disk is unnecessary —
/// nothing here touches the filesystem — but it must look like the absolute root a tool reports.
fn a_worktree_root() -> PathBuf {
    PathBuf::from("/home/agent/.tddy/sessions/1780828020298-abc/worktree")
}

/// A tool input naming one file, the shape claude-cli reports for an edit.
fn an_input_naming(field: &str, path: &str) -> serde_json::Value {
    serde_json::json!({ field: path })
}

/// An absolute path inside the worktree.
fn inside_the_worktree(rel: &str) -> String {
    a_worktree_root().join(rel).to_string_lossy().into_owned()
}

fn declared(tool_name: &str, input: &serde_json::Value) -> Vec<String> {
    declared_paths(tool_name, input, &a_worktree_root())
}

// ---------------------------------------------------------------------------
// Tools that declare a file
// ---------------------------------------------------------------------------

#[rstest]
#[case::edit("Edit", "file_path")]
#[case::write("Write", "file_path")]
#[case::multi_edit("MultiEdit", "file_path")]
#[case::notebook_edit("NotebookEdit", "notebook_path")]
fn credits_a_writing_tool_with_the_file_it_named(#[case] tool: &str, #[case] field: &str) {
    // Given a call that named the file it would write
    let input = an_input_naming(field, &inside_the_worktree("src/lib.rs"));

    // When
    let paths = declared(tool, &input);

    // Then
    assert_eq!(paths, vec!["src/lib.rs".to_string()]);
}

#[test]
fn returns_a_path_relative_to_the_worktree_because_that_is_what_a_pathspec_speaks() {
    // Given a deeply nested absolute path, as a tool reports it
    let input = an_input_naming(
        "file_path",
        &inside_the_worktree("packages/core/src/main.rs"),
    );

    // When
    let paths = declared("Write", &input);

    // Then the worktree prefix is gone — a diff header and a git pathspec both name files
    // relative to the repository, and an absolute path matches neither.
    assert_eq!(paths, vec!["packages/core/src/main.rs".to_string()]);
}

#[test]
fn accepts_a_path_a_tool_already_reported_relative_to_the_worktree() {
    // Given a relative path, which some agents report instead of an absolute one
    let input = an_input_naming("file_path", "src/lib.rs");

    // When
    let paths = declared("Edit", &input);

    // Then
    assert_eq!(paths, vec!["src/lib.rs".to_string()]);
}

// ---------------------------------------------------------------------------
// Tools that declare nothing
// ---------------------------------------------------------------------------

#[rstest]
#[case::bash("Bash")]
#[case::read("Read")]
#[case::grep("Grep")]
#[case::glob("Glob")]
#[case::web_fetch("WebFetch")]
fn credits_a_tool_that_declared_no_write_with_nothing(#[case] tool: &str) {
    // Given a call carrying a path it did not say it would write to
    let input = serde_json::json!({
        "command": "sed -i s/a/b/ src/lib.rs",
        "file_path": inside_the_worktree("src/lib.rs"),
    });

    // When
    let paths = declared(tool, &input);

    // Then nothing is credited. A `Bash` that reformats the tree changes files it never named, and
    // guessing from a stray field would hand this call a patch belonging to whichever call
    // actually wrote it — what such a tool changes travels in the tick's residual instead.
    assert_eq!(paths, Vec::<String>::new());
}

#[test]
fn credits_a_tool_this_build_has_never_heard_of_with_nothing() {
    // Given a tool a newer agent introduced
    let input = an_input_naming("file_path", &inside_the_worktree("src/lib.rs"));

    // When
    let paths = declared("SomeFutureTool", &input);

    // Then it declares nothing rather than being assumed to write what it names — an unknown
    // tool's `file_path` may not be a file it writes at all.
    assert_eq!(paths, Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// Inputs that name nothing usable
// ---------------------------------------------------------------------------

#[test]
fn drops_a_declared_path_that_falls_outside_the_worktree() {
    // Given an edit to a file elsewhere on the host
    let input = an_input_naming("file_path", "/etc/hosts");

    // When
    let paths = declared("Edit", &input);

    // Then it is dropped. It cannot appear in this worktree's diff, so keeping it would be a
    // scope that matches nothing while looking like it matches something.
    assert_eq!(paths, Vec::<String>::new());
}

#[test]
fn drops_a_declared_path_that_climbs_out_of_the_worktree() {
    // Given a path that resolves outside without ever looking absolute
    let input = an_input_naming("file_path", &inside_the_worktree("../../../etc/hosts"));

    // When
    let paths = declared("Write", &input);

    // Then
    assert_eq!(paths, Vec::<String>::new());
}

#[rstest]
#[case::missing_field(serde_json::json!({ "old_string": "a", "new_string": "b" }))]
#[case::null_path(serde_json::json!({ "file_path": null }))]
#[case::non_string_path(serde_json::json!({ "file_path": 42 }))]
#[case::empty_path(serde_json::json!({ "file_path": "" }))]
#[case::input_is_not_an_object(serde_json::json!("just a string"))]
#[case::input_is_null(serde_json::Value::Null)]
fn credits_nothing_when_the_input_names_no_usable_file(#[case] input: serde_json::Value) {
    // Given / When
    let paths = declared("Edit", &input);

    // Then a malformed input is not a reason to guess, and not a reason to panic — a `running`
    // row is written before the tool has produced anything, so an incomplete input is ordinary.
    assert_eq!(paths, Vec::<String>::new());
}

#[test]
fn names_the_worktree_root_itself_as_no_path_at_all() {
    // Given a declared path that IS the worktree root
    let root = a_worktree_root();
    let input = an_input_naming("file_path", &root.to_string_lossy());

    // When
    let paths = declared_paths("Write", &input, Path::new(&root));

    // Then it yields nothing rather than an empty relative path, which as a pathspec would match
    // the entire tree and scope a call to every file in it.
    assert_eq!(paths, Vec::<String>::new());
}
