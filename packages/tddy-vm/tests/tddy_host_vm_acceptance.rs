//! Real-boot acceptance tests for the daemon-spawned tddy host VM feature itself.
//!
//! Proves the whole path: bake a Debian cloud image into a prepared base that has tddy
//! built from the operator's working copy and installed as a systemd service, create a VM
//! from that base, boot it, and confirm the guest daemon is running and reachable.
//!
//! ## Production tests — manual trigger only
//!
//! `#[ignore]`d and env-gated, like [`crate::common`]'s other consumers.
//!
//! - [`bakes_a_prepared_base_whose_guest_runs_the_tddy_daemon_under_systemd`] performs the
//!   full bake. It needs `TDDY_CLOUDINIT_BASE_IMAGE` and **takes hours**: it installs Nix
//!   in the guest and runs `./release` (the whole workspace, including `libwebrtc`) on a
//!   2-vCPU VM.
//! - The remaining tests consume an already-baked prepared base named by
//!   `TDDY_TDDY_HOST_PREPARED_BASE`, so they run in about a minute.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-<arch>.qcow2 \
//!   cargo test -p tddy-vm --test tddy_host_vm_acceptance -- --ignored --nocapture
//! ```

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{
    a_tddy_host_manifest, configured_base_image, configured_prepared_base, BASE_IMAGE_ENV,
    PREPARED_BASE_ENV,
};
use serial_test::serial;
use tddy_vm::library::VmLibrary;
use tddy_vm::tddy_host::{
    build_tddy_host_image, LiveKitCommonRoom, TddyHostBuildOptions, TddyHostSpec,
};
use tempfile::tempdir;

/// The RPC's own ceiling, imported rather than restated so the test cannot drift from the
/// budget `BuildTddyHostImage` actually enforces.
const BAKE_TIMEOUT: Duration = tddy_vm::service::TDDY_HOST_BAKE_TIMEOUT;

/// Boot budget for a VM created from an already-baked prepared base.
///
/// A bare cloud image reaches a login prompt in ~17 s on this host
/// (`vm_boot_control_acceptance.rs`); this budget covers that plus a `./install`-provisioned
/// guest's extra systemd units and the daemon's own start. 300 s is an order of magnitude
/// over the observed figure, deliberately, because a guest that is merely slow to start a
/// service should not fail the suite — but a guest that never starts one still does.
const BOOT_TIMEOUT: Duration = Duration::from_secs(300);

fn a_livekit_common_room() -> LiveKitCommonRoom {
    LiveKitCommonRoom {
        url: "wss://livekit.acceptance.invalid".to_string(),
        api_key: "acceptance-key".to_string(),
        api_secret: "acceptance-secret".to_string(),
        common_room: "tddy-acceptance".to_string(),
    }
}

