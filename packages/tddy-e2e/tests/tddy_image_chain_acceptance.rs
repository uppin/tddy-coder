//! The whole image chain in one pass: a cloud-init base with Nix, the two flavours derived
//! from it, and tddy actually running in the second one.
//!
//! ```text
//! debian cloud image
//!   └── tddy-nix-base     cloud-init + Nix with flakes          ← the base flavour
//!         ├── tddy-builder   9p kernel + warmed dev shell        ← flavour 1: compiles tddy
//!         └── tddy-test-host lean guest, stock -cloud kernel     ← flavour 2: runs tddy
//! ```
//!
//! Each layer is asserted where it is the only thing that can answer:
//!
//! - **base** — a VM booted off `tddy-nix-base` resolves `nix` into `/nix/…` and runs it, so
//!   the layer carries a working package manager rather than a `runcmd` that failed
//!   silently. cloud-init concatenates `runcmd` into one script with no error handling of
//!   its own, which is why the recipe opens with `set -e`; a bake whose provisioning died
//!   still boots and still accepts SSH.
//! - **builder** — `./release` runs *inside* it and hands back Linux binaries. This is the
//!   flavour's entire purpose: an Apple-Silicon host cannot emit Linux ELF, so the guest is
//!   a prerequisite rather than a convenience.
//! - **test host** — those binaries are installed by the real `./install --systemd
//!   --headless`, and the supervised stack is asserted *from the kernel's process table*:
//!   a root supervisor with the daemon already dropped to its service account.
//!
//! ## Production test — manual trigger only
//!
//! `#[ignore]`d and gated on `TDDY_CLOUDINIT_BASE_IMAGE`; a missing image fails rather than
//! skips. **The first run bakes three images and takes hours** — the builder realises a
//! multi-gigabyte dev shell over slirp and then compiles the workspace, `libwebrtc`
//! included, inside a guest. Later runs reuse the sealed layers and cost a boot plus an
//! incremental `./release`. `/dev/kvm` must be *openable*, not merely present.
//!
//! ```sh
//! ./dev cargo test -p tddy-e2e --test tddy_image_chain_acceptance -- --ignored --nocapture
//! ```

use std::time::Duration;

use serial_test::serial;
use tddy_vm_testkit::env_file::BASE_IMAGE_ENV;
use tddy_vm_testkit::recipes::{SOURCE_NIX_PROFILE, TDDY_SERVICE_USERNAME};
use tddy_vm_testkit::{
    boot_probe_of_prepared_base, BuilderVm, TestHostVm, TestkitLayout, NIX_BASE_IMAGE_NAME,
};

/// Host port the throwaway probe VM forwards SSH to. Distinct from every other guest this
/// chain boots — QEMU keys its monitor socket on the port alone.
const NIX_PROBE_PORT: u16 = 2255;

/// Name of the disposable VM booted off the base flavour, in the testkit's own library.
const NIX_PROBE_VM: &str = "nix-base-probe";

/// Boot budget for the probe VM: a lean cloud guest plus the cloud-init pass that applies
/// its own login seed.
const PROBE_BOOT_TIMEOUT: Duration = Duration::from_secs(300);

/// Progress to stderr, so a multi-hour first run is not a silent one.
fn progress(line: &str) {
    eprintln!("[image-chain] {line}");
}

#[tokio::test]
#[ignore = "production test: bakes the whole image chain and runs the supervised stack in a \
            guest — hours on a cold cache, a boot plus an incremental ./release when warm; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn the_chain_bakes_a_nix_base_builds_tddy_in_the_builder_and_runs_it_on_the_test_host() {
    let _ = tddy_vm_testkit::require_env_path(BASE_IMAGE_ENV);
    let layout = TestkitLayout::for_this_repo();

    // Given the base flavour and the builder derived from it, baked from the supplied cloud
    // image. Both are no-ops once sealed, which is what makes a warm run a boot rather than
    // a re-bake.
    let builder = BuilderVm::for_this_repo();
    builder
        .ensure_images(&progress)
        .await
        .expect("the nix base and builder images must bake");

    // Then the base flavour carries a working Nix: a VM booted off it resolves `nix` into
    // the store the bake created and runs it. The profile is sourced explicitly because
    // `ssh host cmd` is a non-login shell — the reason `SOURCE_NIX_PROFILE` exists.
    let nix_path = {
        let guest = boot_probe_of_prepared_base(
            &layout,
            NIX_BASE_IMAGE_NAME,
            NIX_PROBE_VM,
            NIX_PROBE_PORT,
            PROBE_BOOT_TIMEOUT,
        )
        .await
        .expect("a VM created from the sealed nix base must boot and answer over SSH");
        let path = guest
            .run_over_ssh(&format!("{SOURCE_NIX_PROFILE} && command -v nix"))
            .await
            .expect("the base flavour's guest must answer over SSH");
        let version = guest
            .run_over_ssh(&format!("{SOURCE_NIX_PROFILE} && nix --version"))
            .await
            .expect("the base flavour's guest must answer over SSH");
        assert!(
            version.stdout().starts_with("nix (Nix)"),
            "`nix --version` must come from Nix itself, got {:?}",
            version.stdout()
        );
        guest.shutdown().await.expect("the probe VM must shut down");
        layout
            .library()
            .remove_vm(NIX_PROBE_VM)
            .expect("the disposable probe VM must be removable");
        path.stdout().trim().to_string()
    };
    assert!(
        nix_path.starts_with("/nix/"),
        "`nix` must resolve into the Nix store the bake created, got {nix_path:?}"
    );

    // When the builder flavour compiles the workspace for deployment. This is the flavour's
    // job: the binaries come back as Linux ELF from a guest, not from the host toolchain.
    let built = builder
        .build_release(&progress)
        .await
        .expect("the builder guest must produce Linux binaries");

    // Then it produced the two halves of the supervised stack, among the rest
    for required in ["tddy-supervisor", "tddy-daemon"] {
        assert!(
            built
                .binaries
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == required)),
            "the builder must have produced {required}, got {:?}",
            built.binaries
        );
    }

    // And when the test-host flavour installs them with the real ./install --systemd
    let host = TestHostVm::start(&layout, &built, &progress)
        .await
        .expect("the test host must install the supervised stack");

    // Then the stack is running in the guest, as the two different users the privilege
    // split claims — read from the kernel's process table, not from what systemd was asked
    // to do. `./install --systemd` writes one unit, the root supervisor, which starts the
    // daemon as an unprivileged child; there is no tddy-daemon.service to ask about.
    let supervisor_state = host
        .guest()
        .run_over_ssh("systemctl is-active tddy-supervisor")
        .await
        .expect("the guest must answer over SSH");
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

    supervisor_state.assert_stdout_line("active");
    supervisor_user.assert_stdout_line("root");
    daemon_user.assert_stdout_line(TDDY_SERVICE_USERNAME);

    host.shutdown().await.expect("the test host must shut down");
}
