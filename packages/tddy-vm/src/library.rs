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
//! [`crate::cloud_init`] (`relative_backing_path`, `overlay_create_argv`), but per-VM
//! overlays use an **absolute** backing path ([`vm_overlay_create_argv`]) since they
//! live in `vm/<name>/`, separate from the read-only `images/02-prepared-base/` — unlike
//! the layers under `images/`, which reference their parents relatively so the whole
//! library relocates as a unit. A per-VM overlay is disposable and never relocated, so it
//! gains nothing from that discipline.

use crate::cloud_init::run_qemu_img;
use crate::image_import::{
    files_have_identical_content, normalise_to_qcow2, supplied_image_format, SuppliedImageFormat,
};
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

    /// `<root>/vm/<name>/seed/nocloud` — the `user-data`/`meta-data` pair packed into
    /// [`Self::vm_seed_iso_path`], laid out as a bake's scratch seed is.
    pub fn vm_seed_dir(&self, name: &str) -> PathBuf {
        self.vm_dir(name).join("seed").join("nocloud")
    }

    /// `<root>/vm/<name>/<name>-seed.iso` — the NoCloud seed a boot of this VM attaches as
    /// a cdrom, carrying the VM's own public key into the guest.
    pub fn vm_seed_iso_path(&self, name: &str) -> PathBuf {
        self.vm_dir(name).join(format!("{name}-seed.iso"))
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

    /// Place `src` into `images/01-base/<name>.qcow2` and lock it read-only (chmod `0o444`
    /// via [`set_readonly_file`]).
    ///
    /// Idempotent, and never destructive. An absent destination is written; a destination that
    /// already holds exactly this image is left alone; a destination that holds a *different*
    /// image is an **error** naming both files. qcow2 records no identity of its parent — only
    /// a path — so replacing a base changes what every layer already chained onto it sits on,
    /// and neither `qemu-img` nor a boot would report anything wrong. Re-importing has to be
    /// safe to repeat (every bake starts with one) without ever being a silent swap.
    ///
    /// A non-qcow2 source is normalised on the way in
    /// ([`crate::image_import::normalise_to_qcow2`]): every layer above `01-base` is created
    /// with `-F qcow2`, so a raw or VMDK base would otherwise fail at the next layer with a
    /// diagnostic about a file this import already accepted.
    ///
    /// `src` must be a whole image. A qcow2 that names a backing file is **rejected**, not
    /// copied: copying moves the referencing image away from the parent its relative
    /// reference resolves against, which strands the copy — a file that looks imported,
    /// passes an `exists()` check, and cannot be opened. `01-base` holds the one image in
    /// the library that has no parent, and this is where that is enforced.
    pub fn import_base_image(&self, src: &Path, name: &str) -> Result<PathBuf, VmError> {
        if names_a_backing_file(&read_image_header(src)?) {
            return Err(VmError::BuildFailed(format!(
                "{} is a qcow2 delta with a backing file; import the whole image it \
                 ultimately derives from instead",
                src.display()
            )));
        }

        let dest = self.base_images_dir().join(format!("{name}.qcow2"));
        let staged = self.stage_base_image(src, &dest)?;
        let placed = place_base_image(&staged, src, &dest);
        if staged != src {
            let _ = std::fs::remove_file(&staged);
        }
        placed.map(|()| dest)
    }

    /// The qcow2 this import would publish: `src` itself when it already is one, or a
    /// normalised temporary copy of it beside `dest` when it is not.
    ///
    /// Staging the conversion means the comparison in [`place_base_image`] is always against
    /// the bytes that would actually land in `01-base`, so a re-import of an unchanged raw
    /// source is the no-op it should be rather than a reported change.
    fn stage_base_image(&self, src: &Path, dest: &Path) -> Result<PathBuf, VmError> {
        match supplied_image_format(src)? {
            SuppliedImageFormat::Qcow2 => Ok(src.to_path_buf()),
            SuppliedImageFormat::Other(format) => {
                let staged = dest.with_extension("qcow2.importing");
                let _ = std::fs::remove_file(&staged);
                normalise_to_qcow2(src, &format, &staged)?;
                Ok(staged)
            }
        }
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
    /// A fresh per-VM SSH keypair is generated into `vm/<name>/` and recorded in the
    /// persisted manifest's [`crate::vm_manifest::LoginPolicy`], so the launcher can reach
    /// the guest as its own user with its own key rather than as `root` with whatever the
    /// ambient agent happens to hold. Any key paths on the caller's `manifest` are
    /// superseded by the generated pair.
    ///
    /// That key only opens the guest because a NoCloud seed carrying it is written
    /// alongside ([`crate::cloud_init::write_vm_login_seed_iso`]) for the boot to attach:
    /// the prepared base authorized the key its *own* bake was seeded with, and nothing
    /// between that bake and this VM re-renders `{{SSH_PUBLIC_KEY}}` into the chain.
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
        run_qemu_img(None, &args)
            .await
            .map_err(VmError::BuildFailed)?;

        // `qemu-img create` accepts a size smaller than the backing file without complaint,
        // and a bake grows the root partition to fill whatever disk it was given — so the
        // guest boots into an initramfs shell with `PARTUUID=… does not exist` instead of
        // reaching userspace, minutes later and nowhere near this line. Cheap to check here,
        // expensive to diagnose anywhere else.
        let base_size = crate::image_import::virtual_size_bytes(&prepared_base_path)?;
        let overlay_size = crate::image_import::virtual_size_bytes(&overlay_path)?;
        if overlay_size < base_size {
            let _ = std::fs::remove_file(&overlay_path);
            return Err(VmError::BuildFailed(format!(
                "disk_size {} gives {} a {overlay_size}-byte disk, smaller than the \
                 {base_size}-byte image it chains onto ({}). The partition table it \
                 inherits would refer to sectors the disk does not have, so the guest would \
                 drop to an initramfs shell rather than boot. Give the VM at least the \
                 size of its prepared base.",
                manifest.run.disk_size,
                manifest.name,
                prepared_base_path.display(),
            )));
        }

        let keys = generate_vm_ssh_keypair(&vm_dir, &manifest.name)?;
        let public_key = std::fs::read_to_string(&keys.public_key_path).map_err(|e| {
            VmError::BuildFailed(format!(
                "failed to read the generated public key {}: {e}",
                keys.public_key_path.display()
            ))
        })?;
        crate::cloud_init::write_vm_login_seed_iso(
            &self.vm_seed_dir(&manifest.name),
            &self.vm_seed_iso_path(&manifest.name),
            &manifest.name,
            &manifest.login.username,
            &public_key,
        )
        .await?;

        let mut manifest = manifest.clone();
        manifest.login.ssh_private_key = Some(keys.private_key_path.display().to_string());
        manifest.login.ssh_public_key = Some(keys.public_key_path.display().to_string());

        self.write_manifest(&manifest)?;
        Ok(overlay_path)
    }
}

