//! VM-backed cgroups acceptance tests — the properties only a real kernel can prove.
//!
//! On macOS `tddy-sandbox-cgroups` compiles to an empty lib, and on Linux every function
//! that touches `/sys/fs/cgroup` is either never executed by a test or exercised against a
//! `tempfile::tempdir()` standing in for cgroupfs. A temp directory accepts any bytes,
//! never enforces a limit, and returns `ENOTEMPTY` forever where the kernel returns
//! `EBUSY` — so the scope-removal retry path and its success path have never run at all.
//!
//! These tests put the real thing in a guest: binaries built in a builder VM (an
//! Apple-Silicon host cannot emit Linux/aarch64 ELF), deployed into a lean Debian guest
//! running the stock `-cloud` kernel, installed with the real `./install --systemd`.
//!
//! ## Production tests — manual trigger only
//!
//! `#[ignore]`d *and* gated on a configured base image, so `./test`, `./verify` and plain
//! `cargo test` are unaffected. Nothing is ever downloaded: point the testkit at a cloud
//! image already on your disk, in the environment or in the repo-root `.env`.
//!
//! ```sh
//! # .env at the repo root, or exported:
//! #   TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-arm64.qcow2
//! ./dev cargo test -p tddy-e2e --test vm_cgroups_acceptance -- --ignored --test-threads=1
//! ```
//!
//! The first run bakes three images and takes hours; every run after that reuses them and
//! costs a boot plus an incremental `./release`. `/dev/kvm` must be *openable*, not merely
//! present — under TCG even the warm path is measured in hours.

use std::time::Duration;

use serial_test::serial;
use tddy_vm_testkit::env_file::{configured_base_image, BASE_IMAGE_ENV};
use tddy_vm_testkit::{BuilderVm, BuiltBinaries, TestHostVm, TestkitLayout, VmArch};

/// ELF header bytes: magic, then `e_machine` at offset 18.
const ELF_MAGIC: &[u8] = b"\x7fELF";
const EM_AARCH64: u16 = 0xB7;
const EM_X86_64: u16 = 0x3E;

/// The `e_machine` a guest-built binary must carry on this host.
///
/// The builder guest runs the host's own architecture — it is hardware-accelerated, so it
/// runs nothing else, and the test host it hands binaries to runs the same one. The claim
/// under assertion is that the binary was produced in a *Linux guest* for this
/// architecture, not that it is aarch64: on an x86_64 host, aarch64 would be the wrong
/// answer, and the host toolchain's Mach-O would be the wrong answer on either.
fn host_e_machine() -> u16 {
    match VmArch::host() {
        VmArch::Aarch64 => EM_AARCH64,
        VmArch::X86_64 => EM_X86_64,
    }
}

/// Progress to stderr, so a multi-hour first run is not a silent one.
fn progress(line: &str) {
    eprintln!("[vm-cgroups] {line}");
}

/// The gate every test in this file shares.
macro_rules! require_base_image {
    () => {
        if configured_base_image().is_none() {
            eprintln!(
                "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
            );
            return;
        }
    };
}

