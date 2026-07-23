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

/// Create a private scratch build tree for [`build_image`], with its `dl/`
/// download-cache subdirectory already created.
///
/// Not rooted in `tempfile::tempdir()`: that defers to `$TMPDIR`, which the nix dev shell
/// sets to a fresh, per-session path outside Docker Desktop/Rancher Desktop's default
/// shared folders — a bind mount sourced there silently becomes a disconnected, ephemeral
/// mount inside the VM (writes appear to succeed but never reach the real host path).
/// [`docker_cache_root`] is under `$HOME`, which is reliably shared.
async fn create_scratch_build_dirs() -> Result<(tempfile::TempDir, PathBuf), VmError> {
    let docker_cache = docker_cache_root();
    tokio::fs::create_dir_all(&docker_cache)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to create docker cache dir: {e}")))?;
    let build_dir = tempfile::Builder::new()
        .prefix("build-")
        .tempdir_in(&docker_cache)
        .map_err(|e| VmError::BuildFailed(format!("failed to create temp build dir: {e}")))?;
    let dl_dir = build_dir.path().join("dl");
    tokio::fs::create_dir_all(&dl_dir)
        .await
        .map_err(|e| VmError::BuildFailed(format!("failed to create dl dir: {e}")))?;
    Ok((build_dir, dl_dir))
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
    let (build_dir, dl_dir) = create_scratch_build_dirs().await?;

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

impl PipelineError {
    fn message(&self) -> String {
        match self {
            PipelineError::Failed(m) => m.clone(),
            PipelineError::Cancelled => "cancelled".to_string(),
        }
    }
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

/// Where the Buildroot `make` invocations in [`run_buildroot_pipeline`] actually run.
///
/// Buildroot's own dependency checker (`support/dependencies/dependencies.sh`) requires a
/// real Linux `gcc`/`g++` and several Linux-only tools (e.g. `/usr/bin/file`); macOS's
/// `/usr/bin/gcc` is an Apple Clang trampoline that it explicitly rejects. `Docker` routes
/// the same `make` invocations through a Linux container instead.
enum HostToolchain {
    Native,
    Docker { image_tag: String },
}

/// Directory containing the bundled Dockerfile for the toolchain image, resolved relative
/// to this crate's manifest so it doesn't depend on the process's cwd.
fn docker_toolchain_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker/buildroot-host")
}

/// Tag for the Buildroot host-toolchain image, content-addressed by the Dockerfile's own
/// bytes so an edit to `docker/buildroot-host/Dockerfile` automatically busts the cache —
/// [`ensure_docker_image`] only checks whether a tag already exists, so without this an
/// edited Dockerfile would silently keep reusing a stale image built from the old one.
///
/// Errors rather than falling back to a static tag if the bundled Dockerfile can't be
/// read: silently reverting to the non-content-addressed tag would mean the very staleness
/// bug this exists to prevent could recur without any signal.
fn docker_toolchain_image_tag() -> Result<String, PipelineError> {
    let dockerfile = docker_toolchain_dir().join("Dockerfile");
    let contents = std::fs::read(&dockerfile).map_err(|e| {
        PipelineError::Failed(format!(
            "failed to read bundled Dockerfile {}: {e}",
            dockerfile.display()
        ))
    })?;
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    contents.hash(&mut hasher);
    Ok(format!("tddy-buildroot-host:{:016x}", hasher.finish()))
}

/// Decide where `make` should run. `TDDY_VM_BUILD_TOOLCHAIN=native|docker` overrides the
/// per-OS default; macOS defaults to `docker` since Buildroot cannot build there natively
/// (see [`HostToolchain`]), every other OS defaults to `native`.
fn host_toolchain() -> Result<HostToolchain, PipelineError> {
    Ok(
        match std::env::var("TDDY_VM_BUILD_TOOLCHAIN").ok().as_deref() {
            Some("docker") => HostToolchain::Docker {
                image_tag: docker_toolchain_image_tag()?,
            },
            Some("native") => HostToolchain::Native,
            _ if cfg!(target_os = "macos") => HostToolchain::Docker {
                image_tag: docker_toolchain_image_tag()?,
            },
            _ => HostToolchain::Native,
        },
    )
}

/// Build the image tagged `image_tag` from the bundled Dockerfile if it isn't already
/// present locally (`docker image inspect` first, to avoid rebuilding on every run).
async fn ensure_docker_image(image_tag: &str) -> Result<(), PipelineError> {
    let inspect = tokio::process::Command::new("docker")
        .args(["image", "inspect", image_tag])
        .output()
        .await
        .map_err(|e| PipelineError::Failed(format!("docker image inspect launch failed: {e}")))?;
    if inspect.status.success() {
        return Ok(());
    }

    let build = tokio::process::Command::new("docker")
        .args(["build", "-t", image_tag])
        .arg(docker_toolchain_dir())
        .output()
        .await
        .map_err(|e| PipelineError::Failed(format!("docker build launch failed: {e}")))?;
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr);
        return Err(PipelineError::Failed(format!(
            "docker build for {image_tag} failed: {stderr}"
        )));
    }
    Ok(())
}

