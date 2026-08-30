//! Acceptance: the three scripts that put binaries on a host must ship `tddy-sandbox-runner`.
//!
//! Changeset: `docs/dev/1-WIP/2026-08-30-workspace-tool-sandbox.md`.
//!
//! Every jail this project spawns runs `tddy-sandbox-runner` inside itself — the sandboxed
//! `claude-cli` and `cursor-cli` sessions today, and the workspace tool jail this changeset adds.
//! `./release` builds six binaries and this is not one of them; `./install` ships five and
//! `publish.sh` packages five, and it is in neither. On a developer checkout the gap is invisible,
//! because `target/debug/tddy-sandbox-runner` is always there and
//! `sandbox_session::resolve_sandbox_runner_path` finds it as a sibling of `current_exe()`. On a
//! freshly installed host that fallback resolves to the bare name `"tddy-sandbox-runner"`, and the
//! first sandboxed start fails on a missing executable.
//!
//! `vm_workspace_tool_sandbox_acceptance.rs` asserts the same thing against a real guest, but it is
//! `#[ignore]`d and boots QEMU. These are the checks that run on every change: seconds, no VM.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The binary under discussion.
const RUNNER: &str = "tddy-sandbox-runner";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read_script(name: &str) -> String {
    let path = repo_root().join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// ./release — the runner has to be built before anything can ship it
// ---------------------------------------------------------------------------

/// `./install --build` and `publish.sh --build` both delegate here, so a runner missing from this
/// one line is a runner missing from every downstream artifact.
#[test]
fn the_release_script_builds_the_sandbox_runner_every_jail_spawns() {
    // Given
    let release = read_script("release");

    // When
    let builds_runner = release.contains(&format!("-p {RUNNER}"));

    // Then
    assert!(
        builds_runner,
        "`./release` must build {RUNNER}: every sandboxed session spawns it inside its jail, \
         and nothing downstream can ship a binary that was never built.\n{release}"
    );
}

// ---------------------------------------------------------------------------
// ./install — a functional run, not a grep
// ---------------------------------------------------------------------------

/// The binaries a real `install --systemd` run is given to copy. Deliberately the full set plus
/// the runner: an install that silently skipped an unexpected file would pass a narrower fixture.
const RELEASE_BINARIES: &[&str] = &[
    "tddy-supervisor",
    "tddy-daemon",
    "tddy-coder",
    "tddy-tools",
    "tddy-remote-git-repo",
    "tddy-session-sync",
    RUNNER,
];

fn copy_install_tree(dest: &Path) {
    fs::create_dir_all(dest).unwrap();
    let src = repo_root().join("install");
    let dst = dest.join("install");
    fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy install: {e}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dst).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dst, perms).unwrap();
    }
    for template in ["daemon.yaml.production", "supervisor.yaml.production"] {
        fs::copy(repo_root().join(template), dest.join(template))
            .unwrap_or_else(|e| panic!("copy {template}: {e}"));
    }
    let dist = dest.join("packages").join("tddy-web").join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("index.html"), "<!DOCTYPE html><html></html>\n").unwrap();
}

fn write_fake_release_binaries(root: &Path) {
    let rel = root.join("target").join("release");
    fs::create_dir_all(&rel).unwrap();
    for name in RELEASE_BINARIES {
        let p = rel.join(name);
        fs::write(&p, b"fake-binary\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).unwrap();
        }
    }
}

/// `resolve_sandbox_runner_path` looks for the runner beside the daemon before falling back to a
/// bare name, so "installed" means "in the same bin dir as `tddy-daemon`" — which is what this
/// asserts rather than merely "somewhere on the box".
#[test]
fn the_install_script_puts_the_sandbox_runner_beside_the_daemon() {
    // Given a checkout with every release binary built, and an install run pointed at temp dirs
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    copy_install_tree(root);
    write_fake_release_binaries(root);
    let bin_dir = root.join("custom-bin");

    // When
    let status = Command::new("bash")
        .current_dir(root)
        .arg(root.join("install"))
        .arg("--systemd")
        .env("INSTALL_NO_SYSTEMCTL", "1")
        .env("INSTALL_BIN_DIR", &bin_dir)
        .env("INSTALL_CONFIG_DIR", root.join("custom-etc"))
        .env("INSTALL_SYSTEMD_DIR", root.join("custom-systemd"))
        // Without this the script defaults to /usr/local/share and the run dies on permissions
        // rather than on anything this test is about.
        .env("INSTALL_WEB_BUNDLE_DIR", root.join("custom-web"))
        .status()
        .unwrap_or_else(|e| panic!("spawn install: {e}"));

    // Then
    assert!(status.success(), "install must succeed; got {status:?}");
    assert!(
        bin_dir.join("tddy-daemon").is_file(),
        "the fixture itself must be sound: the daemon should have been installed"
    );
    assert!(
        bin_dir.join(RUNNER).is_file(),
        "`./install` must ship {RUNNER} beside tddy-daemon in {}: the daemon spawns it inside \
         every jail, and resolves it as a sibling of its own executable. Installed: {:?}",
        bin_dir.display(),
        fs::read_dir(&bin_dir)
            .map(|d| d
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>())
            .unwrap_or_default()
    );
}

// ---------------------------------------------------------------------------
// publish.sh — the .deb an operator actually installs from
// ---------------------------------------------------------------------------

/// The apt package is how a host gets the stack without a checkout, so a runner missing here is
/// the same broken start with no local build to mask it.
#[test]
fn the_publish_script_packages_the_sandbox_runner_into_the_deb() {
    // Given
    let publish = read_script("publish.sh");

    // When — the script both *verifies* each release binary exists and *installs* it into the
    // staging tree; a runner in only one of the two lists is still a broken package.
    let verified = publish.contains(&format!("target/release/{RUNNER}"));
    let staged = publish.contains(&format!("${{STAGE}}/usr/bin/{RUNNER}"));

    // Then
    assert!(
        verified,
        "`publish.sh` must verify target/release/{RUNNER} is present before packaging"
    );
    assert!(
        staged,
        "`publish.sh` must install {RUNNER} into the package's /usr/bin: every sandboxed session \
         spawns it, and the .deb is how a host without a checkout gets the stack"
    );
}
