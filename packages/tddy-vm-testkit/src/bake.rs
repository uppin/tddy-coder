//! Baking a prepared base, once.
//!
//! Every image in the chain is produced the same way — chain a delta overlay onto its
//! parent in `images/02-prepared-base/`, boot it with a NoCloud seed, let cloud-init
//! provision and self-halt — so the three recipes share one orchestrator and differ only in
//! the document they hand it. Nothing is copied: a child holds its own delta and names the
//! parent it was baked from.
//!
//! The cache key is the `0444` seal `build_cloud_init_image` applies once the guest has
//! signalled completion — not the output file's mere existence. The overlay is created in
//! the first seconds of a bake that then runs for hours, and only an in-process failure
//! removes it again, so a Ctrl-C, a panic or the OOM killer leaves a half-provisioned file
//! at the layer's published path. Only the seal says the bake finished.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tddy_vm::cloud_init::{
    build_cloud_init_image, cloud_init_library_paths, CloudInitBuildOptions, CloudInitUserData,
    IsoTool, NinePShare,
};
use tddy_vm::library::is_sealed_file;
use tddy_vm::qemu::uefi_firmware_for;
use tddy_vm::vm::{VmAccel, VmArch};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};

use crate::guest::BootedGuest;
use crate::layout::TestkitLayout;
use crate::recipes::TDDY_SERVICE_USERNAME;

/// What one link in the image chain is baked from.
pub struct BakeSpec {
    /// The name of the prepared base this produces, e.g. `tddy-nix-base`.
    pub name: String,
    /// The image it derives from — the imported cloud image in `images/01-base/` for the
    /// first link, an already prepared base for the ones below it. It becomes the output's
    /// parent: read during the bake, referenced by the finished layer forever after, and
    /// never modified either way.
    pub source_image: PathBuf,
    /// The provisioning document.
    pub user_data: CloudInitUserData,
    /// Shares attached for the duration of the bake only.
    pub nine_p_shares: Vec<NinePShare>,
    /// The host port the bake's guest forwards SSH to. Must be unique among concurrently
    /// running guests: `QemuVmArgs::monitor_socket_path` keys the monitor socket on the
    /// port alone.
    pub ssh_host_port: u16,
    /// How long cloud-init gets to finish and halt the guest.
    pub timeout: Duration,
    /// The overlay size handed to the bake.
    pub disk_size: String,
    pub memory: String,
    pub cpus: u32,
}

impl BakeSpec {
    /// A bake sized for the testkit's guests.
    pub fn new(name: &str, source_image: &Path, user_data: CloudInitUserData) -> Self {
        Self {
            name: name.to_string(),
            source_image: source_image.to_path_buf(),
            user_data,
            nine_p_shares: vec![],
            ssh_host_port: 2222,
            timeout: Duration::from_secs(3600),
            disk_size: "40G".to_string(),
            memory: "4096M".to_string(),
            cpus: 4,
        }
    }

    pub fn with_shares(mut self, shares: Vec<NinePShare>) -> Self {
        self.nine_p_shares = shares;
        self
    }

