//! The index daemon answers an installed extension language's anchors as the cold CLI does
//! (`#reshape` extension-outline, PRD: "the same anchors come back through the index daemon as
//! through the cold CLI").
//!
//! Spawns the real binary, as `dual_transport_acceptance` does, with `TDDY_DATA_DIR` naming a data
//! dir whose extensions dir holds `tddy-zed-extensions`' `demo-toml` fixture (the vendored
//! tree-sitter-toml grammar) in Zed's installed layout. The rust-analyzer it is given does not
//! exist: a file an extension language claims must need no language server.

use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_code_restructuring::{Anchor, Fingerprint, ItemPath};

const APP: &str = "config/app.toml";
const APP_TOML: &str = "[server]\nhost = \"localhost\"\nport = 8080\n\n[database]\nurl = \"postgres://localhost/app\"\npool = 4\n\n[logging]\nlevel = \"info\"\n";

/// The binary under test, resolved as `dual_transport_acceptance` resolves it.
fn the_index_daemon() -> PathBuf {
    if let Some(from_cargo) = std::env::var_os("CARGO_BIN_EXE_tddy-index-daemon") {
        return PathBuf::from(from_cargo);
    }
    let mut candidate = std::env::current_exe().expect("the test executable's own path");
    candidate.pop();
    if candidate.ends_with("deps") {
        candidate.pop();
    }
    candidate.join("tddy-index-daemon")
}

fn the_extension_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tddy-zed-extensions/tests/fixtures")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the target");
    for entry in std::fs::read_dir(from).expect("read a fixture dir") {
        let entry = entry.expect("a fixture entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a fixture file");
        }
    }
}

/// A tddy data dir with `demo-toml` installed: the fixture extension plus the vendored grammar as
/// its `grammars/toml.wasm`.
fn a_data_dir_with_demo_toml_installed() -> tempfile::TempDir {
    let data = tempfile::tempdir().expect("a temporary data dir");
    let installed = data.path().join("extensions/installed/demo-toml");
    copy_tree(&the_extension_fixtures().join("demo-toml"), &installed);
    std::fs::create_dir_all(installed.join("grammars")).expect("grammars dir");
    std::fs::copy(
        the_extension_fixtures().join("grammars/tree-sitter-toml/toml.wasm"),
        installed.join("grammars/toml.wasm"),
    )
    .expect("copy the vendored grammar");
    data
}

/// A workspace holding `config/app.toml` and no Cargo package.
fn a_workspace_holding_the_app_config() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("config")).expect("config dir");
    std::fs::write(workspace.path().join(APP), APP_TOML).expect("write the app config");
    workspace
}

/// What the daemon's single-shot run said, without the log prefix.
fn rendered(stderr: &[u8]) -> Vec<String> {
    const RENDERED_BY: &str = "[tddy_index_daemon::main] ";
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|line| {
            line.split_once(RENDERED_BY)
                .map(|(_, said)| said.to_string())
        })
        .collect()
}

/// Lines `first..=last` of the app config, fingerprinted as every item anchor is.
fn the_fingerprint_of_lines(first: usize, last: usize) -> Fingerprint {
    let lines: Vec<&str> = APP_TOML.split('\n').collect();
    Fingerprint::of(&lines[first - 1..last].join("\n"))
}

/// The anchor the cold CLI prints for `anchors config/app.toml --items server,database` — the one
/// `tddy-zed-extensions`' outline suite pins at library level, rendered by the console both front
/// ends share.
fn the_anchor_the_cli_prints() -> String {
    tddy_code_restructuring::console::item_anchor(&Anchor::Items {
        file: APP.to_string(),
        items: ["server", "database"]
            .map(|name| ItemPath::parse(&format!("{APP}::{name}")).expect("an item path"))
            .to_vec(),
        fingerprints: vec![
            the_fingerprint_of_lines(1, 3),
            the_fingerprint_of_lines(5, 7),
        ],
    })
}

#[test]
fn the_daemon_answers_the_same_anchors_as_the_cli() {
    // Given a workspace with a TOML file, and the demo-toml extension installed
    let data = a_data_dir_with_demo_toml_installed();
    let workspace = a_workspace_holding_the_app_config();

    // When the daemon is asked, in process, for an anchor over two adjacent tables
    let run = Command::new(the_index_daemon())
        .env("TDDY_DATA_DIR", data.path())
        .args([
            "--rust-analyzer",
            "/nonexistent/rust-analyzer",
            "restructure",
            "anchors",
            "--workspace-root",
            &workspace.path().to_string_lossy(),
            APP,
            "--items",
            "server,database",
        ])
        .output()
        .expect("the binary runs");

    // Then it prints the anchor the cold CLI prints, and succeeds without a language server
    assert_eq!(
        (rendered(&run.stderr), run.status.success()),
        (vec![the_anchor_the_cli_prints()], true),
        "stderr was: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}
