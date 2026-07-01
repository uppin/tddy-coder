//! Pure, unit-testable QEMU argv/config builders for the sandbox backend. Mirrors
//! `tddy_vm::qemu::QemuVmArgs` (argv assembly with no process spawning) but for the
//! `SandboxPlan` contract rather than `tddy_vm::VmConfig`.

use std::path::{Path, PathBuf};
use tddy_sandbox::SandboxPlan;

use crate::spawn::QemuBackendOptions;

/// 9p mount tag reserved for injecting the in-guest `tddy-sandbox-runner` binary.
pub const RUNNER_MOUNT_TAG: &str = "tddy-runner";
/// 9p mount tag reserved for the guest-init plan config (see [`guest_plan_json`]).
pub const PLAN_MOUNT_TAG: &str = "tddy-plan";

/// Build the `-fsdev`/`-device virtio-9p-pci` argument pair for one
/// [`tddy_sandbox::MountSpec`], tagged `tag`. `index` distinguishes concurrent fsdevs
/// (`fs0`, `fs1`, ...). Read-only unless `mount.writable`.
pub fn ninep_fsdev_args(index: usize, mount: &tddy_sandbox::MountSpec, tag: &str) -> Vec<String> {
    let mut fsdev = format!(
        "local,id=fs{index},path={},security_model=mapped-xattr",
        mount.host.display()
    );
    if !mount.writable {
        fsdev.push_str(",readonly=on");
    }
    vec![
        "-fsdev".to_string(),
        fsdev,
        "-device".to_string(),
        format!("virtio-9p-pci,fsdev=fs{index},mount_tag={tag}"),
    ]
}

/// Host-side directory that will be 9p-mounted into the guest under [`PLAN_MOUNT_TAG`].
/// The rendered [`guest_plan_json`] must be written here before boot so the guest init
/// hook can read it once the share is mounted.
///
/// TODO: there is no dedicated field on `QemuBackendOptions` for this; it is derived from
/// the overlay's parent directory until one is added.
pub fn plan_config_dir(opts: &QemuBackendOptions) -> PathBuf {
    opts.overlay_path
        .parent()
        .map(|p| p.join("plan"))
        .unwrap_or_else(|| PathBuf::from("/tmp/tddy-sandbox-qemu-plan"))
}

/// The QEMU monitor Unix socket path for graceful shutdown (`system_powerdown`), derived
/// from `control_port` so concurrent sandbox VMs don't collide, mirroring
/// [`tddy_vm::qemu::QemuVmArgs::monitor_socket_path`].
pub fn monitor_socket_path(control_port: u16) -> String {
    format!("/tmp/tddy-sandbox-qemu-monitor-{control_port}.sock")
}

/// Build the `qemu-img create -f qcow2 -b <base> -F qcow2 <overlay>` argv that creates an
/// ephemeral copy-on-write overlay so `base` is never written to.
pub fn overlay_create_argv(base: &Path, overlay: &Path) -> Vec<String> {
    vec![
        "create".to_string(),
        "-f".to_string(),
        "qcow2".to_string(),
        "-b".to_string(),
        base.display().to_string(),
        "-F".to_string(),
        "qcow2".to_string(),
        overlay.display().to_string(),
    ]
}

/// Serialize the parts of `plan`/`opts` the guest init hook needs (mounts, env, cwd,
/// command, and the control port to bind the runner's gRPC server on) to JSON, for
/// delivery over the [`PLAN_MOUNT_TAG`] 9p share.
pub fn guest_plan_json(plan: &SandboxPlan, opts: &QemuBackendOptions) -> String {
    let mounts: Vec<serde_json::Value> = plan
        .mounts
        .iter()
        .map(|m| {
            serde_json::json!({
                "host": m.host.display().to_string(),
                "jail": m.jail.as_ref().map(|p| p.display().to_string()),
                "writable": m.writable,
            })
        })
        .collect();

    serde_json::json!({
        "command": plan.spec.command,
        "cwd": plan.spec.cwd.as_ref().map(|p| p.display().to_string()),
        "env": plan.env.vars,
        "mounts": mounts,
        "control_port": opts.control_port,
    })
    .to_string()
}

/// Build the full `qemu-system-x86_64` argv for booting `plan` against `opts`: the
/// overlay drive, one 9p fsdev/device pair per `MountSpec` plus the reserved
/// [`RUNNER_MOUNT_TAG`]/[`PLAN_MOUNT_TAG`] shares, and `-netdev user` hostfwd for
/// `opts.control_port` and every `plan.network.loopback_allow_ports` entry.
pub fn qemu_sandbox_argv(plan: &SandboxPlan, opts: &QemuBackendOptions) -> Vec<String> {
    let mut args = vec![
        "-drive".to_string(),
        format!(
            "file={},if=virtio,format=qcow2",
            opts.overlay_path.display()
        ),
        "-m".to_string(),
        "2048M".to_string(),
        "-nographic".to_string(),
    ];

    for (index, mount) in plan.mounts.iter().enumerate() {
        let tag = format!("tddy-mount{index}");
        args.extend(ninep_fsdev_args(index, mount, &tag));
    }

    // Reserved shares for the in-guest runner binary and the guest plan config, placed
    // after the caller-declared mounts so their fsdev ids never collide.
    let runner_index = plan.mounts.len();
    let runner_mount = tddy_sandbox::MountSpec {
        host: opts.runner_dir_path.clone(),
        jail: None,
        writable: false,
    };
    args.extend(ninep_fsdev_args(
        runner_index,
        &runner_mount,
        RUNNER_MOUNT_TAG,
    ));

    let plan_index = runner_index + 1;
    let plan_mount = tddy_sandbox::MountSpec {
        host: plan_config_dir(opts),
        jail: None,
        writable: false,
    };
    args.extend(ninep_fsdev_args(plan_index, &plan_mount, PLAN_MOUNT_TAG));

    args.push("-netdev".to_string());
    args.push(netdev_arg(plan, opts));
    args.push("-device".to_string());
    args.push("virtio-net-pci,netdev=net0".to_string());

    args.push("-monitor".to_string());
    args.push(format!(
        "unix:{},server,nowait",
        monitor_socket_path(opts.control_port)
    ));
    args.push("-serial".to_string());
    args.push(format!(
        "file:/tmp/tddy-sandbox-qemu-serial-{}.log",
        opts.control_port
    ));

    args
}

/// Build the `-netdev user,id=net0,hostfwd=...` value forwarding `opts.control_port` and
/// every `plan.network.loopback_allow_ports` entry, mirroring
/// [`tddy_vm::qemu::QemuVmArgs::netdev_arg`].
fn netdev_arg(plan: &SandboxPlan, opts: &QemuBackendOptions) -> String {
    let mut arg = format!("user,id=net0,hostfwd=tcp::{0}-:{0}", opts.control_port);
    for port in &plan.network.loopback_allow_ports {
        arg.push_str(&format!(",hostfwd=tcp::{port}-:{port}"));
    }
    arg
}