/// Query the CPU count actually available inside the Docker VM by running `nproc` in a
/// throwaway container — commonly far lower than the macOS host's (see the call site in
/// [`run_buildroot_pipeline`] for why this matters). Returns `None` if `docker run` itself
/// fails or its output isn't parseable, leaving the caller to fall back to a safe default.
async fn docker_vm_nproc(image_tag: &str) -> Option<usize> {
    let output = tokio::process::Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg(image_tag)
        .arg("nproc")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// Stable, session-independent cache root for Docker-related mirrors.
///
/// Deliberately not `std::env::temp_dir()`: the nix dev shell sets a fresh, per-session
/// `TMPDIR` on every `./dev` invocation, which would defeat caching across runs.
fn docker_cache_root() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".cache/tddy-vm-build"))
        .unwrap_or_else(|_| std::env::temp_dir().join("tddy-vm-build"))
}

/// Remove a stale mirror directory, first restoring write permission — mirrors inherit
/// the Nix store's read-only permissions (via `cp`), which would otherwise block deletion.
async fn clear_stale_mirror(mirror_dir: &Path) -> Result<(), PipelineError> {
    let _ = tokio::process::Command::new("chmod")
        .arg("-R")
        .arg("u+w")
        .arg(mirror_dir)
        .status()
        .await;
    tokio::fs::remove_dir_all(mirror_dir)
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to clear stale buildroot mirror: {e}")))
}

