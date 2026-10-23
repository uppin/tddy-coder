//! Test fixtures for the crates that verify, anchor or serve an installed extension language
//! against this crate's `demo-toml` fixture: one builder instead of a copy per suite (feature
//! `testkit`, enabled by dependents' dev-dependencies only, never by a production build).
//!
//! Every function takes the directory it fills, so the caller owns the temporary directory and its
//! lifetime.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace file a `demo-toml` suite changes, relative to the workspace root.
pub const APP: &str = "config/app.toml";

/// [`APP`]'s committed text: three tables, `port = 8080` on line 3. The same text this crate's own
/// suites hold in `tests/outline_support`.
pub const APP_TOML: &str = "[server]\nhost = \"localhost\"\nport = 8080\n\n[database]\nurl = \"postgres://localhost/app\"\npool = 4\n\n[logging]\nlevel = \"info\"\n";

/// The extension `demo-toml` installs under, inside a data dir.
const DEMO_TOML: &str = "extensions/installed/demo-toml";

/// This crate's test fixtures: `demo-toml` and the vendored grammars.
fn the_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Install `demo-toml` — the fixture extension and its vendored TOML grammar — under the tddy data
/// dir `data`, in Zed's installed layout.
pub fn install_demo_toml(data: &Path) {
    let installed = data.join(DEMO_TOML);
    copy_tree(&the_fixtures().join("demo-toml"), &installed);
    fs::create_dir_all(installed.join("grammars")).expect("create the grammars dir");
    fs::copy(
        the_fixtures().join("grammars/tree-sitter-toml/toml.wasm"),
        installed.join("grammars/toml.wasm"),
    )
    .expect("copy the vendored grammar");
}

/// Append `table` — `[language_servers.<id>]` entries — to the manifest of the `demo-toml`
/// [`install_demo_toml`] installed under `data`.
pub fn declare_in_demo_toml(data: &Path, table: &str) {
    let manifest = data.join(DEMO_TOML).join("extension.toml");
    let mut text = fs::read_to_string(&manifest).expect("read the installed manifest");
    text.push('\n');
    text.push_str(table);
    fs::write(&manifest, text).expect("write the installed manifest");
}

/// Write `files` — paths relative to `workspace` and their text — and commit them as the baseline
/// of a new git repository at `workspace`. Signing is off, so a developer's signing setup cannot
/// fail or stall the commit.
pub fn commit_baseline(workspace: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let file = workspace.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).expect("create the file's directory");
        }
        fs::write(&file, text).expect("write a baseline file");
    }
    for args in [
        &["init", "--quiet"][..],
        &["add", "--all"],
        &[
            "-c",
            "user.name=tddy tests",
            "-c",
            "user.email=tests@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--message",
            "baseline",
        ],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(workspace)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create the target");
    for entry in fs::read_dir(from).expect("read a fixture dir") {
        let entry = entry.expect("a fixture entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy a fixture file");
        }
    }
}
