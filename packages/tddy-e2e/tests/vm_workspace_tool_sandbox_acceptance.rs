//! VM-backed acceptance: what a Linux workspace tool jail needs from a real host.
//!
//! PRD: `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.
//! Changeset: `docs/dev/changesets/`, 2026-08-30 workspace tool sandbox.
//!
//! The macOS half of the confinement claim runs against a real Seatbelt jail in
//! `tddy-daemon/tests/workspace_tool_sandbox_seatbelt_acceptance.rs`, in-process. Linux has no
//! in-process equivalent: `tddy-sandbox-cgroups` compiles to an empty lib on macOS, and on Linux a
//! `tempfile::tempdir()` standing in for cgroupfs accepts any bytes and enforces nothing — so the
//! properties a workspace jail depends on have to be asserted against a real kernel, in a guest
//! running the stack the way an operator installs it.
//!
//! ## Production tests — manual trigger only
//!
//! `#[ignore]`d *and* gated on a configured base image, so `./test`, `./verify` and plain
//! `cargo test` are unaffected. Nothing is ever downloaded: point the testkit at a cloud image
//! already on your disk, in the environment or in the repo-root `.env`.
//!
//! ```sh
//! # .env at the repo root, or exported:
//! #   TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-arm64.qcow2
//! ./vm-tests vm_workspace_tool_sandbox
//! ```
//!
//! Warm the image cache with `./run-vm-testkit` first, or the first run bakes three images and
//! takes hours.

use std::time::Duration;

use serial_test::serial;
use tddy_vm_testkit::env_file::BASE_IMAGE_ENV;
use tddy_vm_testkit::{BuilderVm, BuiltBinaries, TestHostVm, TestkitLayout, VmArch};

/// ELF header bytes: magic, then `e_machine` at offset 18.
const ELF_MAGIC: &[u8] = b"\x7fELF";
const EM_AARCH64: u16 = 0xB7;
const EM_X86_64: u16 = 0x3E;

/// The binary a workspace jail spawns inside itself.
const RUNNER: &str = "tddy-sandbox-runner";

/// Progress to stderr, so a multi-hour first run is not a silent one.
fn progress(line: &str) {
    eprintln!("[vm-workspace-sandbox] {line}");
}

/// The gate every test in this file shares.
///
/// A missing image is a failure, not a skip: returning early would report success for a
/// test that booted nothing, and an unset variable is precisely what a mistyped path or a
/// failed download looks like. `./vm-tests` reports a deliberate absence once, up front.
macro_rules! require_base_image {
    () => {
        let _ = tddy_vm_testkit::require_env_path(BASE_IMAGE_ENV);
    };
}

fn host_e_machine() -> u16 {
    match VmArch::host() {
        VmArch::Aarch64 => EM_AARCH64,
        VmArch::X86_64 => EM_X86_64,
    }
}

async fn built_binaries() -> BuiltBinaries {
    BuilderVm::for_this_repo()
        .build_release(&progress)
        .await
        .expect("the builder guest must produce Linux binaries")
}

/// A sandboxed workspace session spawns `tddy-sandbox-runner` inside its jail. `./release` builds
/// six binaries and this is not one of them, so on a freshly installed Linux host the very first
/// sandboxed workspace start would fail on a missing executable — the one failure mode no amount
/// of in-process testing can see, because a developer checkout always has it in `target/debug`.
#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and builds the workspace inside it; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_workspace_sandbox)]
async fn the_release_build_produces_the_sandbox_runner_a_workspace_jail_spawns() {
    require_base_image!();

    // Given a release build produced in the builder guest
    let built = built_binaries().await;

    // When the runner is looked for among its outputs
    let runner = built
        .binaries
        .iter()
        .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(RUNNER));

    // Then it is there, and it is a Linux executable for this host's architecture rather than
    // something the host toolchain produced
    let runner = runner.unwrap_or_else(|| {
        panic!(
            "`./release` must build {RUNNER} — a workspace jail spawns it. Built: {:?}",
            built
                .binaries
                .iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
                .collect::<Vec<_>>()
        )
    });
    let bytes = std::fs::read(runner).expect("the runner must be readable on the host");
    assert!(
        bytes.starts_with(ELF_MAGIC),
        "{} is not an ELF binary — the build did not happen in the guest",
        runner.display()
    );
    let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
    assert_eq!(machine, host_e_machine());
}