/// Docker Desktop / Rancher Desktop's macOS VM only bind-mounts specific host paths
/// (typically under `$HOME` and system temp dirs) into containers; a path outside that
/// allowlist — like a Nix store path — silently mounts as an *empty* directory instead of
/// erroring. `BUILDROOT_DIR` is exactly such a path, so on [`HostToolchain::Docker`] this
/// mirrors it once into a Docker-shareable cache dir (keyed by the Nix store path's own
/// hash-bearing directory name, so a changed Buildroot source naturally invalidates the
/// cache) and returns the mirror's path for [`make_command`] to mount instead of the
/// original.
///
/// Symlinks are copied as symlinks, not dereferenced: Buildroot's tree intentionally
/// contains dangling symlinks (e.g. `system/skeleton/dev/stdout`, placeholders for the
/// *target* rootfs, never meant to resolve on the host) that `cp -L` fails to stat.
/// Nix store contents are also read-only, which `cp` preserves onto the mirror by
/// default; `chmod -R u+w` afterward keeps the mirror removable so a later re-mirror
/// (e.g. after a partial failure) doesn't get stuck on permission-denied deletes.
async fn docker_shareable_buildroot_dir(buildroot_dir: &Path) -> Result<PathBuf, PipelineError> {
    let key = buildroot_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "buildroot".to_string());
    let mirror_dir = docker_cache_root().join("buildroot-mirror").join(&key);
    let marker = mirror_dir.join(".tddy-mirror-complete");
    if marker.is_file() {
        return Ok(mirror_dir);
    }

    if mirror_dir.exists() {
        clear_stale_mirror(&mirror_dir).await?;
    }
    if let Some(parent) = mirror_dir.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            PipelineError::Failed(format!("failed to create buildroot mirror dir: {e}"))
        })?;
    }

    let status = tokio::process::Command::new("cp")
        .arg("-R")
        .arg(buildroot_dir)
        .arg(&mirror_dir)
        .status()
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to mirror BUILDROOT_DIR: {e}")))?;
    if !status.success() {
        return Err(PipelineError::Failed(format!(
            "failed to mirror BUILDROOT_DIR into {}",
            mirror_dir.display()
        )));
    }

    let chmod_status = tokio::process::Command::new("chmod")
        .arg("-R")
        .arg("u+w")
        .arg(&mirror_dir)
        .status()
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to chmod buildroot mirror: {e}")))?;
    if !chmod_status.success() {
        return Err(PipelineError::Failed(format!(
            "failed to chmod buildroot mirror at {}",
            mirror_dir.display()
        )));
    }

    tokio::fs::write(&marker, b"")
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to write mirror marker: {e}")))?;

    Ok(mirror_dir)
}

/// Sanitize `build_path`'s basename into a valid Docker volume name
/// (`[a-zA-Z0-9][a-zA-Z0-9_.-]*`) for the Docker-managed volume backing `/build` (see
/// [`make_command`] for why it's a named volume rather than a bind mount).
fn docker_build_volume_name(build_path: &Path) -> String {
    let raw = build_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "default".to_string());
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("tddy-vm-build-out-{sanitized}")
}

/// Docker initializes a brand-new named volume as `root:root`, but the build commands in
/// [`make_command`] run as the host `--user` (needed so `/dl`'s bind mount — which, unlike
/// `/build`, must stay a bind mount — is writable under Rancher Desktop's sharing rules).
/// A non-root `--user` then gets `Permission denied` writing into a fresh, still
/// root-owned volume. This runs once, as root (no `--user`; volumes don't have the
/// bind-mount sharing quirks that made running as root problematic there), to `chown`
/// the volume to the host uid:gid before any build command touches it.
async fn ensure_docker_build_volume_ownership(
    image_tag: &str,
    build_volume: &str,
) -> Result<(), PipelineError> {
    #[cfg(unix)]
    let owner = unsafe { format!("{}:{}", libc::geteuid(), libc::getegid()) };
    #[cfg(not(unix))]
    let owner = "0:0".to_string();

    let output = tokio::process::Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg("-v")
        .arg(format!("{build_volume}:/build"))
        .arg(image_tag)
        .arg("chown")
        .arg(&owner)
        .arg("/build")
        .output()
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to launch volume chown: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PipelineError::Failed(format!(
            "failed to chown Docker volume {build_volume}: {stderr}"
        )));
    }
    Ok(())
}