    pub fn with_ssh_host_port(mut self, port: u16) -> Self {
        self.ssh_host_port = port;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Bake `spec` into `images/02-prepared-base/<name>.qcow2` unless a **finished** layer is
/// already there.
///
/// Returns the prepared overlay path either way, so a caller cannot tell a cache hit from
/// a fresh bake except by how long it waited. An unsealed leftover is re-baked over, which
/// `create_chained_overlay` handles by unlocking the destination before it writes.
pub async fn ensure_prepared_base(
    layout: &TestkitLayout,
    spec: BakeSpec,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf> {
    let library = layout.library();
    library
        .init()
        .map_err(|e| anyhow!("initialising the VM & Image Library: {e}"))?;

    let output = layout.prepared_base_path(&spec.name);
    if is_sealed_file(&output) {
        progress(&format!(
            "{} already prepared at {} — skipping the bake",
            spec.name,
            output.display()
        ));
        return Ok(output);
    }

    if !spec.source_image.exists() {
        return Err(anyhow!(
            "source image {} does not exist",
            spec.source_image.display()
        ));
    }

    progress(&format!(
        "baking {} from {}",
        spec.name,
        spec.source_image.display()
    ));

    // `cloud_init_library_paths` names the 01-base entry separately from the output.
    // Passing `spec.name` for both is deliberate: only the first link in the chain imports
    // a cloud image into `01-base`, and the links below it name their parent straight from
    // `02-prepared-base` — so the `base_image_in_01_base` field goes unused here.
    let paths = cloud_init_library_paths(&library, &spec.name, &spec.name);
    let scratch_dir = library.prepared_base_dir().join(&spec.name);
    tokio::fs::create_dir_all(&scratch_dir)
        .await
        .with_context(|| format!("creating scratch dir {}", scratch_dir.display()))?;

    let arch = VmArch::host();
    let options = CloudInitBuildOptions {
        name: spec.name.clone(),
        base_image_src: spec.source_image.clone(),
        overlay_output: paths.prepared_overlay_output.clone(),
        output_dir: scratch_dir.clone(),
        user_data: spec.user_data,
        disk_size: spec.disk_size,
        memory: spec.memory,
        cpus: spec.cpus,
        ssh_host_port: spec.ssh_host_port,
        timeout: spec.timeout,
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch,
        accel: VmAccel::host_default(),
        firmware: uefi_firmware_for(arch, &scratch_dir, &spec.name)
            .map_err(|e| anyhow!("preparing the bake's UEFI firmware: {e}"))?,
        nine_p_shares: spec.nine_p_shares,
    };

    build_cloud_init_image(&options, progress)
        .await
        .map_err(|e| anyhow!("baking {}: {e}", spec.name))?;

    progress(&format!("{} ready at {}", spec.name, output.display()));
    Ok(output)
}

/// Import the raw cloud image a developer supplied into `images/01-base/`.
///
/// The first and only place a file enters the library from outside it. Nothing here
/// downloads: the path comes from `TDDY_CLOUDINIT_BASE_IMAGE`, set in the environment or
/// in the repo-root `.env`.
pub fn import_supplied_base(layout: &TestkitLayout, source: &Path, name: &str) -> Result<PathBuf> {
    let library = layout.library();
    library
        .init()
        .map_err(|e| anyhow!("initialising the VM & Image Library: {e}"))?;
    library
        .import_base_image(source, name)
        .map_err(|e| anyhow!("importing {} into images/01-base: {e}", source.display()))
}

/// A disposable VM created from a sealed prepared base and booted to an answering sshd.
///
/// The probe a test reaches for when the question is "what did this *layer* end up with",
/// rather than what a guest provisioned per run can do. Created through [`VmLibrary`]
/// rather than booted off the layer directly, because a sealed base is read-only and only
/// ever authorized the key its own bake was seeded with — the per-VM keypair and login seed
/// `create_vm` writes are what make it loginable at all.
///
/// The caller owns the teardown: shut the guest down and call [`VmLibrary::remove_vm`], or
/// the overlay outlives the test in the developer's own library.
pub async fn boot_probe_of_prepared_base(
    layout: &TestkitLayout,
    prepared_base: &str,
    vm_name: &str,
    ssh_host_port: u16,
    ssh_ready_timeout: Duration,
) -> Result<BootedGuest> {
    let library = layout.library();
    // A previous run that died before its teardown would have left this behind, and
    // `qemu-img create` refuses to overwrite.
    let _ = library.remove_vm(vm_name);

    let requested = VmManifest {
        name: vm_name.to_string(),
        prepared_base: Some(prepared_base.to_string()),
        image_path: None,
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: "20G".to_string(),
            ssh_host_port,
            port_forwards: vec![],
            arch: VmArch::host(),
            accel: VmAccel::host_default(),
        },
        login: LoginPolicy {
            username: TDDY_SERVICE_USERNAME.to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    };
    library
        .create_vm(&requested)
        .await
        .map_err(|e| anyhow!("creating the probe overlay off {prepared_base}: {e}"))?;
    // Read back rather than reused: `create_vm` records the per-VM key paths in the
    // manifest it persists, and that key is the only one this guest accepts.
    let manifest = library
        .read_manifest(vm_name)
        .map_err(|e| anyhow!("reading back the probe's manifest: {e}"))?;

    let guest = BootedGuest::boot(&library, &manifest, vec![]).await?;
    guest.wait_for_ssh_ready(ssh_ready_timeout).await?;
    Ok(guest)
}
