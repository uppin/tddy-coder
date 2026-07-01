use crate::vm::VmError;
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_qemu::QemuPlugin;
use tddy_rpc::Status;
use tddy_service::proto::vm::BuildVmImageProgress;
use tddy_task::{TaskBody, TaskChannel, TaskContext, TaskStatus};
use tokio::fs;
use tokio_util::sync::CancellationToken;

use async_trait::async_trait;

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

// ── Build image (spec → explicit output file) ──────────────────────────────────

/// Output disk-image format for [`build_image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// Raw Buildroot rootfs image, unconverted.
    Raw,
    /// QEMU copy-on-write image (`qemu-img convert -O qcow2`).
    Qcow2,
}

/// Build a VM image from a Buildroot `.config` spec and write it to `output` in the
/// requested `format`, reporting each progress line via `progress`.
///
/// This is the pure, output-path-explicit counterpart to [`build_vm_image_from_spec`]:
/// the latter wraps this behind a gRPC progress channel and always writes into a
/// timestamped `tmp/buildroot/disks/build-<ts>` directory. Callers that want a specific
/// output path (e.g. the `tddy-vm-build` CLI) should use this directly.
///
/// Internally this runs the same `make olddefconfig` / `make -j<nproc>` pipeline as
/// [`build_vm_image_from_spec`] inside a private temporary Buildroot build tree (since
/// `output` is a file path, not a directory), then either copies the raw rootfs image
/// straight to `output` (`ImageFormat::Raw`) or runs `qemu-img convert` into `output`
/// (`ImageFormat::Qcow2`).
pub async fn build_image(
    spec: &str,
    output: &Path,
    format: ImageFormat,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf, VmError> {
    let build_dir = tempfile::tempdir()
        .map_err(|e| VmError::BuildFailed(format!("failed to create temp build dir: {e}")))?;
    let dl_dir = build_dir.path().join("dl");
    tokio::fs::create_dir_all(&dl_dir)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to create dl dir: {e}")))?;

    let sink = ProgressSink::Sync(progress);
    let rootfs_ext2 = run_buildroot_pipeline(spec, build_dir.path(), &dl_dir, &sink, None)
        .await
        .map_err(|e| match e {
            PipelineError::Failed(msg) => VmError::BuildFailed(msg),
            // `build_image` never passes a cancellation token, so this is unreachable.
            PipelineError::Cancelled => VmError::BuildFailed("build cancelled".to_string()),
        })?;

    match format {
        ImageFormat::Raw => {
            progress(&format!(
                "Copying raw rootfs image to {}…",
                output.display()
            ));
            tokio::fs::copy(&rootfs_ext2, output).await.map_err(|e| {
                VmError::BuildFailed(format!(
                    "failed to copy raw image to {}: {e}",
                    output.display()
                ))
            })?;
        }
        ImageFormat::Qcow2 => {
            progress(&format!(
                "Converting rootfs.ext2 to qcow2 at {}…",
                output.display()
            ));
            convert_to_qcow2(&rootfs_ext2, output).await.map_err(|e| {
                progress(&e);
                VmError::BuildFailed(e)
            })?;
        }
    }

    progress("Build complete");
    Ok(output.to_path_buf())
}

/// Error from [`run_buildroot_pipeline`], distinguishing a caller-requested cancellation
/// from any other build failure so callers that support cancellation (e.g.
/// [`build_vm_image_from_spec`]) can report it distinctly.
///
/// Carries the raw failure message (not a [`VmError`]) so callers can forward the exact
/// text already reported via `progress` without a `VmError`'s `Display` prefix altering it.
enum PipelineError {
    Failed(String),
    Cancelled,
}

/// Where [`run_buildroot_pipeline`] reports its progress lines.
///
/// [`build_image`]'s public API takes a synchronous `&dyn Fn(&str)` callback (per its
/// signature), while [`build_vm_image_from_spec`] needs to `.await` an async gRPC send for
/// every line so messages are delivered to the RPC caller in order. This enum lets
/// `run_buildroot_pipeline` `.await` a single `report` call either way, without forcing the
/// gRPC path through a synchronous callback (which would require detached `tokio::spawn`
/// sends and lose ordering).
enum ProgressSink<'a> {
    /// Wraps [`build_image`]'s synchronous progress callback.
    Sync(&'a (dyn Fn(&str) + Sync)),
    /// Forwards lines to the `BuildVmImage` RPC's gRPC channel and task log channel, tagging
    /// each with the current build stage.
    Grpc {
        tx: &'a tokio::sync::mpsc::Sender<Result<BuildVmImageProgress, Status>>,
        log_ch: &'a Option<Arc<TaskChannel>>,
        stage: std::sync::atomic::AtomicI32,
    },
}

impl ProgressSink<'_> {
    async fn report(&self, line: &str) {
        match self {
            ProgressSink::Sync(f) => f(line),
            ProgressSink::Grpc { tx, log_ch, stage } => {
                if line.starts_with("Building rootfs") {
                    stage.store(STAGE_BUILDING, std::sync::atomic::Ordering::Relaxed);
                }
                write_to_channel(log_ch, line);
                let current_stage = stage.load(std::sync::atomic::Ordering::Relaxed);
                send_progress(tx, current_stage, line, "").await;
            }
        }
    }
}