/// Publish `staged` as the base image at `dest`, or explain why it must not be.
///
/// `source` is the file the caller actually asked to import — the same as `staged` unless it
/// needed normalising — and is what the refusal names, since that is the path the caller knows.
fn place_base_image(staged: &Path, source: &Path, dest: &Path) -> Result<(), VmError> {
    if dest.exists() {
        if files_have_identical_content(staged, dest)? {
            return Ok(());
        }
        return Err(VmError::BuildFailed(format!(
            "{} already holds a different image than {}; importing over it would invalidate \
             every layer chained onto it, because qcow2 records no identity of its parent and \
             nothing would detect the change. Import under a different name, or remove that \
             base and rebuild the layers above it.",
            dest.display(),
            source.display()
        )));
    }

    std::fs::copy(staged, dest).map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to copy base image {} to {}: {e}",
            source.display(),
            dest.display()
        ))
    })?;
    set_readonly_file(dest)
}

/// The qcow2 header prefix [`names_a_backing_file`] reads: magic (4 bytes), version (4),
/// `backing_file_offset` (8).
const QCOW2_HEADER_PREFIX_LEN: u64 = 16;

/// The first four bytes of every qcow2 image.
const QCOW2_MAGIC: &[u8] = b"QFI\xfb";

/// Read the first [`QCOW2_HEADER_PREFIX_LEN`] bytes of `src`, or fewer if that is all there
/// is — a file too short to hold a qcow2 header is simply not one.
fn read_image_header(src: &Path) -> Result<Vec<u8>, VmError> {
    use std::io::Read;

    let file = std::fs::File::open(src)
        .map_err(|e| VmError::BuildFailed(format!("failed to open {}: {e}", src.display())))?;
    let mut header = Vec::new();
    file.take(QCOW2_HEADER_PREFIX_LEN)
        .read_to_end(&mut header)
        .map_err(|e| VmError::BuildFailed(format!("failed to read {}: {e}", src.display())))?;
    Ok(header)
}

