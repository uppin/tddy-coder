//! Unit tests for `QemuVm` building blocks.
//!
//! These tests exercise the concrete helper functions and `QemuVm` methods
//! using local TCP/Unix-socket infrastructure — no real QEMU process required.

use std::time::Duration;
use tddy_vm::{PortForward, QemuVm, RunningVm, Vm};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

// ────────────────────────────────────────────────────────────────────────────
// wait_for_ssh_port
// ────────────────────────────────────────────────────────────────────────────

/// `wait_for_ssh_port` must return `Ok` once a TCP listener appears on the target port.
#[tokio::test]
async fn ssh_poll_returns_ok_when_port_accepts_connections() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let port = listener.local_addr().unwrap().port();

    // Accept one connection in the background so the poll can complete.
    tokio::spawn(async move {
        let _ = listener.accept().await;
    });

    tddy_demo_runner::wait_for_ssh_port("127.0.0.1", port, Duration::from_secs(2))
        .await
        .expect("should return Ok once the port is listening");
}

/// `wait_for_ssh_port` must return `Err` when no listener ever appears within the timeout.
#[tokio::test]
async fn ssh_poll_times_out_when_no_listener_is_present() {
    // Port 19867 is unlikely to be in use; if it is the test is a false-positive.
    let result =
        tddy_demo_runner::wait_for_ssh_port("127.0.0.1", 19867, Duration::from_millis(300)).await;
    assert!(
        result.is_err(),
        "should return Err when no listener is present within the timeout"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// send_monitor_command
// ────────────────────────────────────────────────────────────────────────────

/// `send_monitor_command` must write `"system_powerdown\n"` to the QEMU monitor socket.
#[tokio::test]
async fn monitor_socket_receives_powerdown_command() {
    use tokio::net::UnixListener;

    let dir = tempfile::tempdir().expect("tempdir");
    let socket_path = dir.path().join("monitor.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind monitor socket");

    let path_str = socket_path.to_str().unwrap().to_string();
    tokio::spawn(async move {
        tddy_demo_runner::send_monitor_command(&path_str, "system_powerdown")
            .await
            .expect("send_monitor_command should succeed");
    });

    let (mut stream, _) = listener.accept().await.expect("accept");
    let mut buf = vec![0u8; 128];
    let n = stream.read(&mut buf).await.expect("read");
    let received = String::from_utf8_lossy(&buf[..n]);
    assert!(
        received.contains("system_powerdown"),
        "monitor socket must receive system_powerdown; got: {received:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// QemuVm::forward
// ────────────────────────────────────────────────────────────────────────────

/// `QemuVm::forward` must validate connectivity on the host port and return a
/// `ForwardHandle` whose `share_url` is `"http://localhost:<host_port>"`.
///
/// QEMU slirp sets up the hostfwd at boot-time; `forward()` only needs to confirm the
/// host-side port is reachable (simulated here by a plain TCP listener) and build the URL.
#[tokio::test]
async fn qemu_forward_validates_host_port_and_builds_share_url() {
    // Simulate the guest service already being reachable on the host via slirp hostfwd.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let host_port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = listener.accept().await;
    });

    let vm = QemuVm;
    let running_vm = RunningVm {
        ssh_host_port: 2222,
        monitor_socket: "/tmp/test-monitor.sock".to_string(),
        pid: 99999,
    };
    let port_forward = PortForward {
        host_port,
        guest_port: 80,
    };

    let handle = vm
        .forward(&running_vm, &port_forward)
        .await
        .expect("forward should succeed when host port is reachable");

    assert_eq!(handle.host_port, host_port);
    assert_eq!(handle.guest_port, 80);
    assert_eq!(
        handle.share_url,
        format!("http://localhost:{host_port}"),
        "share_url must be http://localhost:<host_port>"
    );
}

/// `QemuVm::forward` must return `Err` when the host port is not reachable.
///
/// If slirp didn't set up the forward (or the port is wrong), the caller must
/// surface a `ForwardFailed` rather than returning a URL that doesn't work.
#[tokio::test]
async fn qemu_forward_returns_err_when_host_port_not_reachable() {
    let vm = QemuVm;
    let running_vm = RunningVm {
        ssh_host_port: 2222,
        monitor_socket: "/tmp/test-monitor.sock".to_string(),
        pid: 99999,
    };
    // Port 19868 is unlikely to have a listener.
    let port_forward = PortForward {
        host_port: 19868,
        guest_port: 80,
    };

    let result = vm.forward(&running_vm, &port_forward).await;
    assert!(
        result.is_err(),
        "forward should return Err when the host port is not reachable"
    );
}