/// Build the workspace in the builder guest. Shared by every test that needs binaries.
async fn built_binaries() -> BuiltBinaries {
    BuilderVm::for_this_repo()
        .build_release(&progress)
        .await
        .expect("the builder guest must produce Linux binaries")
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and builds the workspace inside it; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_cgroups)]
async fn builds_deployable_linux_binaries_on_a_host_that_cannot_compile_them() {
    require_base_image!();

    // Given a macOS host, which cannot emit Linux/aarch64 ELF at all

    // When the builder guest is asked for a release build
    let built = built_binaries().await;

    // Then every binary the supervised stack needs is on the host, and every one of them is a
    // Linux executable for this host's architecture rather than something the host toolchain
    // produced
    let expected_machine = host_e_machine();
    for path in &built.binaries {
        let bytes = std::fs::read(path).expect("a built binary must be readable on the host");
        assert!(
            bytes.starts_with(ELF_MAGIC),
            "{} is not an ELF binary — the build did not happen in the guest",
            path.display()
        );
        let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
        assert_eq!(
            machine,
            expected_machine,
            "{} targets machine {machine:#x}, not this host's {expected_machine:#x}",
            path.display()
        );
    }
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and installs the supervised stack; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_cgroups)]
async fn installs_a_supervisor_that_really_delegates_a_writable_cgroup_subtree() {
    require_base_image!();

    // Given a guest with the freshly built stack installed by the real ./install --systemd
    let built = built_binaries().await;
    let host = TestHostVm::start(&TestkitLayout::for_this_repo(), &built, &progress)
        .await
        .expect("the test host must install the supervised stack");

    // When the supervisor's delegated subtree is inspected on real cgroupfs
    let controllers = host
        .guest()
        .run_over_ssh("cat /sys/fs/cgroup/tddy.slice/cgroup.subtree_control")
        .await
        .expect("the delegated subtree must exist");

    // Then the controllers the supervisor asked for are actually delegated. Today only the
    // *text* of the `Delegate=yes` unit is asserted (install_supervisor.rs:265-282), which
    // says nothing about whether the kernel honoured it
    for controller in ["memory", "pids"] {
        assert!(
            controllers.stdout().contains(controller),
            "the {controller} controller was not delegated: {controllers}"
        );
    }

    host.shutdown().await.expect("the guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and exercises real cgroupfs; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_cgroups)]
async fn removes_an_emptied_scope_but_refuses_a_populated_one() {
    require_base_image!();

    // Given a guest with a scope holding a live process
    let built = built_binaries().await;
    let host = TestHostVm::start(&TestkitLayout::for_this_repo(), &built, &progress)
        .await
        .expect("the test host must install the supervised stack");
    let scope = "/sys/fs/cgroup/tddy.slice/rmdir-probe.scope";

    host.guest()
        .run_over_ssh(&format!(
            "set -e; sudo mkdir -p {scope}; sudo sh -c 'sleep 300 & echo $! > \
             {scope}/cgroup.procs'"
        ))
        .await
        .expect("a scope holding a live process must be creatable");

    // When removal is attempted while it is populated
    let populated = host
        .guest()
        .run_over_ssh_once(
            &format!("sudo rmdir {scope} 2>&1; echo exit=$?"),
            Duration::from_secs(60),
        )
        .await
        .expect("the removal attempt must run");

    // Then the kernel refuses with EBUSY — not the ENOTEMPTY a plain directory returns.
    // This is the distinction `classify_removal_failure` is built around and which its
    // unit tests can only fabricate with `io::Error::from_raw_os_error`
    assert!(
        populated.stdout().contains("Device or resource busy"),
        "a populated scope must be refused with EBUSY, got: {populated}"
    );

    // When the scope is emptied and removal is retried
    let emptied = host
        .guest()
        .run_over_ssh_once(
            &format!(
                "set -e; sudo sh -c 'kill $(cat {scope}/cgroup.procs) 2>/dev/null || true'; \
                 sleep 1; sudo rmdir {scope} && echo removed"
            ),
            Duration::from_secs(60),
        )
        .await
        .expect("the retry must run");

    // Then it succeeds — the success path that never executes against a temp directory,
    // because an emptied plain directory still returns ENOTEMPTY forever
    emptied.assert_succeeded();

    host.shutdown().await.expect("the guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and exercises real cgroupfs; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_cgroups)]
async fn enforces_the_process_limit_written_into_a_scope() {
    require_base_image!();

    // Given a scope whose pids.max the kernel actually honours
    let built = built_binaries().await;
    let host = TestHostVm::start(&TestkitLayout::for_this_repo(), &built, &progress)
        .await
        .expect("the test host must install the supervised stack");
    let scope = "/sys/fs/cgroup/tddy.slice/limit-probe.scope";

    host.guest()
        .run_over_ssh(&format!(
            "set -e; sudo mkdir -p {scope}; echo 2 | sudo tee {scope}/pids.max > /dev/null"
        ))
        .await
        .expect("a scope with a process limit must be creatable");

    // When a shell inside it tries to exceed that limit
    let forked = host
        .guest()
        .run_over_ssh_once(
            &format!(
                "sudo sh -c 'echo $$ > {scope}/cgroup.procs; \
                 for i in 1 2 3 4 5 6 7 8; do sleep 5 & done; wait' 2>&1; echo exit=$?"
            ),
            Duration::from_secs(120),
        )
        .await
        .expect("the fork attempt must run");

    // Then the kernel refuses the fork with EAGAIN, which is what a pids-controller
    // rejection reports. A temp directory accepts `2` into a file named `pids.max` and then
    // enforces nothing at all, so `write_cgroup_limits` has never been shown to produce a
    // limit rather than merely a file.
    //
    // Deliberately not matching ENOMEM ("Cannot allocate memory") as well: that is what an
    // unrelated guest OOM prints, and accepting it would let this pass on a failure mode
    // that is not the one under test.
    assert!(
        forked.stdout().contains("Resource temporarily unavailable"),
        "pids.max=2 must stop the ninth fork with EAGAIN, got: {forked}"
    );

    host.shutdown().await.expect("the guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM and installs the supervised stack; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_cgroups)]
async fn runs_the_daemon_unprivileged_under_a_root_supervisor() {
    require_base_image!();

    // Given a guest running the installed stack
    let built = built_binaries().await;
    let host = TestHostVm::start(&TestkitLayout::for_this_repo(), &built, &progress)
        .await
        .expect("the test host must install the supervised stack");

    // When the two processes' real users are read from the kernel
    let supervisor_user = host
        .guest()
        .run_over_ssh("ps -o user= -C tddy-supervisor | head -1")
        .await
        .expect("the supervisor must be running");
    let daemon_user = host
        .guest()
        .run_over_ssh("ps -o user= -C tddy-daemon | head -1")
        .await
        .expect("the daemon must be running");

    // Then the privilege split the feature claims is real: a root supervisor, and a daemon
    // that already dropped. The existing 33 supervisor acceptance tests declare the
    // *invoking* user as the service user, so `privilege_to_drop` returns None and no drop
    // is ever planned, let alone performed
    supervisor_user.assert_stdout_line("root");
    daemon_user.assert_stdout_line("tddy");

    host.shutdown().await.expect("the guest must shut down");
}