/// Build a `make -C <buildroot_dir> O=<build_path> BR2_DL_DIR=<dl_dir> ... <args>`
/// invocation for `toolchain`.
///
/// On [`HostToolchain::Native`] this runs `make` directly, forcing `HOSTCC`/`HOSTCXX` to
/// `/usr/bin/gcc`/`/usr/bin/g++`. On [`HostToolchain::Docker`] it instead runs `docker run
/// --rm --user <uid>:<gid> -v <buildroot_dir>:/buildroot:ro -v <dl_dir>:/dl -v
/// <docker_build_volume_name(build_path)>:/build -w /buildroot <image_tag> make O=/build
/// BR2_DL_DIR=/dl <args>`.
///
/// `/build` is a Docker-managed **named volume**, not a bind mount: Buildroot's `-jN`
/// build creates and `chmod +x`s many host tool binaries/scripts under `O=`, and that
/// exec-permission change is not reliably visible back through Docker
/// Desktop/Rancher Desktop's macOS bind-mount sharing under concurrent access — observed
/// in practice as a `Permission denied` executing a file Buildroot itself just built and
/// chmod'd (e.g. `host/bin/pkg-config`). A named volume lives natively inside the Linux
/// VM with no cross-platform filesystem semantics involved, sidestepping the whole
/// category of issue; [`extract_docker_build_output`] copies the final `images/`
/// directory out to the real host `build_path` afterward. `/buildroot`/`/dl` stay bind
/// mounts (`/buildroot` is read-only; `/dl` only ever holds unmodified downloaded
/// tarballs, neither hits this issue). The container's own `cc`/`g++` are real Linux
/// compilers, so no `HOSTCC`/`HOSTCXX` override is needed there.
fn make_command(
    toolchain: &HostToolchain,
    buildroot_dir: &Path,
    build_path: &Path,
    dl_dir: &Path,
    args: &[&str],
) -> tokio::process::Command {
    match toolchain {
        HostToolchain::Native => {
            let mut cmd = tokio::process::Command::new("make");
            cmd.arg("-C")
                .arg(buildroot_dir)
                .arg(format!("O={}", build_path.display()))
                .arg(format!("BR2_DL_DIR={}", dl_dir.display()))
                .arg("HOSTCC=/usr/bin/gcc")
                .arg("HOSTCXX=/usr/bin/g++")
                .args(args);
            cmd
        }
        HostToolchain::Docker { image_tag } => {
            let mut cmd = tokio::process::Command::new("docker");
            cmd.arg("run").arg("--rm");
            #[cfg(unix)]
            cmd.arg("--user")
                .arg(format!("{}:{}", unsafe { libc::geteuid() }, unsafe {
                    libc::getegid()
                }));
            cmd.arg("-v")
                .arg(format!("{}:/buildroot:ro", buildroot_dir.display()))
                .arg("-v")
                .arg(format!("{}:/dl", dl_dir.display()))
                .arg("-v")
                .arg(format!("{}:/build", docker_build_volume_name(build_path)))
                .arg("-w")
                .arg("/buildroot")
                .arg(image_tag)
                .arg("make")
                .arg("O=/build")
                .arg("BR2_DL_DIR=/dl")
                .args(args);
            cmd
        }
    }
}

/// Copy the produced `images/` directory out of the Docker-managed volume backing
/// `/build` (see [`make_command`]) into the real host `build_path`, so downstream code
/// finds `build_path/images/rootfs.ext2` exactly as it would under
/// [`HostToolchain::Native`].
async fn extract_docker_build_output(
    image_tag: &str,
    build_volume: &str,
    build_path: &Path,
) -> Result<(), PipelineError> {
    tokio::fs::create_dir_all(build_path)
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to create build output dir: {e}")))?;
    let output = tokio::process::Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg("-v")
        .arg(format!("{build_volume}:/build"))
        .arg("-v")
        .arg(format!("{}:/host-out", build_path.display()))
        .arg(image_tag)
        .arg("cp")
        .arg("-a")
        .arg("/build/images")
        .arg("/host-out/")
        .output()
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to launch output extraction: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PipelineError::Failed(format!(
            "failed to extract build output from Docker volume: {stderr}"
        )));
    }
    Ok(())
}

