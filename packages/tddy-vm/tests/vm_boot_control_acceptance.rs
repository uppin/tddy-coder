//! Real-boot acceptance tests for the QEMU launcher and serial-console control.
//!
//! These prove the prerequisites the daemon-spawned tddy host VM feature stands on: that
//! `tddy-vm` can boot a guest on this host's architecture with hardware acceleration and
//! UEFI firmware, drive it over the serial console (before SSH or guest networking exist),
//! share a host directory into it over virtio-9p, log in over SSH with the per-VM key, and
//! shut it down gracefully.
//!
//! ## Production tests — manual trigger only
//!
//! Each test boots a real `qemu-system-*` process. They are `#[ignore]`d (excluded from
//! `./test`, `./verify`, and plain `cargo test`) *and* gated on `TDDY_CLOUDINIT_BASE_IMAGE`
//! pointing at a real cloud-init-compatible qcow2 image whose architecture matches this
//! host. There is no bundled or auto-downloaded image.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-<arch>.qcow2 \
//!   cargo test -p tddy-vm --test vm_boot_control_acceptance -- --ignored --nocapture
//! ```

mod common;

use std::time::Duration;

use common::{a_console_loginable_user_data, a_test_guest, require_base_image, GUEST_USERNAME};
use serial_test::serial;
use tddy_vm::serial_shell::SerialShellState;
use tddy_vm::tddy_host::ninep_capable_kernel_command;
use tempfile::tempdir;

/// Boot budget for a cloud image on an accelerated host. Measured at ~17 s cold and ~7 s
/// warm for Debian 12 arm64 under HVF; 180 s leaves room for a cold host page cache and a
/// first-boot cloud-init run without hiding a real regression.
const BOOT_TIMEOUT: Duration = Duration::from_secs(180);

/// Budget for a single command executed over the serial console once the guest is at a
/// prompt. Generous relative to the work, because console I/O is line-rate over a virtual
/// UART rather than CPU-bound.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a guest may take to service the ACPI powerdown request and release its
/// forwarded ports. Debian reaches `poweroff.target` in a few seconds; 90 s absorbs a
/// service that is slow to stop without letting a VM that never shuts down pass.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(90);

/// Budget for installing a kernel from the Debian archive and rebooting into it. Dominated
/// by the ~100 MB download and `update-initramfs`; measured at just under 3 minutes on this
/// host, so 15 minutes absorbs a slow mirror without hanging the suite indefinitely.
const KERNEL_PREP_TIMEOUT: Duration = Duration::from_secs(900);

/// Wrap `script` in single quotes for `sh -c`, escaping any single quotes it contains.
fn shell_quote(script: &str) -> String {
    format!("'{}'", script.replace('\'', r"'\''"))
}

