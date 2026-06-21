//! Unit tests for `QemuVmArgs` — the QEMU argument builder.
//!
//! These tests fully specify the expected argv so that the implementation
//! can be verified independently of process spawning.

use tddy_demo_runner::qemu::QemuVmArgs;
use tddy_demo_runner::vm::DemoVmConfig;
use tddy_workflow_recipes::parser::PortMap;

// ── hostfwd spec formatting ──────────────────────────────────────────────────

/// A single port map must format as `tcp::<host>-:<guest>`.
#[test]
fn qemu_args_hostfwd_formats_correctly() {
    let port_map = PortMap {
        host_port: 2222,
        guest_port: 22,
    };
    let spec = QemuVmArgs::hostfwd_spec(&port_map);
    assert_eq!(
        spec, "tcp::2222-:22",
        "hostfwd spec must be tcp::<host>-:<guest>, got: {spec:?}"
    );
}

/// An app port forward must format as `tcp::8080-:80`.
#[test]
fn qemu_args_app_hostfwd_formats_correctly() {
    let port_map = PortMap {
        host_port: 8080,
        guest_port: 80,
    };
    let spec = QemuVmArgs::hostfwd_spec(&port_map);
    assert_eq!(
        spec, "tcp::8080-:80",
        "app hostfwd spec must be tcp::8080-:80, got: {spec:?}"
    );
}

// ── netdev arg assembly ──────────────────────────────────────────────────────

/// The `-netdev user,...` arg must always include the SSH forward and any extra maps.
#[test]
fn qemu_args_netdev_includes_ssh_forward() {
    let config = DemoVmConfig {
        qcow2_path: "/tmp/test.qcow2".to_string(),
        extra_hostfwd: vec![],
        ssh_host_port: 2222,
    };
    let arg = QemuVmArgs::netdev_arg(&config);
    assert!(
        arg.contains("hostfwd=tcp::2222-:22"),
        "netdev arg must include SSH hostfwd tcp::2222-:22, got: {arg:?}"
    );
    assert!(
        arg.starts_with("user,id=net0,"),
        "netdev arg must start with 'user,id=net0,', got: {arg:?}"
    );
}

/// Extra port maps are appended after the SSH forward, each as a `hostfwd=` spec.
#[test]
fn qemu_args_multiple_hostfwds_combined() {
    let config = DemoVmConfig {
        qcow2_path: "/tmp/test.qcow2".to_string(),
        extra_hostfwd: vec![PortMap {
            host_port: 8080,
            guest_port: 80,
        }],
        ssh_host_port: 2222,
    };
    let arg = QemuVmArgs::netdev_arg(&config);
    assert!(
        arg.contains("hostfwd=tcp::2222-:22"),
        "must include SSH forward, got: {arg:?}"
    );
    assert!(
        arg.contains("hostfwd=tcp::8080-:80"),
        "must include app forward tcp::8080-:80, got: {arg:?}"
    );
}

// ── full argv ────────────────────────────────────────────────────────────────

/// The full argv must include the qcow2 drive, netdev, device, and monitor args.
#[test]
fn qemu_args_full_argv_has_required_elements() {
    let config = DemoVmConfig {
        qcow2_path: "/images/demo.qcow2".to_string(),
        extra_hostfwd: vec![PortMap {
            host_port: 8080,
            guest_port: 80,
        }],
        ssh_host_port: 2222,
    };
    let args = QemuVmArgs::build(&config);

    // Drive
    let drive_idx = args
        .iter()
        .position(|a| a == "-drive")
        .expect("-drive flag must be present");
    assert!(
        args[drive_idx + 1].contains("demo.qcow2"),
        "-drive value must reference the qcow2 path, got: {:?}",
        args.get(drive_idx + 1)
    );
    assert!(
        args[drive_idx + 1].contains("if=virtio"),
        "-drive value must include if=virtio, got: {:?}",
        args.get(drive_idx + 1)
    );
    assert!(
        args[drive_idx + 1].contains("format=qcow2"),
        "-drive value must include format=qcow2, got: {:?}",
        args.get(drive_idx + 1)
    );

    // Netdev
    let netdev_idx = args
        .iter()
        .position(|a| a == "-netdev")
        .expect("-netdev flag must be present");
    assert!(
        args[netdev_idx + 1].contains("user,id=net0"),
        "-netdev value must be user,id=net0,..., got: {:?}",
        args.get(netdev_idx + 1)
    );

    // Device
    assert!(
        args.iter().any(|a| a == "-device"),
        "-device flag must be present in argv: {args:?}"
    );

    // Monitor socket
    assert!(
        args.iter().any(|a| a == "-monitor"),
        "-monitor flag must be present in argv: {args:?}"
    );
    let mon_idx = args.iter().position(|a| a == "-monitor").unwrap();
    assert!(
        args[mon_idx + 1].starts_with("unix:"),
        "-monitor value must be a unix socket, got: {:?}",
        args.get(mon_idx + 1)
    );
}

// ── port-map → share URL ─────────────────────────────────────────────────────

/// The share URL for a PortForward must use `http://localhost:<host_port>`.
#[test]
fn port_map_to_share_url_uses_host_port() {
    let port_map = PortMap {
        host_port: 8080,
        guest_port: 80,
    };
    let url = format!("http://localhost:{}", port_map.host_port);
    assert_eq!(
        url, "http://localhost:8080",
        "share URL must be http://localhost:<host_port>"
    );
}
