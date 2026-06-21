use crate::vm::VmError;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_qemu::QemuPlugin;
use tddy_rpc::Status;
use tddy_service::proto::vm::BuildVmImageProgress;
use tokio::fs;

// ── Image listing ──────────────────────────────────────────────────────────────

/// Metadata about a built qcow2 image found in the Buildroot output tree.
#[derive(Debug, Clone)]
pub struct VmImageRecord {
    /// Absolute path to the `.qcow2` file.
    pub path: String,
    /// The build directory name (e.g. `build-1748123456789`).
    pub name: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// File modification time in milliseconds since UNIX epoch.
    pub modified_unix_ms: u64,
}

/// Returns the canonical `tmp/buildroot/disks` directory (relative to the daemon's cwd).
pub fn built_images_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("tmp/buildroot/disks")
}

/// Scan `disks_dir` for `build-*/images/*.qcow2` files and return them sorted newest-first.
///
/// Silently skips any build dirs that have no `images/` subdirectory or no `.qcow2` files.
/// Returns an empty vec if `disks_dir` does not exist.
pub async fn list_built_images_in(disks_dir: &Path) -> Vec<VmImageRecord> {
    let mut records = Vec::new();

    let mut top = match fs::read_dir(disks_dir).await {
        Ok(d) => d,
        Err(_) => return records, // dir doesn't exist or not readable — not an error
    };

    while let Ok(Some(entry)) = top.next_entry().await {
        let build_dir = entry.path();
        if !build_dir.is_dir() {
            continue;
        }
        let build_name = match build_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let images_dir = build_dir.join("images");
        let mut img_dir = match fs::read_dir(&images_dir).await {
            Ok(d) => d,
            Err(_) => continue, // no images/ subdir — skip
        };

        while let Ok(Some(img_entry)) = img_dir.next_entry().await {
            let img_path = img_entry.path();
            if img_path.extension().and_then(|e| e.to_str()) != Some("qcow2") {
                continue;
            }
            let meta = match fs::metadata(&img_path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size_bytes = meta.len();
            let modified_unix_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            records.push(VmImageRecord {
                path: img_path.to_string_lossy().into_owned(),
                name: build_name.clone(),
                size_bytes,
                modified_unix_ms,
            });
        }
    }

    // Newest first
    records.sort_by(|a, b| b.modified_unix_ms.cmp(&a.modified_unix_ms));
    records
}

/// Scan the canonical built-images directory for `.qcow2` files, sorted newest-first.
pub async fn list_built_images() -> Vec<VmImageRecord> {
    list_built_images_in(&built_images_dir()).await
}

// ── Stage constants ────────────────────────────────────────────────────────────

// Stage constants matching BuildVmImageProgress::Stage proto enum values
const STAGE_CONFIGURING: i32 = 1;
const STAGE_BUILDING: i32 = 2;
const STAGE_CONVERTING: i32 = 3;
const STAGE_DONE: i32 = 4;
const STAGE_ERROR: i32 = 5;

/// Helper to send a progress message, ignoring send errors (receiver may have dropped).
async fn send_progress(
    tx: &tokio::sync::mpsc::Sender<Result<BuildVmImageProgress, Status>>,
    stage: i32,
    message: impl Into<String>,
    image_path: impl Into<String>,
) {
    let _ = tx
        .send(Ok(BuildVmImageProgress {
            stage,
            message: message.into(),
            image_path: image_path.into(),
        }))
        .await;
}

/// Build a VM image from a Buildroot `.config` spec string, streaming progress messages.
///
/// Stages:
/// 1. STAGE_CONFIGURING — writes spec, runs `make olddefconfig`
/// 2. STAGE_BUILDING    — runs `make -j<nproc>`, forwarding each stdout line
/// 3. STAGE_CONVERTING  — runs `qemu-img convert` to produce a qcow2
/// 4. STAGE_DONE        — sends the final image path
/// 5. STAGE_ERROR       — sent if any step fails (Buildroot not found, make failed, etc.)
pub async fn build_vm_image_from_spec(
    spec: &str,
    tx: tokio::sync::mpsc::Sender<Result<BuildVmImageProgress, Status>>,
) {
    use std::env;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    // Emit immediately so the UI shows feedback before any blocking work begins.
    send_progress(&tx, STAGE_CONFIGURING, "Starting Buildroot build…", "").await;
    log::info!(
        "build_vm_image_from_spec: build requested, spec length={}",
        spec.len()
    );

    // ── 1. Locate the Buildroot source directory ──────────────────────────────
    let buildroot_dir = match env::var("BUILDROOT_DIR") {
        Ok(dir) if !dir.is_empty() => {
            log::info!("build_vm_image_from_spec: BUILDROOT_DIR={}", dir);
            std::path::PathBuf::from(dir)
        }
        _ => {
            log::error!("build_vm_image_from_spec: BUILDROOT_DIR not set");
            send_progress(
                &tx,
                STAGE_ERROR,
                "Buildroot not found: set BUILDROOT_DIR env var to the Buildroot source directory",
                "",
            )
            .await;
            return;
        }
    };

    // ── 2. Resolve output dirs relative to the daemon's cwd (repo root when started via web-dev).
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dl_dir = cwd.join("tmp/buildroot/dl");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let build_path = built_images_dir().join(format!("build-{ts}"));
    if let Err(e) = tokio::fs::create_dir_all(&build_path).await {
        send_progress(
            &tx,
            STAGE_ERROR,
            format!("failed to create build dir {}: {e}", build_path.display()),
            "",
        )
        .await;
        return;
    }
    log::info!(
        "build_vm_image_from_spec: build_path={} dl_dir={}",
        build_path.display(),
        dl_dir.display()
    );

    // ── 3. Write the spec as .config ─────────────────────────────────────────
    let config_path = build_path.join(".config");
    if let Err(e) = tokio::fs::write(&config_path, spec).await {
        send_progress(
            &tx,
            STAGE_ERROR,
            format!("failed to write .config: {e}"),
            "",
        )
        .await;
        return;
    }

    // ── 4. make olddefconfig ─────────────────────────────────────────────────
    log::info!(
        "build_vm_image_from_spec: running make olddefconfig in {}",
        build_path.display()
    );
    send_progress(&tx, STAGE_CONFIGURING, "Running make olddefconfig…", "").await;
    let olddefconfig = Command::new("make")
        .arg("-C")
        .arg(&buildroot_dir)
        .arg(format!("O={}", build_path.display()))
        .arg(format!("BR2_DL_DIR={}", dl_dir.display()))
        .arg("HOSTCC=/usr/bin/gcc")
        .arg("HOSTCXX=/usr/bin/g++")
        .arg("olddefconfig")
        .output()
        .await;
    match olddefconfig {
        Err(e) => {
            send_progress(
                &tx,
                STAGE_ERROR,
                format!("make olddefconfig launch failed: {e}"),
                "",
            )
            .await;
            return;
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            send_progress(
                &tx,
                STAGE_ERROR,
                format!("make olddefconfig failed: {stderr}"),
                "",
            )
            .await;
            return;
        }
        Ok(_) => {}
    }

    // ── 5. make -j<nproc> ────────────────────────────────────────────────────
    let nproc = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    log::info!("build_vm_image_from_spec: running make -j{nproc}");

    let mut make_build = match Command::new("make")
        .arg("-C")
        .arg(&buildroot_dir)
        .arg(format!("O={}", build_path.display()))
        .arg(format!("BR2_DL_DIR={}", dl_dir.display()))
        .arg("HOSTCC=/usr/bin/gcc")
        .arg("HOSTCXX=/usr/bin/g++")
        .arg(format!("-j{nproc}"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            send_progress(
                &tx,
                STAGE_ERROR,
                format!("make build launch failed: {e}"),
                "",
            )
            .await;
            return;
        }
    };

    send_progress(
        &tx,
        STAGE_BUILDING,
        "Building rootfs (this takes several minutes)…",
        "",
    )
    .await;
    // Drain stdout and stderr concurrently — reading only one at a time risks a pipe-buffer deadlock
    // when the other fills up, and also hides error messages that make writes to stderr.
    let stdout = make_build.stdout.take();
    let stderr = make_build.stderr.take();

    let tx_out = tx.clone();
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::info!("buildroot: {}", line);
                send_progress(&tx_out, STAGE_BUILDING, &line, "").await;
            }
        }
    });
    let tx_err = tx.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::warn!("buildroot stderr: {}", line);
                send_progress(&tx_err, STAGE_BUILDING, &line, "").await;
            }
        }
    });
    let _ = tokio::join!(stdout_task, stderr_task);

    let status = match make_build.wait().await {
        Ok(s) => s,
        Err(e) => {
            send_progress(&tx, STAGE_ERROR, format!("make build wait failed: {e}"), "").await;
            return;
        }
    };
    if !status.success() {
        send_progress(
            &tx,
            STAGE_ERROR,
            format!("make build failed (exit {})", status),
            "",
        )
        .await;
        return;
    }

    // ── 6. Convert rootfs.ext2 → qcow2 ──────────────────────────────────────
    let rootfs_ext2 = build_path.join("images").join("rootfs.ext2");
    let qcow2_path = build_path.join("images").join("rootfs.qcow2");

    log::info!(
        "build_vm_image_from_spec: converting {} to qcow2",
        rootfs_ext2.display()
    );
    send_progress(
        &tx,
        STAGE_CONVERTING,
        "Converting rootfs.ext2 to qcow2…",
        "",
    )
    .await;

    let convert = Command::new("qemu-img")
        .arg("convert")
        .arg("-f")
        .arg("raw")
        .arg("-O")
        .arg("qcow2")
        .arg(&rootfs_ext2)
        .arg(&qcow2_path)
        .output()
        .await;

    match convert {
        Err(e) => {
            send_progress(
                &tx,
                STAGE_ERROR,
                format!("qemu-img convert launch failed: {e}"),
                "",
            )
            .await;
            return;
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            send_progress(
                &tx,
                STAGE_ERROR,
                format!("qemu-img convert failed: {stderr}"),
                "",
            )
            .await;
            return;
        }
        Ok(_) => {}
    }

    // ── 7. STAGE_DONE ────────────────────────────────────────────────────────
    // Keep the temp dir alive by leaking it (image must persist for caller to use).
    // In production the daemon owns the image path; the build dir is intentionally leaked here.
    let image_path = qcow2_path.to_string_lossy().into_owned();
    log::info!("build_vm_image_from_spec: done, image_path={}", image_path);
    send_progress(&tx, STAGE_DONE, "Build complete", &image_path).await;
}

