//! VM & Image Library — organizes base images, prepared bases, and per-VM state under
//! a single root directory (the caller-resolved tddy data dir; see
//! `tddy-daemon::user_sessions_path::tddy_data_root_matching_child`).
//!
//! Layout:
//! ```text
//! <root>/
//!   images/
//!     01-base/            immutable, downloaded base images (files chmod 0444)
//!     02-prepared-base/   cloud-init-baked prepared bases    (files chmod 0444)
//!   vm/
//!     <name>/
//!       manifest.yaml     how to run, login policy, prepared-base reference
//!       <name>.qcow2      mutable overlay backed by an absolute path to a prepared base
//!       id_<name>[.pub]   SSH keypair for login (private key chmod 0600)
//! ```
//!
//! Image chaining reuses the qcow2 backing-file approach already implemented in
//! [`crate::cloud_init`] (`base_convert_argv`, `overlay_create_argv`), but per-VM
//! overlays use an **absolute** backing path ([`vm_overlay_create_argv`]) since they
//! live in `vm/<name>/`, separate from the read-only `images/02-prepared-base/` — unlike
//! cloud-init's co-located pair, which uses a relative basename so it can be relocated
//! as a unit.

use crate::cloud_init::run_qemu_img;
use crate::vm::VmError;
use crate::vm_manifest::VmManifest;
use std::path::{Path, PathBuf};

/// Subdirectory name for the images root, under the library root.
pub const IMAGES_SUBDIR: &str = "images";
/// Subdirectory name for immutable, downloaded base images, under `images/`.
pub const BASE_IMAGES_SUBDIR: &str = "01-base";
/// Subdirectory name for read-only, cloud-init-baked prepared bases, under `images/`.
pub const PREPARED_BASE_SUBDIR: &str = "02-prepared-base";
/// Subdirectory name for per-VM directories, under the library root.
pub const VMS_SUBDIR: &str = "vm";
/// Filename of the per-VM manifest, inside each `vm/<name>/` directory.
pub const MANIFEST_FILENAME: &str = "manifest.yaml";

/// Root of the VM & Image Library.
#[derive(Debug, Clone)]
pub struct VmLibrary {
    root: PathBuf,
}

impl VmLibrary {
    /// Create a library handle rooted at `root`. Does not touch the filesystem — call
    /// [`VmLibrary::init`] to create the directory tree.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The library root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/images`.
    pub fn images_dir(&self) -> PathBuf {
        self.root.join(IMAGES_SUBDIR)
    }

    /// `<root>/images/01-base` — immutable, downloaded base images.
    pub fn base_images_dir(&self) -> PathBuf {
        self.images_dir().join(BASE_IMAGES_SUBDIR)
    }

    /// `<root>/images/02-prepared-base` — read-only, cloud-init-baked prepared bases.
    pub fn prepared_base_dir(&self) -> PathBuf {
        self.images_dir().join(PREPARED_BASE_SUBDIR)
    }

    /// `<root>/vm` — per-VM directories.
    pub fn vms_dir(&self) -> PathBuf {
        self.root.join(VMS_SUBDIR)
    }

    /// `<root>/vm/<name>`.
    pub fn vm_dir(&self, name: &str) -> PathBuf {
        self.vms_dir().join(name)
    }

    /// Create the full library tree (`images/01-base`, `images/02-prepared-base`,
    /// `vm/`), if not already present.
    pub fn init(&self) -> Result<(), VmError> {
        for dir in [
            self.base_images_dir(),
            self.prepared_base_dir(),
            self.vms_dir(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| {
                VmError::BuildFailed(format!("failed to create {}: {e}", dir.display()))
            })?;
        }
        Ok(())
    }

    /// Copy `src` into `images/01-base/<name>.qcow2` and lock it read-only (chmod
    /// `0o444` via [`set_readonly_file`]). Removes any existing file at the destination
    /// first (unlock-before-overwrite), so re-importing the same name replaces it.
    pub fn import_base_image(&self, src: &Path, name: &str) -> Result<PathBuf, VmError> {
        let dest = self.base_images_dir().join(format!("{name}.qcow2"));
        if dest.exists() {
            std::fs::remove_file(&dest).map_err(|e| {
                VmError::BuildFailed(format!(
                    "failed to remove existing base image {}: {e}",
                    dest.display()
                ))
            })?;
        }
        std::fs::copy(src, &dest).map_err(|e| {
            VmError::BuildFailed(format!(
                "failed to copy base image {} to {}: {e}",
                src.display(),
                dest.display()
            ))
        })?;
        set_readonly_file(&dest)?;
        Ok(dest)
    }

    /// Write `manifest` to `vm/<name>/manifest.yaml`, creating the directory if needed.
    /// Returns the manifest file path.
    pub fn write_manifest(&self, manifest: &VmManifest) -> Result<PathBuf, VmError> {
        let dir = self.vm_dir(&manifest.name);
        std::fs::create_dir_all(&dir).map_err(|e| {
            VmError::BuildFailed(format!("failed to create {}: {e}", dir.display()))
        })?;
        let path = dir.join(MANIFEST_FILENAME);
        let yaml = serde_yml::to_string(manifest)
            .map_err(|e| VmError::BuildFailed(format!("failed to render manifest YAML: {e}")))?;
        std::fs::write(&path, yaml).map_err(|e| {
            VmError::BuildFailed(format!("failed to write {}: {e}", path.display()))
        })?;
        Ok(path)
    }

