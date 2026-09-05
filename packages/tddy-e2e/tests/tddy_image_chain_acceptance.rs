//! Cloud-init all the way to a running daemon: the layers a real deployment stands on, and
//! the supervised stack answering on top of them.
//!
//! ```text
//! debian cloud image
//!   └── tddy-nix-base      cloud-init + Nix, the accounts, the packages
//!         └── tddy-test-host  the deployment target: stock -cloud kernel, tddy installed
//! ```
//!
//! **No builder guest.** The builder exists because a macOS host cannot emit Linux ELF —
//! not a constraint where this runs, which is Linux x86_64 already. The binaries come from
//! a dist directory built by whoever ran `./release`, named by `TDDY_PREBUILT_DIST_DIR`;
//! rebuilding them inside a VM would cost hours to produce the same bytes.
//!
//! What is *not* shortcut is the install. `./install --systemd --headless` is the real
//! script writing the real unit, and the assertions read the result from the kernel and
//! from the daemon's own port rather than from what systemd was asked to do:
//!
//! - the supervisor unit is `active`,
//! - the supervisor runs as `root` and the daemon as the unprivileged service account, so
//!   the privilege split the feature claims actually happened,
//! - and the daemon **serves**: it answers HTTP on its configured `web_port`. A process in
//!   the table proves it was started; only a response proves it is operational.
//!
//! The copy is scp rather than a 9p share, for the reason `TestHostVm::deploy` gives: this
//! guest runs Debian's stock `-cloud` kernel, which ships no 9p modules, and giving it the
//! generic kernel would diverge the kernel under test from the one a real host runs.
//!
//! ## Production test — manual trigger only
//!
//! `#[ignore]`d and gated on `TDDY_CLOUDINIT_BASE_IMAGE` and `TDDY_PREBUILT_DIST_DIR`;
//! either one missing fails the test rather than skipping it.
//!
//! ```sh
//! TDDY_PREBUILT_DIST_DIR=/path/to/dist \
//!   ./dev cargo test -p tddy-e2e --test tddy_image_chain_acceptance -- --ignored --nocapture
//! ```

use std::time::Duration;

use serial_test::serial;
use tddy_vm_testkit::env_file::BASE_IMAGE_ENV;
use tddy_vm_testkit::recipes::TDDY_SERVICE_USERNAME;
use tddy_vm_testkit::{require_env_path, BuiltBinaries, TestHostVm, TestkitLayout};

/// Names the directory of Linux binaries to deploy — flat, as `./release` plus the install
/// bundle leave it. Its absence is a failure: a test that cannot deploy has not passed.
const PREBUILT_DIST_DIR_ENV: &str = "TDDY_PREBUILT_DIST_DIR";

/// The port `daemon.yaml.production` serves on (`listen.web_port`).
const DAEMON_WEB_PORT: u16 = 8899;

/// How long the daemon gets to answer after the install.
///
/// It starts asynchronously under the supervisor, which systemd starts in turn, so the
/// first probes routinely land before it is listening. Polling is right here — this is a
/// condition being waited into existence, not a command being retried.
const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(120);

/// Progress to stderr, so a long bake is not a silent one.
fn progress(line: &str) {
    eprintln!("[image-chain] {line}");
}

#[tokio::test]
#[ignore = "production test: bakes the cloud-init layers, installs prebuilt binaries with \
            the real ./install --systemd and asserts the daemon serves; requires \
            TDDY_CLOUDINIT_BASE_IMAGE and TDDY_PREBUILT_DIST_DIR (see module docs); run \
            with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn the_installed_stack_runs_under_systemd_and_the_daemon_serves() {
    let _ = require_env_path(BASE_IMAGE_ENV);
    let dist_dir = require_env_path(PREBUILT_DIST_DIR_ENV);

    // Given the binaries a deployment ships, checked before a guest is booted rather than
    // discovered missing by an install inside one
    let built = BuiltBinaries::from_dist_dir(&dist_dir)
        .expect("the prebuilt dist directory must hold every binary a deployment needs");

    // When the cloud-init layers are baked and the real installer runs in the guest.
    // `TestHostVm::start` bakes tddy-nix-base if it is not already sealed, then the
    // test-host layer on top of it, boots a disposable VM off that, copies the binaries in
    // and runs `./install --systemd --headless`.
    let layout = TestkitLayout::for_this_repo();
    let host = TestHostVm::start(&layout, &built, &progress)
        .await
        .expect("the test host must install the supervised stack");

    progress("asking the guest what systemd and the kernel say about the stack");
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

    // Then systemd is running the one unit `./install --systemd` writes — there is no
    // tddy-daemon.service to ask about, because the supervisor starts the daemon itself
    supervisor_state.assert_stdout_line("active");

    // And the privilege split is real, read from the process table rather than from the
    // unit file's intentions: a root supervisor, and a daemon that already dropped
    supervisor_user.assert_stdout_line("root");
    daemon_user.assert_stdout_line(TDDY_SERVICE_USERNAME);

    // And the daemon is *operational*, not merely present. A process in the table proves it
    // was started; only an answer on its own port proves it is serving.
    progress("waiting for the daemon to answer on its configured web port");
    let served = host
        .guest()
        .run_over_ssh_until_success(
            &format!(
                "curl -fsS -o /dev/null -w '%{{http_code}}' \
                 http://127.0.0.1:{DAEMON_WEB_PORT}/api/config"
            ),
            DAEMON_READY_TIMEOUT,
        )
        .await
        .expect("the guest must answer over SSH");

    served.assert_succeeded().assert_stdout_line("200");

    host.shutdown().await.expect("the test host must shut down");
}
