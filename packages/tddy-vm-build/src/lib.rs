//! `tddy-vm-build`: CLI wrapper over `tddy_vm::build_image` — builds a Buildroot spec
//! into a VM image file at a caller-chosen path, independent of the daemon's
//! `BuildVmImage` RPC. Also wraps `tddy_vm::cloud_init::build_cloud_init_image` for
//! cloud-init based image-chaining builds via the `cloud-init` subcommand, placing
//! results into the `tddy_vm::library::VmLibrary` (images/01-base,
//! images/02-prepared-base) instead of a bare caller-chosen output directory.

use clap::Parser;
use std::path::{Path, PathBuf};
use tddy_vm::cloud_init::{
    build_cloud_init_image, cloud_init_library_paths, CloudInitBuildOptions, CloudInitUserData,
    IsoTool,
};
use tddy_vm::{set_readonly_file, ImageFormat, VmLibrary};

/// Resolve the default VM & Image Library root when `--library-root` is not given:
/// the profile default (`tmp/.tddy` in debug), falling back to `$HOME/.tddy` — the
/// same fallback chain `tddy-daemon` uses for its own data dir, minus the multi-user
/// `getpwnam` lookup (this CLI always runs as the invoking user).
fn default_library_root() -> Option<PathBuf> {
    tddy_core::output::default_tddy_data_dir()
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".tddy")))
}

/// `tddy-vm-build` top-level CLI: dispatches to the `build` (Buildroot spec) or
/// `cloud-init` (image-chaining) subcommand.
#[derive(Parser, Debug)]
#[command(name = "tddy-vm-build")]
#[command(
    about = "Build a QEMU VM image — from a Buildroot spec, or via cloud-init image-chaining"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Build a QEMU VM image from a Buildroot spec and write it to a file
    Build(BuildImageArgs),
    /// Build a cloud-init-provisioned VM image with image-chaining
    CloudInit(CloudInitBuildArgs),
}

/// Output image format accepted on the CLI.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    #[value(name = "raw")]
    Raw,
    #[value(name = "qcow2")]
    Qcow2,
}

impl From<OutputFormat> for ImageFormat {
    fn from(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Raw => ImageFormat::Raw,
            OutputFormat::Qcow2 => ImageFormat::Qcow2,
        }
    }
}

#[derive(Parser, Debug, Clone)]
pub struct BuildImageArgs {
    /// Path to a Buildroot `.config` spec file.
    #[arg(long)]
    pub spec: PathBuf,

    /// Path to write the built image to.
    #[arg(long)]
    pub output: PathBuf,

    /// Output image format.
    #[arg(long, value_enum, default_value = "qcow2")]
    pub format: OutputFormat,
}