/// Run `make olddefconfig` then `make -j<nproc>` for `spec` inside `build_path`, streaming
/// each output line to `progress`. Returns the path to the produced raw rootfs image
/// (`<build_path>/images/rootfs.ext2`) on success.
///
/// If `cancel` is given and becomes cancelled while `make` is running, the build is
/// interrupted (`SIGINT` to the `make` process group leader on unix) and
/// `Err(PipelineError::Cancelled)` is returned.
///
/// Shared by [`build_image`] and [`build_vm_image_from_spec`] so there is a single
/// source of truth for the Buildroot configure/build steps.
async fn run_buildroot_pipeline(
    spec: &str,
    build_path: &Path,
    dl_dir: &Path,
    progress: &ProgressSink<'_>,
    cancel: Option<&CancellationToken>,
) -> Result<PathBuf, PipelineError> {
    use std::env;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    progress.report("Starting Buildroot build…").await;
    log::info!(
        "run_buildroot_pipeline: build requested, spec length={}",
        spec.len()
    );

    // ── Locate the Buildroot source directory ────────────────────────────────
    let buildroot_dir = match env::var("BUILDROOT_DIR") {
        Ok(dir) if !dir.is_empty() => {
            log::info!("run_buildroot_pipeline: BUILDROOT_DIR={}", dir);
            PathBuf::from(dir)
        }
        _ => {
            log::error!("run_buildroot_pipeline: BUILDROOT_DIR not set");
            let msg =
                "Buildroot not found: set BUILDROOT_DIR env var to the Buildroot source directory"
                    .to_string();
            return Err(PipelineError::Failed(msg));
        }
    };

    // ── Create the build dir and write the spec as .config ────────────────────
    tokio::fs::create_dir_all(build_path).await.map_err(|e| {
        PipelineError::Failed(format!(
            "failed to create build dir {}: {e}",
            build_path.display()
        ))
    })?;
    let config_path = build_path.join(".config");
    tokio::fs::write(&config_path, spec)
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to write .config: {e}")))?;

    // ── make olddefconfig ───────────────────────────────────────────────────────
    log::info!(
        "run_buildroot_pipeline: running make olddefconfig in {}",
        build_path.display()
    );
    progress.report("Running make olddefconfig…").await;
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
            return Err(PipelineError::Failed(format!(
                "make olddefconfig launch failed: {e}"
            )));
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(PipelineError::Failed(format!(
                "make olddefconfig failed: {stderr}"
            )));
        }
        Ok(_) => {}
    }

    // ── make -j<nproc> ──────────────────────────────────────────────────────────
    let nproc = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    log::info!("run_buildroot_pipeline: running make -j{nproc}");

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
            return Err(PipelineError::Failed(format!(
                "make build launch failed: {e}"
            )));
        }
    };

    // Capture PID before taking stdio (id() requires mutable Child)
    let make_pid = make_build.id();

    progress
        .report("Building rootfs (this takes several minutes)…")
        .await;
    // Drain stdout and stderr concurrently — reading only one at a time risks a pipe-buffer
    // deadlock when the other fills up, and also hides error messages that make writes to
    // stderr. The line-reading tasks forward lines over an mpsc channel and this function
    // awaits `progress.report(...)` on each as it arrives, so lines stay strictly ordered.
    let stdout = make_build.stdout.take();
    let stderr = make_build.stderr.take();

    let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let line_tx_out = line_tx.clone();
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::info!("buildroot: {}", line);
                let _ = line_tx_out.send(line);
            }
        }
    });
    let line_tx_err = line_tx.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::warn!("buildroot stderr: {}", line);
                let _ = line_tx_err.send(line);
            }
        }
    });
    drop(line_tx);

    // Forward lines to `progress` as they arrive, concurrently with waiting for the child
    // to exit (draining the pipes is what lets the child make progress and eventually exit)
    // and with cancellation, if requested.
    let mut wait_fut = Box::pin(make_build.wait());
    // A never-resolving future when there's no cancellation token, so the select below
    // has a uniform third branch regardless of whether cancellation is supported.
    let cancelled = async {
        match cancel {
            Some(token) => token.cancelled().await,
            None => std::future::pending().await,
        }
    };
    let mut cancelled = Box::pin(cancelled);

    let status = loop {
        tokio::select! {
            maybe_line = line_rx.recv() => {
                if let Some(line) = maybe_line {
                    progress.report(&line).await;
                }
            }
            result = &mut wait_fut => {
                // Drain any remaining buffered lines before finishing.
                while let Ok(line) = line_rx.try_recv() {
                    progress.report(&line).await;
                }
                break result;
            }
            _ = &mut cancelled => {
                #[cfg(unix)]
                if let Some(pid) = make_pid {
                    unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
                }
                stdout_task.abort();
                stderr_task.abort();
                // Drop the in-flight wait future so we can reap the child below without a
                // duplicate-mutable-borrow of `make_build`.
                drop(wait_fut);
                // Reap the child so we don't leave a zombie.
                let _ = make_build.wait().await;
                return Err(PipelineError::Cancelled);
            }
        }
    };
    let _ = tokio::join!(stdout_task, stderr_task);

    let status = match status {
        Ok(s) => s,
        Err(e) => {
            return Err(PipelineError::Failed(format!(
                "make build wait failed: {e}"
            )));
        }
    };

    if !status.success() {
        return Err(PipelineError::Failed(format!(
            "make build failed (exit {})",
            status
        )));
    }

    Ok(build_path.join("images").join("rootfs.ext2"))
}

