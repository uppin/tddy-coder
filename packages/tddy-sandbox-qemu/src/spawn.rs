//! QEMU-VM sandbox backend: boots a qcow2 image and confines a command inside the
//! guest, mirroring the `spawn_plan` contract implemented by `tddy-sandbox-darwin` and
//! `tddy-sandbox-cgroups`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use tddy_sandbox::{SandboxError, SandboxHandle, SandboxPlan};

use crate::argv::{self, overlay_create_argv, plan_config_dir, qemu_sandbox_argv};

/// Backend-specific options not carried by `SandboxPlan` (the plan model is shared with
/// the non-VM backends and has no notion of "which VM image to boot").
#[derive(Debug, Clone)]
pub struct QemuBackendOptions {
    /// Path to the immutable base qcow2 image to boot.
    pub image_path: PathBuf,
    /// Path to the ephemeral copy-on-write overlay created for this run (see
    /// [`crate::argv::overlay_create_argv`]). `image_path` itself is never written to.
    pub overlay_path: PathBuf,
    /// Directory containing the cross-compiled `tddy-sandbox-runner` binary, 9p-mounted
    /// into the guest under the reserved runner tag.
    pub runner_dir_path: PathBuf,
    /// Host TCP port the guest's `tddy-sandbox-runner` gRPC server is forwarded to.
    pub control_port: u16,
}

/// Environment variable read by [`spawn_plan`] to resolve `QemuBackendOptions.image_path`
/// when the caller has no explicit option to pass (daemon dispatch parity with the
/// `#[cfg(target_os = ...)]`-selected darwin/cgroups backends).
pub const IMAGE_PATH_ENV: &str = "TDDY_SANDBOX_QEMU_IMAGE";

/// Environment variable read by [`spawn_plan`] to resolve
/// `QemuBackendOptions.runner_dir_path`.
pub const RUNNER_DIR_ENV: &str = "TDDY_SANDBOX_QEMU_RUNNER_DIR";

/// Default host port for the guest's forwarded `tddy-sandbox-runner` gRPC control channel.
pub const DEFAULT_CONTROL_PORT: u16 = 6700;

/// Boot `opts.image_path`, apply `plan`'s mounts/env/cwd/command inside the guest, and
/// return a handle to the running confinement.
///
/// This creates the ephemeral overlay, writes the guest plan config, and spawns
/// `qemu-system-x86_64` with the full sandbox argv — all real, observable steps. What is
/// **not yet implemented** is the other side of the gap documented in
/// `docs/dev/1-WIP/qemu-sandbox-cli.md` ("Open design points"): connecting to the
/// in-guest `tddy-sandbox-runner` over the forwarded TCP control port and streaming the
/// guest command's output back. `SandboxHandle.grpc_socket_path`/`ready_marker_path` are
/// placeholder paths — nothing populates them yet.
pub fn spawn_plan_with(
    plan: SandboxPlan,
    opts: QemuBackendOptions,
) -> Result<SandboxHandle, SandboxError> {
    plan.spec.validate()?;

    if !opts.image_path.is_file() {
        return Err(SandboxError::InvalidSpec(format!(
            "image not found at {}",
            opts.image_path.display()
        )));
    }

    // 1. Create the ephemeral copy-on-write overlay so the base image is never written to.
    if let Some(parent) = opts.overlay_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SandboxError::Io(e.to_string()))?;
    }
    let overlay_output = Command::new("qemu-img")
        .args(overlay_create_argv(&opts.image_path, &opts.overlay_path))
        .output()
        .map_err(|e| SandboxError::Io(format!("qemu-img create spawn failed: {e}")))?;
    if !overlay_output.status.success() {
        return Err(SandboxError::Io(format!(
            "qemu-img create failed: {}",
            String::from_utf8_lossy(&overlay_output.stderr).trim()
        )));
    }

    // 2. Write the guest plan config (mounts/env/cwd/command) to the host directory that
    //    will be 9p-mounted into the guest under `argv::PLAN_MOUNT_TAG`.
    let plan_dir = plan_config_dir(&opts);
    std::fs::create_dir_all(&plan_dir).map_err(|e| SandboxError::Io(e.to_string()))?;
    std::fs::write(plan_dir.join("plan.json"), argv::guest_plan_json(&plan))
        .map_err(|e| SandboxError::Io(e.to_string()))?;

    // 3. Boot qemu-system-x86_64 against the overlay with the full sandbox argv.
    let args = qemu_sandbox_argv(&plan, &opts);
    log::info!(
        target: "tddy_sandbox_qemu::spawn",
        "spawning qemu-system-x86_64 overlay={} control_port={} command={:?}",
        opts.overlay_path.display(),
        opts.control_port,
        plan.spec.command,
    );
    let child = Command::new("qemu-system-x86_64")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| SandboxError::Io(format!("qemu-system-x86_64 spawn failed: {e}")))?;

    Ok(SandboxHandle::new(
        child,
        plan.spec.profile_path.clone(),
        plan_dir.join("runner.grpc.sock"),
        plan_dir.join("ready"),
    ))
}

/// [`spawn_plan_with`] with the image path resolved from [`IMAGE_PATH_ENV`], matching the
/// `spawn_plan(SandboxPlan) -> Result<SandboxHandle, SandboxError>` signature the daemon
/// dispatches to for the darwin/cgroups backends.
pub fn spawn_plan(plan: SandboxPlan) -> Result<SandboxHandle, SandboxError> {
    let image_path = std::env::var(IMAGE_PATH_ENV)
        .map(PathBuf::from)
        .map_err(|_| {
            SandboxError::InvalidSpec(format!("{IMAGE_PATH_ENV} environment variable not set"))
        })?;
    let runner_dir_path = std::env::var(RUNNER_DIR_ENV)
        .map(PathBuf::from)
        .map_err(|_| {
            SandboxError::InvalidSpec(format!("{RUNNER_DIR_ENV} environment variable not set"))
        })?;
    let overlay_path = image_path.with_extension("overlay.qcow2");
    spawn_plan_with(
        plan,
        QemuBackendOptions {
            image_path,
            overlay_path,
            runner_dir_path,
            control_port: DEFAULT_CONTROL_PORT,
        },
    )
}