#[tokio::test]
#[ignore = "production test: bakes a real image by installing Nix and running ./release in \
            a guest — takes hours; requires TDDY_CLOUDINIT_BASE_IMAGE; run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn bakes_a_prepared_base_whose_guest_runs_the_tddy_daemon_under_systemd() {
    let Some(base_image) = configured_base_image() else {
        eprintln!("{BASE_IMAGE_ENV} not set — skipping production test (see module docs)");
        return;
    };

    // Given a library and this repository's own working copy as the build source
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().expect("library must initialise");

    let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root must resolve from CARGO_MANIFEST_DIR")
        .to_path_buf();

    let progress_lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&progress_lines);

    // When the tddy host image is baked
    let prepared_base = build_tddy_host_image(
        &TddyHostBuildOptions {
            library: library.clone(),
            name: "debian-12-tddy".to_string(),
            base_image_name: "debian-12".to_string(),
            base_image_src: base_image,
            source_dir,
            spec: TddyHostSpec {
                hostname: "tddy-host".to_string(),
                username: common::GUEST_USERNAME.to_string(),
                livekit: Some(a_livekit_common_room()),
            },
            disk_size: "40G".to_string(),
            memory: "4096M".to_string(),
            cpus: 2,
            ssh_host_port: 2241,
            timeout: BAKE_TIMEOUT,
        },
        &move |line: &str| sink.lock().unwrap().push(line.to_string()),
    )
    .await
    .expect("tddy host image must bake");

    // Then the prepared base pair landed in the library and progress was streamed
    assert_eq!(
        prepared_base,
        library.prepared_base_dir().join("debian-12-tddy.qcow2")
    );
    assert!(
        library
            .prepared_base_dir()
            .join("debian-12-tddy-base.qcow2")
            .exists(),
        "the flattened base half of the chained pair must be co-located with the overlay"
    );
    assert!(
        !progress_lines.lock().unwrap().is_empty(),
        "the bake must stream serial-console progress to its caller"
    );
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM from a baked prepared base; requires \
            TDDY_TDDY_HOST_PREPARED_BASE; run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn runs_the_tddy_daemon_under_systemd_in_a_vm_created_from_the_prepared_base() {
    let Some(prepared_base) = configured_prepared_base() else {
        eprintln!("{PREPARED_BASE_ENV} not set — skipping production test (see module docs)");
        return;
    };

    // Given a VM created from an already-baked tddy host prepared base
    let dir = tempdir().unwrap();
    let library = a_library_seeded_with(&prepared_base, dir.path(), "debian-12-tddy");
    let manifest = a_tddy_host_manifest("tddy-host-1", "debian-12-tddy", 2242);
    library
        .create_vm(&manifest)
        .await
        .expect("VM must be creatable from the prepared base");

    let guest = common::boot_library_vm(&library, &manifest, BOOT_TIMEOUT).await;

    // When the guest is asked whether the daemon service is up, over SSH as the manifest's
    // login-policy user
    let active = guest.run_over_ssh("systemctl is-active tddy-daemon").await;

    // Then systemd reports it active
    assert_eq!(active.exit_code, 0);
    assert_eq!(active.output.trim(), "active");

    guest.shutdown().await;
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM from a baked prepared base; requires \
            TDDY_TDDY_HOST_PREPARED_BASE; run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn serves_the_guest_daemon_connect_port_over_the_forwarded_host_port() {
    let Some(prepared_base) = configured_prepared_base() else {
        eprintln!("{PREPARED_BASE_ENV} not set — skipping production test (see module docs)");
        return;
    };

    // Given a booted VM created from the tddy host prepared base
    let dir = tempdir().unwrap();
    let library = a_library_seeded_with(&prepared_base, dir.path(), "debian-12-tddy");
    let manifest = a_tddy_host_manifest("tddy-host-2", "debian-12-tddy", 2243);
    library
        .create_vm(&manifest)
        .await
        .expect("VM must be creatable from the prepared base");

    let guest = common::boot_library_vm(&library, &manifest, BOOT_TIMEOUT).await;
    let forwarded_port = manifest.run.port_forwards[0].host_port;

    // When the host speaks HTTP to the forwarded Connect port.
    //
    // A TCP connect proves nothing here: QEMU's slirp opens the host-side listening socket
    // at startup and accepts whether or not anything in the guest is listening, so a
    // connect-only assertion passes with the daemon dead, absent, or the VM halted. Only a
    // response the guest had to produce distinguishes those.
    let response = http_status_line(forwarded_port, BOOT_TIMEOUT).await;

    // Then the guest daemon answered
    assert!(
        response.starts_with("HTTP/"),
        "guest daemon must serve HTTP on forwarded host port {forwarded_port}, got: {response:?}"
    );

    guest.shutdown().await;
}

/// Send a minimal HTTP request to `port` and return the first line of the response,
/// retrying until `timeout` because the guest daemon starts asynchronously under systemd.
///
/// Returns the reason as the "status line" when nothing ever answers, so a failure message
/// says what happened rather than just that an assertion was false.
async fn http_status_line(port: u16, timeout: Duration) -> String {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last = "no connection attempt completed".to_string();
    while tokio::time::Instant::now() < deadline {
        last = match read_http_status_line(port).await {
            Ok(line) => return line,
            Err(e) => e,
        };
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    last
}

/// One HTTP exchange against `port`: connect, send a `GET /`, read the status line.
async fn read_http_status_line(port: u16) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|e| format!("connect failed: {e}"))?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .map_err(|e| format!("request write failed: {e}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|e| format!("response read failed: {e}"))?;
    let text = String::from_utf8_lossy(&response);
    text.lines()
        .next()
        .map(|line| line.to_string())
        .ok_or_else(|| "the connection closed without sending anything".to_string())
}

/// Place an already-baked prepared-base pair into a fresh library rooted at `root`.
fn a_library_seeded_with(
    prepared_base: &std::path::Path,
    root: &std::path::Path,
    name: &str,
) -> VmLibrary {
    let library = VmLibrary::new(root);
    library.init().expect("library must initialise");

    let base_half = prepared_base.with_file_name(format!("{name}-base.qcow2"));
    std::fs::copy(
        &base_half,
        library
            .prepared_base_dir()
            .join(format!("{name}-base.qcow2")),
    )
    .expect("the prepared base's flattened half must be copyable alongside its overlay");
    std::fs::copy(
        prepared_base,
        library.prepared_base_dir().join(format!("{name}.qcow2")),
    )
    .expect("the prepared base overlay must be copyable");

    library
}
