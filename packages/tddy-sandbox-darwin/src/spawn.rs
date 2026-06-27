use std::fs::{File, OpenOptions};
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use tddy_sandbox::{
    append_line, egress_log_path, SandboxError, SandboxHandle, SandboxSpec,
    SANDBOX_EXEC_STDERR_LOG, SANDBOX_EXEC_STDOUT_LOG, SANDBOX_SPAWN_MANIFEST,
};

use crate::profile::render_profile;

fn open_egress_log(egress_dir: &std::path::Path, name: &str) -> Result<File, SandboxError> {
    std::fs::create_dir_all(egress_dir).map_err(|e| SandboxError::Io(e.to_string()))?;
    let path = egress_log_path(egress_dir, name);
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| SandboxError::Io(format!("open egress log {}: {e}", path.display())))
}

fn write_spawn_manifest(
    spec: &SandboxSpec,
    pid: u32,
    profile_path: &std::path::Path,
) -> Result<(), SandboxError> {
    let manifest_path = egress_log_path(&spec.egress_dir, SANDBOX_SPAWN_MANIFEST);
    let payload = serde_json::json!({
        "pid": pid,
        "profile_path": profile_path.to_string_lossy(),
        "project_root": spec.project_root.to_string_lossy(),
        "scratch_dir": spec.scratch_dir.to_string_lossy(),
        "egress_dir": spec.egress_dir.to_string_lossy(),
        "command": spec.command,
        "egress_via": "session_channel",
        "network_policy": "deny",
        "logs": {
            "stderr": SANDBOX_EXEC_STDERR_LOG,
            "stdout": SANDBOX_EXEC_STDOUT_LOG,
            "runner": tddy_sandbox::SANDBOX_RUNNER_LOG,
        },
    });
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|e| SandboxError::Io(format!("serialize spawn manifest: {e}")))?;
    std::fs::write(&manifest_path, text)
        .map_err(|e| SandboxError::Io(format!("write spawn manifest: {e}")))?;
    Ok(())
}

/// Spawn `spec.command` inside `sandbox-exec` with a rendered SBPL profile.
///
/// Child stdout/stderr are redirected to files under [`SandboxSpec::egress_dir`]; see
/// [`tddy_sandbox::format_egress_logs`] for host-side inspection.
pub fn spawn(spec: SandboxSpec) -> Result<SandboxHandle, SandboxError> {
    spec.validate()?;

    std::fs::create_dir_all(&spec.project_root).map_err(|e| SandboxError::Io(e.to_string()))?;
    std::fs::create_dir_all(&spec.scratch_dir).map_err(|e| SandboxError::Io(e.to_string()))?;
    std::fs::create_dir_all(&spec.egress_dir).map_err(|e| SandboxError::Io(e.to_string()))?;

    let profile_text = render_profile(&spec)?;
    std::fs::write(&spec.profile_path, &profile_text)
        .map_err(|e| SandboxError::Io(format!("write profile: {e}")))?;

    let grpc_socket_path = spec.project_root.join("sandbox.grpc.sock");
    let ready_marker_path = spec.project_root.join("sandbox.ready");
    let _ = std::fs::remove_file(&grpc_socket_path);
    let _ = std::fs::remove_file(&ready_marker_path);

    let stderr_log = open_egress_log(&spec.egress_dir, SANDBOX_EXEC_STDERR_LOG)?;
    let stdout_log = open_egress_log(&spec.egress_dir, SANDBOX_EXEC_STDOUT_LOG)?;

    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-f").arg(&spec.profile_path);

    cmd.arg("/usr/bin/env");
    cmd.arg("-i");
    for (k, v) in &spec.env {
        cmd.arg(format!("{k}={v}"));
    }
    cmd.args(&spec.command);

    cmd.current_dir(&spec.project_root);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(stdout_log));
    cmd.stderr(Stdio::from(stderr_log));

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    log::info!(
        target: "tddy_sandbox_darwin::spawn",
        "spawning sandbox-exec profile={} egress={} command={:?}",
        spec.profile_path.display(),
        spec.egress_dir.display(),
        spec.command,
    );

    let child = cmd
        .spawn()
        .map_err(|e| SandboxError::Io(format!("sandbox-exec spawn failed: {e}")))?;

    let pid = child.id();
    write_spawn_manifest(&spec, pid, &spec.profile_path)?;
    let _ = append_line(
        &egress_log_path(&spec.egress_dir, SANDBOX_EXEC_STDERR_LOG),
        &format!(
            "sandbox-exec spawned pid={pid} profile={}",
            spec.profile_path.display()
        ),
    );

    log::info!(
        target: "tddy_sandbox_darwin::spawn",
        "sandbox-exec child pid={pid} logs under {}",
        spec.egress_dir.display(),
    );

    Ok(SandboxHandle::new(
        child,
        spec.profile_path,
        grpc_socket_path,
        ready_marker_path,
    ))
}

/// Detect common toolchain paths for the read allow-list.
pub fn detect_allow_read_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Ok(dev) = std::process::Command::new("xcode-select")
        .arg("-p")
        .output()
    {
        if dev.status.success() {
            let p = String::from_utf8_lossy(&dev.stdout).trim().to_string();
            push_allow_path(&mut paths, p);
        }
    }
    if let Ok(node_out) = std::process::Command::new("which").arg("node").output() {
        if node_out.status.success() {
            let node = String::from_utf8_lossy(&node_out.stdout).trim().to_string();
            if let Some(parent) = std::path::Path::new(&node).parent() {
                push_allow_path(&mut paths, parent.to_string_lossy().to_string());
            }
        }
    }
    if let Ok(brew) = std::process::Command::new("brew").arg("--prefix").output() {
        if brew.status.success() {
            let p = String::from_utf8_lossy(&brew.stdout).trim().to_string();
            push_allow_path(&mut paths, p);
        }
    }
    if let Ok(sh_out) = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("command -v sh")
        .output()
    {
        if sh_out.status.success() {
            let sh = String::from_utf8_lossy(&sh_out.stdout).trim().to_string();
            push_allow_path(&mut paths, sh.clone());
            if let Some(parent) = std::path::Path::new(&sh).parent() {
                push_allow_path(&mut paths, parent.to_string_lossy().to_string());
            }
        }
    }
    paths
}

fn push_allow_path(paths: &mut Vec<std::path::PathBuf>, raw: String) {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return;
    }
    paths.push(std::path::PathBuf::from(trimmed));
}