/// Read the spec file and build the image at `args.output`, printing progress to stderr.
pub async fn run_build_image(args: BuildImageArgs) -> anyhow::Result<PathBuf> {
    let spec = tokio::fs::read_to_string(&args.spec)
        .await
        .map_err(|e| anyhow::anyhow!("failed to read spec {}: {e}", args.spec.display()))?;

    let format: ImageFormat = args.format.into();
    tddy_vm::build_image(&spec, &args.output, format, &|line| eprintln!("{line}"))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Arguments for the `cloud-init` subcommand: build a cloud-init-provisioned VM image
/// with image-chaining (immutable base + delta overlay), placed into the VM & Image
/// Library.
#[derive(Parser, Debug, Clone)]
pub struct CloudInitBuildArgs {
    /// Name for the produced image pair (`<name>-base.qcow2` + `<name>.qcow2` in
    /// `images/02-prepared-base/`) and for the imported base image in
    /// `images/01-base/`.
    #[arg(long)]
    pub name: String,

    /// Path to the cached base cloud image to copy (never mutated, never downloaded
    /// by this command).
    #[arg(long, env = "TDDY_CLOUDINIT_BASE_IMAGE")]
    pub base_image: PathBuf,

    /// Root of the VM & Image Library (`images/01-base`, `images/02-prepared-base`,
    /// `vm/`). Defaults to the profile-default tddy data dir, or `$HOME/.tddy`.
    #[arg(long)]
    pub library_root: Option<PathBuf>,

    /// Path to a YAML file matching the `CloudInitUserData` shape (hostname, users,
    /// packages, runcmd, write_files, bootcmd).
    #[arg(long)]
    pub user_data: PathBuf,

    /// Size of the delta overlay.
    #[arg(long, default_value = "20G")]
    pub disk_size: String,

    /// Memory given to the baking VM.
    #[arg(long, default_value = "2048M")]
    pub memory: String,

    /// CPU count given to the baking VM.
    #[arg(long, default_value_t = 2)]
    pub cpus: u32,

    /// Host port to forward the guest's SSH port to during the baking boot.
    #[arg(long, default_value_t = 2222)]
    pub ssh_host_port: u16,

    /// Path to an existing SSH public key to embed. If omitted, a fresh ed25519
    /// keypair is generated in the per-image scratch subdirectory
    /// `images/02-prepared-base/<name>/`, alongside the seed ISO and boot log.
    #[arg(long)]
    pub ssh_public_key: Option<PathBuf>,

    /// Seconds to wait for the cloud-init completion token before giving up.
    #[arg(long, default_value_t = 300)]
    pub timeout_secs: u64,
}

/// Parse the `--user-data` YAML file and build a cloud-init-provisioned, chained VM
/// image into the VM & Image Library, printing progress to stderr.
///
/// 1. Resolves the library root (`--library-root`, or the profile/`$HOME` default).
/// 2. Imports `--base-image` into `images/01-base/<name>.qcow2` (catalogs the raw
///    source; the actual chaining pipeline below still reads from the original path).
/// 3. Runs the existing `build_cloud_init_image` pipeline with `output_dir` set to a
///    per-image scratch directory, `images/02-prepared-base/<name>/` — every artifact
///    the pipeline produces (the qcow2 pair, the NoCloud seed ISO, the generated SSH
///    keypair, the boot log) lands there.
/// 4. Moves the finished qcow2 pair up to the flat `images/02-prepared-base/`
///    location [`cloud_init_library_paths`] resolves (both files together, preserving
///    the overlay's relative backing-file reference to the base), then locks both
///    read-only. The scratch artifacts stay behind in the per-image subdirectory
///    instead of cluttering `02-prepared-base/` with non-image files.
pub async fn run_cloud_init_build(args: CloudInitBuildArgs) -> anyhow::Result<PathBuf> {
    let user_data_yaml = tokio::fs::read_to_string(&args.user_data)
        .await
        .map_err(|e| {
            anyhow::anyhow!("failed to read user-data {}: {e}", args.user_data.display())
        })?;
    let user_data: CloudInitUserData = serde_yml::from_str(&user_data_yaml).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse user-data {} as YAML: {e}",
            args.user_data.display()
        )
    })?;

    let library_root = args
        .library_root
        .or_else(default_library_root)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "could not resolve a VM & Image Library root — pass --library-root explicitly \
             (no $HOME available to fall back to)"
            )
        })?;
    let library = VmLibrary::new(library_root);
    library
        .init()
        .map_err(|e| anyhow::anyhow!("failed to initialize the VM & Image Library: {e}"))?;

    library
        .import_base_image(&args.base_image, &args.name)
        .map_err(|e| anyhow::anyhow!("failed to import base image into images/01-base: {e}"))?;

    let name = args.name;
    let paths = cloud_init_library_paths(&library, &name, &name);
    let scratch_dir = library.prepared_base_dir().join(&name);

    let opts = CloudInitBuildOptions {
        name: name.clone(),
        base_image_src: args.base_image,
        output_dir: scratch_dir.clone(),
        user_data,
        disk_size: args.disk_size,
        memory: args.memory,
        cpus: args.cpus,
        ssh_host_port: args.ssh_host_port,
        timeout: std::time::Duration::from_secs(args.timeout_secs),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: args.ssh_public_key,
    };

    build_cloud_init_image(&opts, &|line| eprintln!("{line}"))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    move_scratch_output(
        &scratch_dir,
        &format!("{name}-base.qcow2"),
        &paths.prepared_base_output,
    )
    .await?;
    move_scratch_output(
        &scratch_dir,
        &format!("{name}.qcow2"),
        &paths.prepared_overlay_output,
    )
    .await?;

    set_readonly_file(&paths.prepared_base_output)
        .map_err(|e| anyhow::anyhow!("failed to lock the prepared base read-only: {e}"))?;
    set_readonly_file(&paths.prepared_overlay_output)
        .map_err(|e| anyhow::anyhow!("failed to lock the provisioned overlay read-only: {e}"))?;

    Ok(paths.prepared_overlay_output)
}

/// Move `scratch_dir.join(filename)` to `dest` — used to promote a finished qcow2 file
/// out of the per-image scratch subdirectory to its flat `images/02-prepared-base/`
/// location, once `build_cloud_init_image` has produced it.
async fn move_scratch_output(
    scratch_dir: &Path,
    filename: &str,
    dest: &Path,
) -> anyhow::Result<()> {
    let src = scratch_dir.join(filename);
    tokio::fs::rename(&src, dest).await.map_err(|e| {
        anyhow::anyhow!(
            "failed to move {} to {}: {e}",
            src.display(),
            dest.display()
        )
    })
}