/// Build a VM image from the given build target using the tddy-build system.
/// Returns the path to the produced qcow2 image.
pub async fn build_vm_image(repo_root: &Path, build_target: &str) -> Result<PathBuf, VmError> {
    // Discover BUILD.yaml manifests from repo_root
    let discovered =
        discover_build_manifests(repo_root).map_err(|e| VmError::BuildFailed(e.to_string()))?;
    if discovered.is_empty() {
        return Err(VmError::BuildFailed(format!(
            "no BUILD.yaml found under {}",
            repo_root.display()
        )));
    }

    let manifests = discovered.into_iter().map(|(_, m)| m).collect();
    let graph =
        BuildGraph::from_manifests(manifests).map_err(|e| VmError::BuildFailed(e.to_string()))?;

    // Set up plugin registry with QemuPlugin
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(QemuPlugin));

    // Get output path before executing (from the action plan)
    let actions = graph
        .actions_for(build_target, &registry)
        .map_err(|e| VmError::BuildFailed(e.to_string()))?;
    let output_path = actions
        .first()
        .and_then(|a| a.outputs.first())
        .map(|o| repo_root.join(&o.path))
        .ok_or_else(|| {
            VmError::BuildFailed(format!("target '{}' has no output actions", build_target))
        })?;

    // Execute the target
    execute_target(
        repo_root,
        &graph,
        build_target,
        &ExecuteOptions::default(),
        &registry,
    )
    .await
    .map_err(|e| VmError::BuildFailed(e.to_string()))?;

    Ok(output_path)
}