/// Run `qemu-img convert -f raw -O qcow2` from `input` to `output`.
async fn convert_to_qcow2(input: &Path, output: &Path) -> Result<(), String> {
    let out = tokio::process::Command::new("qemu-img")
        .arg("convert")
        .arg("-f")
        .arg("raw")
        .arg("-O")
        .arg("qcow2")
        .arg(input)
        .arg(output)
        .output()
        .await
        .map_err(|e| format!("qemu-img convert launch failed: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("qemu-img convert failed: {stderr}"));
    }
    Ok(())
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

/// Write a log line to the task channel so `WatchTask` subscribers (Tasks UI) see build output.
fn write_to_channel(ch: &Option<Arc<TaskChannel>>, line: &str) {
    if let Some(ch) = ch {
        ch.write(Bytes::from(format!("{line}\n")));
    }
}

/// `TaskBody` implementation for a Buildroot VM image build.
///
/// Holds the buildroot spec and an mpsc sender for streaming `BuildVmImageProgress` messages
/// back to the `BuildVmImage` RPC caller. The task also writes raw build log lines to its
/// channel "0" for observability via `TaskService.WatchTask`.
pub struct VmBuildTaskBody {
    pub buildroot_spec: String,
    pub progress_tx: tokio::sync::mpsc::Sender<Result<BuildVmImageProgress, Status>>,
}

#[async_trait]
impl TaskBody for VmBuildTaskBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let cancel = ctx.cancel_token().clone();
        let log_ch = ctx.channel("0");
        let success =
            build_vm_image_from_spec(&self.buildroot_spec, self.progress_tx, cancel, log_ch).await;
        if ctx.is_cancelled() {
            TaskStatus::Cancelled
        } else if success {
            TaskStatus::Completed { exit_code: Some(0) }
        } else {
            TaskStatus::Failed {
                message: "VM image build failed".to_string(),
            }
        }
    }
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
    cancel: CancellationToken,
    log_ch: Option<Arc<TaskChannel>>,
) -> bool {
    // ── 1. Resolve output dirs relative to the daemon's cwd (repo root when started via web-dev).
    // The build dir itself is created by `run_buildroot_pipeline` (after the BUILDROOT_DIR
    // check) so an unset BUILDROOT_DIR fails fast without touching the filesystem.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dl_dir = cwd.join("tmp/buildroot/dl");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let build_path = built_images_dir().join(format!("build-{ts}"));
    log::info!(
        "build_vm_image_from_spec: build_path={} dl_dir={}",
        build_path.display(),
        dl_dir.display()
    );

    // ── 2. Run the shared configure/build pipeline, forwarding each progress line to the
    // gRPC channel and the task log channel with the right stage. `run_buildroot_pipeline`
    // reports "Starting Buildroot build…" and "Running make olddefconfig…" while still
    // configuring, then "Building rootfs…" and every subsequent line while building — the
    // sink tracks that transition so stages match the original, un-refactored behavior.
    let sink = ProgressSink::Grpc {
        tx: &tx,
        log_ch: &log_ch,
        stage: std::sync::atomic::AtomicI32::new(STAGE_CONFIGURING),
    };

    let rootfs_ext2 =
        match run_buildroot_pipeline(spec, &build_path, &dl_dir, &sink, Some(&cancel)).await {
            Ok(path) => path,
            Err(PipelineError::Cancelled) => {
                send_progress(&tx, STAGE_ERROR, "Build cancelled", "").await;
                write_to_channel(&log_ch, "Build cancelled");
                return false;
            }
            Err(PipelineError::Failed(msg)) => {
                send_progress(&tx, STAGE_ERROR, &msg, "").await;
                write_to_channel(&log_ch, &msg);
                return false;
            }
        };

    // ── 3. Convert rootfs.ext2 → qcow2 ──────────────────────────────────────
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
    write_to_channel(&log_ch, "Converting rootfs.ext2 to qcow2…");

    if let Err(msg) = convert_to_qcow2(&rootfs_ext2, &qcow2_path).await {
        send_progress(&tx, STAGE_ERROR, &msg, "").await;
        write_to_channel(&log_ch, &msg);
        return false;
    }

    // ── 4. STAGE_DONE ────────────────────────────────────────────────────────
    // Keep the temp dir alive by leaking it (image must persist for caller to use).
    // In production the daemon owns the image path; the build dir is intentionally leaked here.
    let image_path = qcow2_path.to_string_lossy().into_owned();
    log::info!("build_vm_image_from_spec: done, image_path={}", image_path);
    send_progress(&tx, STAGE_DONE, "Build complete", &image_path).await;
    write_to_channel(&log_ch, &format!("Build complete — {image_path}"));
    true
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
