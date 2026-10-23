//! `verify --against` hands a changed file of a language beyond Rust to that language's verify, and
//! keeps Rust files to the statement comparison (`#reshape` extension-code-actions, R5).
//!
//! Library level: a git worktree and a stand-in verify claiming `.toml`; no grammar, no server.

use std::path::Path;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_code_restructuring::verify::{SourceChange, SourceFinding, SourceVerify};
use tddy_code_restructuring::Position;

/// A verify claiming `.toml` files that records each change it is handed and finds one diagnostic
/// in each.
#[derive(Default)]
struct ATomlVerify {
    handed: Mutex<Vec<SourceChange>>,
}

impl SourceVerify for ATomlVerify {
    fn claims(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension == "toml")
    }

    fn verify(
        &self,
        _root: &Path,
        change: &SourceChange,
    ) -> tddy_code_restructuring::Result<Vec<SourceFinding>> {
        self.handed.lock().unwrap().push(change.clone());
        Ok(vec![a_finding_in(&change.path)])
    }
}

fn a_finding_in(path: &str) -> SourceFinding {
    SourceFinding::NewErrorDiagnostic {
        path: path.to_string(),
        at: Position { line: 1, col: 1 },
        message: "unexpected BROKEN".to_string(),
    }
}

fn git(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

/// A git worktree whose one commit holds `config/app.toml`, `config/untouched.toml` and
/// `src/lib.rs`, each of which the test then changes or not.
fn a_committed_worktree() -> tempfile::TempDir {
    let tree = tempfile::tempdir().expect("a temporary worktree");
    let root = tree.path();
    git(root, &["init", "--quiet"]);
    std::fs::create_dir_all(root.join("config")).expect("config dir");
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::write(root.join("config/app.toml"), "[server]\nport = 8080\n").expect("app config");
    std::fs::write(root.join("config/untouched.toml"), "[a]\nb = 1\n").expect("untouched");
    std::fs::write(root.join("src/lib.rs"), "pub fn one() -> u32 {\n    1\n}\n").expect("lib");
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "base",
        ],
    );
    tree
}

fn a_verify_against_head_with(verify: Arc<ATomlVerify>) -> Options {
    Options {
        command: Command::Verify,
        against: Some("HEAD".to_string()),
        verifies: vec![verify],
        ..Options::default()
    }
}

#[test]
fn a_changed_extension_file_is_handed_to_its_languages_verify() {
    // Given a worktree whose TOML config and Rust source both changed since HEAD
    let tree = a_committed_worktree();
    std::fs::write(
        tree.path().join("config/app.toml"),
        "[server]\nport = BROKEN\n",
    )
    .expect("change the config");
    std::fs::write(
        tree.path().join("src/lib.rs"),
        "pub fn one() -> u32 {\n    2\n}\n",
    )
    .expect("change the source");
    let toml = Arc::new(ATomlVerify::default());

    // When the sources are verified against HEAD
    let options = a_verify_against_head_with(toml.clone());
    let changed = runner::changed_sources(tree.path(), &options).expect("the changes are read");
    let findings =
        runner::verify_sources(tree.path(), &options, &changed).expect("the tree is verified");

    // Then the one changed TOML file — and neither the unchanged one nor the Rust file — was handed
    // over as it was and as it is, and what its verify found is the answer
    assert_eq!(
        (findings, toml.handed.lock().unwrap().clone()),
        (
            vec![a_finding_in("config/app.toml")],
            vec![SourceChange {
                path: "config/app.toml".to_string(),
                before: "[server]\nport = 8080\n".to_string(),
                after: "[server]\nport = BROKEN\n".to_string(),
            }]
        )
    );
}

#[test]
fn with_no_verify_registered_a_changed_extension_file_has_no_findings() {
    // Given a changed TOML file, and a run that registered no verify beyond Rust's
    let tree = a_committed_worktree();
    std::fs::write(
        tree.path().join("config/app.toml"),
        "[server]\nport = BROKEN\n",
    )
    .expect("change the config");
    let options = Options {
        command: Command::Verify,
        against: Some("HEAD".to_string()),
        ..Options::default()
    };

    // When the sources are verified
    let changed = runner::changed_sources(tree.path(), &options).expect("the changes are read");
    let findings =
        runner::verify_sources(tree.path(), &options, &changed).expect("the tree is verified");

    // Then there is nothing to report, as before extension verifies existed
    assert_eq!(findings, Vec::new());
}

#[test]
fn a_root_below_the_repositorys_top_level_hands_over_its_files_relative_to_itself() {
    // Given a worktree whose `config/` directory is the root, and whose TOML config there changed
    let tree = a_committed_worktree();
    let root = tree.path().join("config");
    std::fs::write(root.join("app.toml"), "[server]\nport = BROKEN\n").expect("change the config");
    let toml = Arc::new(ATomlVerify::default());
    let options = a_verify_against_head_with(toml.clone());

    // When the sources under that root are verified against HEAD
    let changed = runner::changed_sources(&root, &options).expect("the changes are read");
    let findings = runner::verify_sources(&root, &options, &changed).expect("the tree is verified");

    // Then the file is named relative to the root, read from the ref and the tree at that path
    assert_eq!(
        (findings, toml.handed.lock().unwrap().clone()),
        (
            vec![a_finding_in("app.toml")],
            vec![SourceChange {
                path: "app.toml".to_string(),
                before: "[server]\nport = 8080\n".to_string(),
                after: "[server]\nport = BROKEN\n".to_string(),
            }]
        )
    );
}