/// Remove a Docker volume created for one pipeline run. Failures are fatal — a leaked
/// volume is treated as a real error, not swallowed.
async fn remove_docker_volume(name: &str) -> Result<(), String> {
    let output = tokio::process::Command::new("docker")
        .args(["volume", "rm", "-f", name])
        .output()
        .await
        .map_err(|e| format!("failed to launch docker volume rm for {name}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to remove Docker volume {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Attempt to clean up `build_volume`, folding a cleanup failure into `primary`'s message
/// if both fail, so neither the original failure nor the leaked volume goes unreported.
async fn fail_with_volume_cleanup(primary: String, build_volume: &str) -> PipelineError {
    match remove_docker_volume(build_volume).await {
        Ok(()) => PipelineError::Failed(primary),
        Err(cleanup_err) => PipelineError::Failed(format!(
            "{primary}; additionally, Docker volume cleanup failed: {cleanup_err}"
        )),
    }
}

/// Select the toolchain (native vs. Docker), resolve `buildroot_dir` to a
/// Docker-shareable mirror if needed, and — for Docker — ensure the toolchain image
/// exists and the `/build` volume is owned by the host user before any `make` invocation
/// touches it. Returns `(toolchain, buildroot_dir, build_volume)`; `build_volume` is
/// computed unconditionally (simply unused under `Native`) so callers don't re-derive it.
async fn prepare_toolchain(
    buildroot_dir: PathBuf,
    build_path: &Path,
    progress: &ProgressSink<'_>,
) -> Result<(HostToolchain, PathBuf, String), PipelineError> {
    let toolchain = host_toolchain()?;
    let buildroot_dir = if let HostToolchain::Docker { image_tag } = &toolchain {
        progress
            .report("Preparing Buildroot host-toolchain Docker image…")
            .await;
        ensure_docker_image(image_tag).await?;
        progress
            .report("Mirroring Buildroot source into a Docker-shareable cache…")
            .await;
        docker_shareable_buildroot_dir(&buildroot_dir).await?
    } else {
        buildroot_dir
    };

    let build_volume = docker_build_volume_name(build_path);
    if let HostToolchain::Docker { image_tag } = &toolchain {
        if let Err(e) = ensure_docker_build_volume_ownership(image_tag, &build_volume).await {
            return Err(fail_with_volume_cleanup(e.message(), &build_volume).await);
        }
    }

    Ok((toolchain, buildroot_dir, build_volume))
}

/// Copy the `.config` already written to the host `build_path` (see
/// [`resolve_buildroot_source_and_write_config`]) into the Docker-managed volume backing
/// `/build` (see [`make_command`]). No-op under [`HostToolchain::Native`], where
/// `build_path` *is* what the `make` invocation sees as `O=`.
///
/// Necessary because `/build` is a named volume, not a bind mount of `build_path` — nothing
/// written to the host path is otherwise visible inside the container. Without this, `make
/// olddefconfig` finds no existing `.config` in `/build` and silently falls back to
/// Buildroot's bare defaults, discarding the caller's spec entirely while still producing a
/// plausible-looking (but wrong) build.
///
/// Runs as the host `--user` (like the `make` invocations in [`make_command`]), not root:
/// [`ensure_docker_build_volume_ownership`] already chowned `/build`'s directory itself to
/// that user, so a file written by that same user lands with matching ownership — a
/// root-owned `.config` would risk the later non-root `make olddefconfig` being unable to
/// overwrite it.
async fn inject_config_into_build_volume(
    toolchain: &HostToolchain,
    build_path: &Path,
    build_volume: &str,
) -> Result<(), PipelineError> {
    let HostToolchain::Docker { image_tag } = toolchain else {
        return Ok(());
    };

    let mut cmd = tokio::process::Command::new("docker");
    cmd.arg("run").arg("--rm");
    #[cfg(unix)]
    cmd.arg("--user")
        .arg(format!("{}:{}", unsafe { libc::geteuid() }, unsafe {
            libc::getegid()
        }));
    let output = cmd
        .arg("-v")
        .arg(format!("{}:/host-config:ro", build_path.display()))
        .arg("-v")
        .arg(format!("{build_volume}:/build"))
        .arg(image_tag)
        .arg("cp")
        .arg("/host-config/.config")
        .arg("/build/.config")
        .output()
        .await
        .map_err(|e| PipelineError::Failed(format!("failed to launch config injection: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(fail_with_volume_cleanup(
            format!("failed to inject .config into Docker volume: {stderr}"),
            build_volume,
        )
        .await);
    }
    Ok(())
}

/// Run `make olddefconfig` against the `.config` already written to `build_path`,
/// cleaning up the Docker volume on failure.
async fn run_olddefconfig(
    toolchain: &HostToolchain,
    buildroot_dir: &Path,
    build_path: &Path,
    dl_dir: &Path,
    build_volume: &str,
) -> Result<(), PipelineError> {
    let olddefconfig = make_command(
        toolchain,
        buildroot_dir,
        build_path,
        dl_dir,
        &["olddefconfig"],
    )
    .output()
    .await;
    match olddefconfig {
        Err(e) => {
            let msg = format!("make olddefconfig launch failed: {e}");
            Err(if let HostToolchain::Docker { .. } = toolchain {
                fail_with_volume_cleanup(msg, build_volume).await
            } else {
                PipelineError::Failed(msg)
            })
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let msg = format!("make olddefconfig failed: {stderr}");
            Err(if let HostToolchain::Docker { .. } = toolchain {
                fail_with_volume_cleanup(msg, build_volume).await
            } else {
                PipelineError::Failed(msg)
            })
        }
        Ok(_) => Ok(()),
    }
}

/// Run `make -j<nproc>`, streaming stdout/stderr to `progress` and honoring `cancel`;
/// cleans up the Docker volume on every failure/cancellation exit.
///
/// `nproc` is matched to the actual Docker VM's CPU count under `HostToolchain::Docker`
/// (not the host's `std::thread::available_parallelism()`, which can wildly
/// oversubscribe a VM commonly configured with far fewer CPUs/RAM than the host — e.g. 2
/// CPUs / ~6 GiB by default — getting `cc1plus` OOM-killed mid-build with no compiler
/// diagnostic, just a bare `make: *** Error 2`).
///
/// If `cancel` is given and becomes cancelled while `make` is running, the build is
/// interrupted (`SIGINT` to the `make` process group leader on unix) and
/// `Err(PipelineError::Cancelled)` is returned.
async fn run_parallel_build(
    toolchain: &HostToolchain,
    buildroot_dir: &Path,
    build_path: &Path,
    dl_dir: &Path,
    build_volume: &str,
    progress: &ProgressSink<'_>,
    cancel: Option<&CancellationToken>,
) -> Result<(), PipelineError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let nproc = match toolchain {
        HostToolchain::Docker { image_tag } => docker_vm_nproc(image_tag).await.unwrap_or(1),
        HostToolchain::Native => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
    };
    log::info!("run_buildroot_pipeline: running make -j{nproc}");

    let jobs_arg = format!("-j{nproc}");
    let mut make_build =
        match make_command(toolchain, buildroot_dir, build_path, dl_dir, &[&jobs_arg])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                let msg = format!("make build launch failed: {e}");
                return Err(if let HostToolchain::Docker { .. } = toolchain {
                    fail_with_volume_cleanup(msg, build_volume).await
                } else {
                    PipelineError::Failed(msg)
                });
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
                return Err(if let HostToolchain::Docker { .. } = toolchain {
                    match remove_docker_volume(build_volume).await {
                        Ok(()) => PipelineError::Cancelled,
                        Err(cleanup_err) => PipelineError::Failed(format!(
                            "build cancelled, but Docker volume cleanup also failed: {cleanup_err}"
                        )),
                    }
                } else {
                    PipelineError::Cancelled
                });
            }
        }
    };
    let _ = tokio::join!(stdout_task, stderr_task);

    let status = match status {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("make build wait failed: {e}");
            return Err(if let HostToolchain::Docker { .. } = toolchain {
                fail_with_volume_cleanup(msg, build_volume).await
            } else {
                PipelineError::Failed(msg)
            });
        }
    };

    if !status.success() {
        let msg = format!("make build failed (exit {status})");
        return Err(if let HostToolchain::Docker { .. } = toolchain {
            fail_with_volume_cleanup(msg, build_volume).await
        } else {
            PipelineError::Failed(msg)
        });
    }

    Ok(())
}

