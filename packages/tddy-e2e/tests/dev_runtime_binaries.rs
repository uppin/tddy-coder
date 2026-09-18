//! Acceptance test for `scripts/dev-runtime-binaries.sh` — the one list of cargo packages whose
//! binaries a running dev stack can invoke, shared by `web-dev --build` and `desktop-dev --build`.

use std::path::PathBuf;

use tddy_e2e::dev_script_contract::{
    read_repo_file, verify_runtime_list_covers_installed_siblings, verify_syntax,
    RUNTIME_BINARIES_SCRIPT,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// `bash -n` must accept the sourced list — both launchers `source` it under `set -euo pipefail`,
/// so a syntax error there kills the launcher rather than the build.
#[test]
fn dev_runtime_binaries_list_bash_syntax_is_valid() {
    // Given
    let path = repo_root().join(RUNTIME_BINARIES_SCRIPT);

    // When / Then
    verify_syntax(&path);
}

/// Every binary `install` ships beside the daemon must be in the `--build` list: the resolvers in
/// the running stack look for those names next to `current_exe()`, and a dev run resolves the same
/// way against `target/debug`.
#[test]
fn dev_runtime_binaries_list_covers_every_installed_sibling() {
    // Given
    let root = repo_root();
    let runtime_list = read_repo_file(&root, RUNTIME_BINARIES_SCRIPT);
    let install = read_repo_file(&root, "install");

    // When / Then
    verify_runtime_list_covers_installed_siblings(&runtime_list, &install);
}