/// Poll until nothing is listening on `port`, returning whether that happened within
/// `timeout`.
async fn wait_for_port_to_close(port: u16, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM; requires TDDY_CLOUDINIT_BASE_IMAGE \
            (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn boots_an_aarch64_guest_and_reaches_a_login_prompt_over_the_serial_console() {
    let base_image = require_base_image();

    // Given a guest built from the developer-supplied cloud image
    let dir = tempdir().unwrap();
    let mut guest = a_test_guest(&base_image, dir.path(), "boot-probe")
        .with_ssh_host_port(2231)
        .boot()
        .await;

    // When the host watches the serial console
    guest
        .console
        .wait_for_login_prompt(BOOT_TIMEOUT)
        .await
        .expect("guest must reach a login prompt on the serial console");

    // Then the console state machine has advanced out of the boot prelude
    assert_eq!(guest.console.state(), SerialShellState::AtLogin);

    guest.shutdown().await.expect("guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM; requires TDDY_CLOUDINIT_BASE_IMAGE \
            (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn logs_in_over_the_serial_console_and_runs_a_command_returning_its_exit_code() {
    let base_image = require_base_image();

    // Given a booted guest the host has logged into over the serial console only
    let dir = tempdir().unwrap();
    let mut guest = a_test_guest(&base_image, dir.path(), "console-control")
        .with_ssh_host_port(2232)
        .boot()
        .await;
    guest
        .login_on_console(GUEST_USERNAME)
        .await
        .expect("the serial console must accept the acceptance-test credentials");

    // When a command is executed over that console
    let result = guest
        .console
        .run_command("id -un", COMMAND_TIMEOUT)
        .await
        .expect("command must execute over the serial console");

    // Then its output and exit code come back
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout_lines, vec![GUEST_USERNAME.to_string()]);

    guest.shutdown().await.expect("guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM; requires TDDY_CLOUDINIT_BASE_IMAGE \
            (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn reports_the_failing_exit_code_of_a_command_run_over_the_serial_console() {
    let base_image = require_base_image();

    // Given a booted, logged-in guest
    let dir = tempdir().unwrap();
    let mut guest = a_test_guest(&base_image, dir.path(), "console-exit-code")
        .with_ssh_host_port(2233)
        .boot()
        .await;
    guest
        .login_on_console(GUEST_USERNAME)
        .await
        .expect("the serial console must accept the acceptance-test credentials");

    // When a command that fails is executed — in a subshell, since a bare `exit` would end
    // the login shell itself before it could report anything
    let result = guest
        .console
        .run_command("(exit 42)", COMMAND_TIMEOUT)
        .await
        .expect("command must execute over the serial console");

    // Then the real exit code is reported, not a success placeholder
    assert_eq!(result.exit_code, 42);

    guest.shutdown().await.expect("guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM; requires TDDY_CLOUDINIT_BASE_IMAGE \
            (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn mounts_the_read_only_nine_p_share_and_reads_a_host_file_in_the_guest() {
    let base_image = require_base_image();

    // Given a host directory shared into the guest over virtio-9p
    let dir = tempdir().unwrap();
    let share = dir.path().join("share");
    std::fs::create_dir_all(&share).unwrap();
    std::fs::write(share.join("MARKER.txt"), "hello-from-host-9p\n").unwrap();

    let mut guest = a_test_guest(&base_image, dir.path(), "ninep-share")
        .with_ssh_host_port(2234)
        .with_read_only_nine_p_share(&share, "tddy-src")
        .boot()
        .await;
    guest
        .login_on_console(GUEST_USERNAME)
        .await
        .expect("the serial console must accept the acceptance-test credentials");

    // …and given the guest has been put on a 9p-capable kernel, exactly as the tddy-host
    // recipe does it. A stock Debian genericcloud image runs the cloud kernel flavour, which
    // ships no 9p modules at all; this installs the generic flavour, removes the cloud one so
    // GRUB stops preferring it, and reboots.
    let prepared = guest
        .console
        .run_command(
            &format!(
                "sudo sh -c {}",
                shell_quote(&ninep_capable_kernel_command())
            ),
            KERNEL_PREP_TIMEOUT,
        )
        .await
        .expect("kernel preparation must execute");
    assert_eq!(
        prepared.exit_code, 0,
        "kernel preparation failed: {:?}",
        prepared.stdout_lines
    );
    guest
        .console
        .wait_for_login_prompt(BOOT_TIMEOUT)
        .await
        .expect("guest must come back up on the 9p-capable kernel");
    guest
        .login_on_console(GUEST_USERNAME)
        .await
        .expect("the serial console must accept the acceptance-test credentials");

    // When the guest mounts the share and reads the host file
    let mount = guest
        .console
        .run_command(
            "sudo mkdir -p /mnt/tddy-src && \
             sudo mount -t 9p -o trans=virtio,version=9p2000.L,ro tddy-src /mnt/tddy-src",
            COMMAND_TIMEOUT,
        )
        .await
        .expect("mount command must execute");
    let read = guest
        .console
        .run_command("cat /mnt/tddy-src/MARKER.txt", COMMAND_TIMEOUT)
        .await
        .expect("read command must execute");

    // Then the mount succeeded and the host's bytes are visible in the guest
    assert_eq!(mount.exit_code, 0, "mount failed: {:?}", mount.stdout_lines);
    assert_eq!(read.exit_code, 0, "read failed: {:?}", read.stdout_lines);
    assert_eq!(read.stdout_lines, vec!["hello-from-host-9p".to_string()]);

    guest.shutdown().await.expect("guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM; requires TDDY_CLOUDINIT_BASE_IMAGE \
            (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn accepts_ssh_as_the_policy_user_with_the_generated_per_vm_key() {
    let base_image = require_base_image();

    // Given a guest whose cloud-init seed authorized a freshly generated per-VM keypair
    let dir = tempdir().unwrap();
    let keys = tddy_vm::library::generate_vm_ssh_keypair(dir.path(), "ssh-login")
        .expect("per-VM keypair must be generatable");

    let mut user_data = a_console_loginable_user_data("ssh-login");
    user_data.users[0].ssh_authorized_keys =
        vec![std::fs::read_to_string(&keys.public_key_path).unwrap()];

    let guest = a_test_guest(&base_image, dir.path(), "ssh-login")
        .with_ssh_host_port(2235)
        .with_user_data(user_data)
        .with_ssh_key_login(&keys.private_key_path, &keys.public_key_path)
        .boot()
        .await;
    // Slirp accepts the forwarded connection from the moment QEMU starts, so waiting for
    // sshd to answer is a separate step from asking it anything.
    guest
        .wait_for_ssh_ready(BOOT_TIMEOUT)
        .await
        .expect("sshd must start answering in the guest");

    // When a command is run over SSH as the policy user with that private key
    let verified = guest
        .run_over_ssh("id -un")
        .await
        .expect("ssh must authenticate and run the command");

    // Then SSH authenticated as the policy user, not as root with an ambient key
    verified.assert_stdout_line(GUEST_USERNAME);

    guest.shutdown().await.expect("guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM; requires TDDY_CLOUDINIT_BASE_IMAGE \
            (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn shuts_a_running_vm_down_gracefully_via_the_qemu_monitor() {
    let base_image = require_base_image();

    // Given a booted guest sitting at its login prompt
    let dir = tempdir().unwrap();
    let mut guest = a_test_guest(&base_image, dir.path(), "graceful-shutdown")
        .with_ssh_host_port(2236)
        .boot()
        .await;
    guest
        .wait_for_login_prompt(BOOT_TIMEOUT)
        .await
        .expect("guest must reach a login prompt");
    let ssh_host_port = guest.ssh_host_port();

    // When it is shut down through the QEMU monitor
    guest.shutdown().await.expect("guest must shut down");

    // Then the forwarded SSH port stops accepting connections. `system_powerdown` is an
    // ACPI request the guest services asynchronously, so this polls for the port to go away
    // rather than asserting it has already gone.
    let closed = wait_for_port_to_close(ssh_host_port, SHUTDOWN_TIMEOUT).await;
    assert!(
        closed,
        "port {ssh_host_port} must stop listening within {SHUTDOWN_TIMEOUT:?} of a graceful \
         powerdown"
    );
}