    /// Read and parse `vm/<name>/manifest.yaml`.
    pub fn read_manifest(&self, name: &str) -> Result<VmManifest, VmError> {
        let path = self.vm_dir(name).join(MANIFEST_FILENAME);
        let yaml =
            std::fs::read_to_string(&path).map_err(|_| VmError::NotFound(name.to_string()))?;
        serde_yml::from_str(&yaml)
            .map_err(|e| VmError::BuildFailed(format!("failed to parse {}: {e}", path.display())))
    }

    /// List every VM manifest currently in the library, by scanning `vm/*/manifest.yaml`.
    ///
    /// Mirrors `build.rs::list_built_images_in`'s tolerance for a missing root: an
    /// absent `vm/` directory yields an empty list, not an error. Entries without a
    /// readable `manifest.yaml` (e.g. a partially-written directory) are skipped.
    pub fn list_manifests(&self) -> Result<Vec<VmManifest>, VmError> {
        let vms_dir = self.vms_dir();
        let entries = match std::fs::read_dir(&vms_dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(Vec::new()),
        };

        let mut manifests = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                VmError::BuildFailed(format!("failed to read {}: {e}", vms_dir.display()))
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Ok(manifest) = self.read_manifest(name) {
                manifests.push(manifest);
            }
        }
        Ok(manifests)
    }

    /// Delete `vm/<name>/` entirely (manifest, overlay, and SSH keys).
    pub fn remove_vm(&self, name: &str) -> Result<(), VmError> {
        let dir = self.vm_dir(name);
        if !dir.exists() {
            return Err(VmError::NotFound(name.to_string()));
        }
        std::fs::remove_dir_all(&dir)
            .map_err(|e| VmError::BuildFailed(format!("failed to remove {}: {e}", dir.display())))
    }

    /// Create `vm/<manifest.name>/`, build its mutable overlay backed by the absolute
    /// path to `images/02-prepared-base/<manifest.prepared_base>.qcow2` (sized per
    /// `manifest.run.disk_size`), and write `manifest.yaml`. Returns the overlay path.
    ///
    /// Requires `manifest.prepared_base` to be `Some` — this is the prepared-base-driven
    /// creation path; manifests that instead set `image_path` reference an
    /// already-existing, library-unmanaged image and are persisted via
    /// [`VmLibrary::write_manifest`] without calling this method.
    pub async fn create_vm(&self, manifest: &VmManifest) -> Result<PathBuf, VmError> {
        let prepared_base = manifest.prepared_base.as_ref().ok_or_else(|| {
            VmError::InvalidState("create_vm requires manifest.prepared_base to be set".to_string())
        })?;
        let prepared_base_path = self
            .prepared_base_dir()
            .join(format!("{prepared_base}.qcow2"));

        let vm_dir = self.vm_dir(&manifest.name);
        std::fs::create_dir_all(&vm_dir).map_err(|e| {
            VmError::BuildFailed(format!("failed to create {}: {e}", vm_dir.display()))
        })?;
        let overlay_path = vm_dir.join(format!("{}.qcow2", manifest.name));

        let args =
            vm_overlay_create_argv(&prepared_base_path, &overlay_path, &manifest.run.disk_size);
        run_qemu_img(&args).await.map_err(VmError::BuildFailed)?;

        self.write_manifest(manifest)?;
        Ok(overlay_path)
    }
}

/// Lock `path` read-only (chmod `0o444`). Used to protect files placed into
/// `images/01-base` and `images/02-prepared-base` from accidental mutation.
///
/// No-op on non-unix platforms — file mode bits have no equivalent there.
#[cfg(unix)]
pub fn set_readonly_file(path: &Path) -> Result<(), VmError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444)).map_err(|e| {
        VmError::BuildFailed(format!("failed to lock {} read-only: {e}", path.display()))
    })
}

#[cfg(not(unix))]
pub fn set_readonly_file(_path: &Path) -> Result<(), VmError> {
    Ok(())
}

/// Build `qemu-img create -f qcow2 -F qcow2 -b <prepared_base_abs> <overlay> <disk_size>`
/// using an **absolute** backing-file path.
///
/// Contrast [`crate::cloud_init::overlay_create_argv`], which uses a **relative**
/// basename so its base+overlay pair can be relocated together — that invariant only
/// holds because cloud-init co-locates the pair in the same directory. Per-VM overlays
/// instead live in `vm/<name>/`, separate from the read-only `images/02-prepared-base/`
/// directory their prepared base lives in, so the backing reference must be absolute.
pub fn vm_overlay_create_argv(
    prepared_base_abs: &Path,
    overlay: &Path,
    disk_size: &str,
) -> Vec<String> {
    vec![
        "create".to_string(),
        "-f".to_string(),
        "qcow2".to_string(),
        "-F".to_string(),
        "qcow2".to_string(),
        "-b".to_string(),
        prepared_base_abs.display().to_string(),
        overlay.display().to_string(),
        disk_size.to_string(),
    ]
}