/// Shipping it is a second, separate claim: `./install --systemd` chooses what lands on a host, and
/// a runner that is built but not installed leaves the same broken start behind.
#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and installs the supervised stack; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_workspace_sandbox)]
async fn the_installed_stack_puts_the_sandbox_runner_on_the_hosts_path() {
    require_base_image!();

    // Given a guest with the freshly built stack installed by the real ./install --systemd
    let built = built_binaries().await;
    let host = TestHostVm::start(&TestkitLayout::for_this_repo(), &built, &progress)
        .await
        .expect("the test host must install the supervised stack");

    // When the runner is looked for the way the daemon resolves it — on PATH
    let found = host
        .guest()
        .run_over_ssh_once(
            &format!("command -v {RUNNER} || echo MISSING"),
            Duration::from_secs(60),
        )
        .await
        .expect("the lookup must run");

    // Then the daemon would find it
    assert!(
        !found.stdout().contains("MISSING"),
        "`./install --systemd` must ship {RUNNER}: a sandboxed workspace session spawns it, \
         and a host without it fails every such start; got: {found}"
    );
}

/// A Linux workspace jail is a delegated cgroup scope under `tddy.slice`, created per session by
/// the unprivileged daemon. Whether the delegation the supervisor sets up actually lets that
/// daemon create one is the kernel's answer, and a temp directory standing in for cgroupfs would
/// say yes to anything.
#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and exercises real cgroupfs; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_workspace_sandbox)]
async fn the_unprivileged_daemon_can_create_the_per_session_scope_a_workspace_jail_needs() {
    require_base_image!();

    // Given a guest running the installed stack, where the daemon is unprivileged
    let built = built_binaries().await;
    let host = TestHostVm::start(&TestkitLayout::for_this_repo(), &built, &progress)
        .await
        .expect("the test host must install the supervised stack");
    let scope = "/sys/fs/cgroup/tddy.slice/workspace-jail-probe.scope";

    // When that user — not root — creates a per-session scope inside the delegated subtree
    let created = host
        .guest()
        .run_over_ssh_once(
            &format!("sudo -u tddy mkdir -p {scope} 2>&1; echo exit=$?"),
            Duration::from_secs(60),
        )
        .await
        .expect("the creation attempt must run");

    // Then the delegation holds for the user that will actually do it at session start
    assert!(
        created.stdout().contains("exit=0"),
        "the unprivileged daemon user must be able to create its own session scope under the \
         delegated subtree; got: {created}"
    );

    // And the scope it made is one the kernel will enforce limits in, not merely a directory
    host.guest()
        .run_over_ssh(&format!(
            "echo 2 | sudo -u tddy tee {scope}/pids.max > /dev/null"
        ))
        .await
        .expect("the daemon user must be able to write its own scope's limits");
    let forked = host
        .guest()
        .run_over_ssh_once(
            &format!(
                "sudo -u tddy sh -c 'echo $$ > {scope}/cgroup.procs; \
                 for i in 1 2 3 4 5 6 7 8; do sleep 5 & done; wait' 2>&1; echo exit=$?"
            ),
            Duration::from_secs(120),
        )
        .await
        .expect("the fork attempt must run");
    assert!(
        forked.stdout().contains("Resource temporarily unavailable"),
        "pids.max=2 must stop the ninth fork with EAGAIN inside the session scope, got: {forked}"
    );

    host.shutdown().await.expect("the guest must shut down");
}
