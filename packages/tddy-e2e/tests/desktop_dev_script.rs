//! Acceptance tests for `desktop-dev` — the Tauri-app counterpart of `web-dev`.
//! Static contract checks — no app, no build. See [`tddy_e2e::dev_script_contract`].

use std::path::PathBuf;

use tddy_e2e::dev_script_contract::{
    read_repo_file, verify_build_flag_uses_shared_runtime_list,
    verify_declares_and_consumes_new_flags, verify_private_repo_flag_delegates_to_local_registry,
    verify_syntax,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read_desktop_dev() -> String {
    read_repo_file(&repo_root(), "desktop-dev")
}

/// `bash -n` must accept `desktop-dev`.
#[test]
fn desktop_dev_bash_syntax_is_valid() {
    // Given
    let path = repo_root().join("desktop-dev");

    // When / Then
    verify_syntax(&path);
}

/// `--build` and `--resolve-private-repo` must be documented and consumed by the script, not
/// forwarded to `tauri dev` — which accepts neither and would fail on them.
#[test]
fn desktop_dev_declares_build_and_private_repo_flags() {
    // Given
    let contents = read_desktop_dev();

    // When / Then
    verify_declares_and_consumes_new_flags(&contents, "desktop-dev");
}

/// `--build` must build the shared runtime-invocable package list, so the app's sibling binaries
/// land in `target/debug` beside the `tddy-desktop` binary `tauri dev` produces.
#[test]
fn desktop_dev_build_flag_uses_shared_runtime_list() {
    // Given
    let contents = read_desktop_dev();

    // When / Then
    verify_build_flag_uses_shared_runtime_list(&contents, "desktop-dev");
}

/// `--resolve-private-repo` must go through `bun run local-registry-install`.
#[test]
fn desktop_dev_private_repo_flag_uses_local_registry() {
    // Given
    let contents = read_desktop_dev();

    // When / Then
    verify_private_repo_flag_delegates_to_local_registry(&contents, "desktop-dev");
}