/// Whether `header` is a qcow2 that names a backing file — the head of a chain rather than a
/// whole image.
///
/// Reads the format's own fields (magic at 0, big-endian `backing_file_offset` at 8) instead
/// of shelling out to `qemu-img info`: the question is what the bytes say, and the answer
/// must not depend on an external process being able to open the file.
fn names_a_backing_file(header: &[u8]) -> bool {
    let Some(prefix) = header.get(..QCOW2_HEADER_PREFIX_LEN as usize) else {
        return false;
    };
    if &prefix[..QCOW2_MAGIC.len()] != QCOW2_MAGIC {
        return false;
    }
    prefix[8..]
        .iter()
        .fold(0u64, |acc, b| (acc << 8) | u64::from(*b))
        != 0
}

/// The per-VM SSH keypair written alongside a VM's manifest and overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSshKeypair {
    pub private_key_path: PathBuf,
    pub public_key_path: PathBuf,
}

/// Generate a fresh ed25519 keypair at `<dir>/id_<name>` and `<dir>/id_<name>.pub`.
///
/// Shells out to `ssh-keygen` — the same tool [`crate::cloud_init`] uses to mint the key a
/// seed authorizes — so the on-disk formats are exactly what OpenSSH expects. Any existing
/// pair at those paths is replaced: `ssh-keygen` refuses to overwrite non-interactively,
/// and a half-written pair from a failed earlier attempt must not block a retry.
///
/// The private key is left at mode `0600`; OpenSSH refuses to use anything more permissive.
pub fn generate_vm_ssh_keypair(dir: &Path, name: &str) -> Result<VmSshKeypair, VmError> {
    let private_key_path = dir.join(format!("id_{name}"));
    let public_key_path = dir.join(format!("id_{name}.pub"));
    let _ = std::fs::remove_file(&private_key_path);
    let _ = std::fs::remove_file(&public_key_path);

    let output = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C"])
        .arg(format!("tddy-vm-{name}"))
        .arg("-f")
        .arg(&private_key_path)
        .output()
        .map_err(|e| VmError::BuildFailed(format!("failed to spawn ssh-keygen: {e}")))?;
    if !output.status.success() {
        return Err(VmError::BuildFailed(format!(
            "ssh-keygen exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    set_owner_only_file(&private_key_path)?;
    Ok(VmSshKeypair {
        private_key_path,
        public_key_path,
    })
}

/// Restrict `path` to owner read/write (chmod `0o600`), as OpenSSH requires of a private
/// key and as [`crate::cloud_init`] requires of the rendered `user-data` seed.
///
/// No-op on non-unix platforms — file mode bits have no equivalent there.
#[cfg(unix)]
pub(crate) fn set_owner_only_file(path: &Path) -> Result<(), VmError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to restrict {} to owner-only: {e}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
pub(crate) fn set_owner_only_file(_path: &Path) -> Result<(), VmError> {
    Ok(())
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

/// Whether `path` is a **finished** library layer: present, and locked read-only by
/// [`set_readonly_file`].
///
/// The distinction matters because an overlay exists from the first seconds of a bake that
/// then runs for hours, and only an in-process failure removes it again — a Ctrl-C, a panic
/// or the OOM killer leaves a half-provisioned file at the layer's published path. The seal
/// is applied once the guest has signalled completion, so it is the only mark on disk that
/// says the bake finished.
#[cfg(unix)]
pub fn is_sealed_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o222 == 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn is_sealed_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().readonly())
        .unwrap_or(false)
}

/// Build `qemu-img create -f qcow2 -F qcow2 -b <prepared_base_abs> <overlay> <disk_size>`
/// using an **absolute** backing-file path.
///
/// Contrast [`crate::cloud_init::overlay_create_argv`], which uses a path **relative** to
/// the overlay's own directory ([`crate::cloud_init::relative_backing_path`]) so a library
/// of layers relocates as a unit. Per-VM overlays are disposable and never relocated — they
/// are rebuilt from their prepared base whenever a VM is created — so they pay none of that
/// discipline's cost and name their parent absolutely.
pub fn vm_overlay_create_argv(
    prepared_base_abs: &Path,
    overlay: &Path,
    disk_size: &str,
) -> Vec<String> {
    crate::cloud_init::qcow2_overlay_create_argv(
        &prepared_base_abs.display().to_string(),
        overlay,
        disk_size,
    )
}
