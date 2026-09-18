//! Static contract checks shared by the repo-root dev launchers, `web-dev` and `desktop-dev`.
//!
//! Both scripts start a long-lived stack (a daemon, or the Tauri app that hosts one) plus the
//! `tddy-web` Vite dev server, and both grew the same two opt-in preparation steps:
//!
//! * `--build` — build every cargo package whose binary the running stack can *invoke* (the
//!   session worker, the tool binary, the sandbox runner, the index daemon, the git shim, the
//!   worktree mirror), not just the one process the script launches itself. A missing sibling is
//!   not a build error: the resolvers fall back to a bare name on `PATH`, so the failure surfaces
//!   much later as a session that cannot start.
//! * `--resolve-private-repo` — install the JS dependencies through the local private npm
//!   registry instead of the public one.
//!
//! The checks are static (file reads plus `bash -n`); nothing here starts a server or a build.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Repo-root file holding the one list of runtime-invocable cargo packages.
pub const RUNTIME_BINARIES_SCRIPT: &str = "scripts/dev-runtime-binaries.sh";

/// Shell array declared by [`RUNTIME_BINARIES_SCRIPT`].
pub const RUNTIME_PACKAGES_ARRAY: &str = "DEV_RUNTIME_PACKAGES";

/// The package-manager script both launchers delegate `--resolve-private-repo` to.
pub const LOCAL_REGISTRY_SCRIPT: &str = "local-registry-install";

/// `bash -n` must accept the script.
pub fn verify_syntax(path: &Path) {
    let path_str = path.to_str().expect("dev script path must be UTF-8");
    let status = Command::new("bash")
        .args(["-n", path_str])
        .status()
        .unwrap_or_else(|e| panic!("spawn bash -n for {path_str}: {e}"));
    assert!(
        status.success(),
        "bash -n {path_str} must exit 0 (syntax); got {:?}",
        status.code()
    );
}

/// Returns true if the script's usage block documents `flag`.
///
/// Usage lines are the only place a developer looks for a flag, and both scripts pass unrecognised
/// arguments straight through to the process they launch — so an undocumented flag is invisible
/// *and* silently reinterpreted by the daemon or by `tauri dev`.
pub fn documents_flag_in_usage(contents: &str, flag: &str) -> bool {
    let documented = contents
        .lines()
        .take_while(|l| l.starts_with('#') || l.trim().is_empty())
        .any(|l| l.contains(flag));
    log::debug!("documents_flag_in_usage flag={flag} documented={documented}");
    documented
}

/// Returns true if the script matches `flag` as its own argument rather than forwarding it.
pub fn consumes_flag(contents: &str, flag: &str) -> bool {
    let consumed = contents.contains(&format!("{flag})"))
        || contents.contains(&format!("\"{flag}\""))
        || contents.contains(&format!("= \"{flag}\""));
    log::debug!("consumes_flag flag={flag} consumed={consumed}");
    consumed
}

/// PRD: `--build` and `--resolve-private-repo` are documented, parsed, and consumed by the script
/// itself — never appended to the arguments handed to `tddy-daemon` or to `tauri dev`.
pub fn verify_declares_and_consumes_new_flags(contents: &str, label: &str) {
    log::info!("verify_declares_and_consumes_new_flags: start label={label}");
    for flag in ["--build", "--resolve-private-repo"] {
        assert!(
            documents_flag_in_usage(contents, flag),
            "{label} must document {flag} in its usage header"
        );
        assert!(
            consumes_flag(contents, flag),
            "{label} must match {flag} as its own argument, not forward it to the process it \
             launches"
        );
    }
    log::info!("verify_declares_and_consumes_new_flags: ok");
}

/// PRD: `--build` builds the runtime-invocable set from the one shared list, so the two launchers
/// and the list cannot drift apart.
pub fn verify_build_flag_uses_shared_runtime_list(contents: &str, label: &str) {
    log::info!("verify_build_flag_uses_shared_runtime_list: start label={label}");
    assert!(
        contents.contains(RUNTIME_BINARIES_SCRIPT),
        "{label} must source {RUNTIME_BINARIES_SCRIPT} rather than hardcode its own package list"
    );
    assert!(
        contents.contains(RUNTIME_PACKAGES_ARRAY),
        "{label} must build the {RUNTIME_PACKAGES_ARRAY} list that {RUNTIME_BINARIES_SCRIPT} \
         declares"
    );
    assert!(
        contents.contains("cargo build"),
        "{label} --build must run a cargo build"
    );
    log::info!("verify_build_flag_uses_shared_runtime_list: ok");
}

/// PRD: `--resolve-private-repo` delegates to the repo's existing local-registry install
/// (`bun run local-registry-install` — lock resolution plus a registry-pinned install), never a
/// bare `bun install`, which would resolve against the public registry the flag exists to avoid.
pub fn verify_private_repo_flag_delegates_to_local_registry(contents: &str, label: &str) {
    log::info!("verify_private_repo_flag_delegates_to_local_registry: start label={label}");
    assert!(
        contents.contains(LOCAL_REGISTRY_SCRIPT),
        "{label} --resolve-private-repo must delegate to `bun run {LOCAL_REGISTRY_SCRIPT}`"
    );
    for line in contents.lines() {
        let code = line.split('#').next().unwrap_or("");
        assert!(
            !code.contains("bun install"),
            "{label} must not run a bare `bun install` (public registry); use \
             {LOCAL_REGISTRY_SCRIPT}: {line}"
        );
    }
    log::info!("verify_private_repo_flag_delegates_to_local_registry: ok");
}

/// PRD: every binary `install` ships as an invocable sibling is in the shared `--build` list.
///
/// `install`'s `DESKTOP_BINARIES` is the production statement of "these binaries must sit beside
/// the daemon", and the resolvers in the running stack look for exactly those next to
/// `current_exe()`. A dev run resolves the same way against `target/debug`, so the two lists are
/// the same contract and this is what keeps them from drifting.
pub fn verify_runtime_list_covers_installed_siblings(runtime_list: &str, install_contents: &str) {
    log::info!("verify_runtime_list_covers_installed_siblings: start");
    assert!(
        runtime_list.contains(RUNTIME_PACKAGES_ARRAY),
        "{RUNTIME_BINARIES_SCRIPT} must declare {RUNTIME_PACKAGES_ARRAY}"
    );
    let desktop_binaries = install_contents
        .lines()
        .find(|l| l.trim_start().starts_with("DESKTOP_BINARIES="))
        .expect("install must declare a DESKTOP_BINARIES list");
    let declared: Vec<&str> = desktop_binaries
        .trim_end_matches(')')
        .split('(')
        .nth(1)
        .expect("DESKTOP_BINARIES must be a shell array")
        .split_whitespace()
        .collect();
    assert!(
        !declared.is_empty(),
        "install's DESKTOP_BINARIES must name at least one binary"
    );
    for name in declared {
        assert!(
            runtime_list.contains(name),
            "{RUNTIME_BINARIES_SCRIPT} must list {name}: install ships it as a sibling of the \
             daemon, so a dev run has to build it into target/debug too"
        );
    }
    log::info!("verify_runtime_list_covers_installed_siblings: ok");
}

/// Reads a repo-root file, panicking with the path on failure.
pub fn read_repo_file(repo_root: &Path, relative: &str) -> String {
    let path = repo_root.join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}