/// After a successful build, extract the produced `images/` directory from the Docker
/// volume to the real host `build_path` and remove the volume. No-op under
/// `HostToolchain::Native`.
async fn finalize_docker_output(
    toolchain: &HostToolchain,
    build_volume: &str,
    build_path: &Path,
    progress: &ProgressSink<'_>,
) -> Result<(), PipelineError> {
    if let HostToolchain::Docker { image_tag } = toolchain {
        progress
            .report("Extracting build output from Docker volume…")
            .await;
        if let Err(extract_err) =
            extract_docker_build_output(image_tag, build_volume, build_path).await
        {
            return Err(fail_with_volume_cleanup(extract_err.message(), build_volume).await);
        }
        if let Err(cleanup_err) = remove_docker_volume(build_volume).await {
            return Err(PipelineError::Failed(format!(
                "build succeeded, but Docker volume cleanup failed: {cleanup_err}"
            )));
        }
    }
    Ok(())
}

/// Resolve `BUILDROOT_DIR` from the environment and write `spec` as `<build_path>/.config`.
///
/// This only reaches the Docker `HostToolchain`'s `/build` volume via a separate step (see
/// [`inject_config_into_build_volume`]) — writing to `build_path` here only ever touches the
/// host filesystem.
async fn resolve_buildroot_source_and_write_config(
    build_path: &Path,
    spec: &str,
) -> Result<PathBuf, PipelineError> {
    let buildroot_dir = match std::env::var("BUILDROOT_DIR") {
        Ok(dir) if !dir.is_empty() => {
            log::info!("run_buildroot_pipeline: BUILDROOT_DIR={}", dir);
            PathBuf::from(dir)
        }
        _ => {
            log::error!("run_buildroot_pipeline: BUILDROOT_DIR not set");
            return Err(PipelineError::Failed(
                "Buildroot not found: set BUILDROOT_DIR env var to the Buildroot source directory"
                    .to_string(),
            ));
        }
    };

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

    Ok(buildroot_dir)
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
    progress.report("Starting Buildroot build…").await;
    log::info!(
        "run_buildroot_pipeline: build requested, spec length={}",
        spec.len()
    );

    let buildroot_dir = resolve_buildroot_source_and_write_config(build_path, spec).await?;

    // ── Select native vs. containerized make, per-OS default (macOS → docker) ────
    let (toolchain, buildroot_dir, build_volume) =
        prepare_toolchain(buildroot_dir, build_path, progress).await?;

    inject_config_into_build_volume(&toolchain, build_path, &build_volume).await?;

    // ── make olddefconfig ───────────────────────────────────────────────────────
    log::info!(
        "run_buildroot_pipeline: running make olddefconfig in {}",
        build_path.display()
    );
    progress.report("Running make olddefconfig…").await;
    run_olddefconfig(
        &toolchain,
        &buildroot_dir,
        build_path,
        dl_dir,
        &build_volume,
    )
    .await?;

    // ── make -j<nproc> ──────────────────────────────────────────────────────────
    run_parallel_build(
        &toolchain,
        &buildroot_dir,
        build_path,
        dl_dir,
        &build_volume,
        progress,
        cancel,
    )
    .await?;

    finalize_docker_output(&toolchain, &build_volume, build_path, progress).await?;

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
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .map_err(|e| VmError::BuildFailed(e.to_string()))?;

    Ok(output_path)
}
