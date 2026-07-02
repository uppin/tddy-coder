//! `tddy-vm-build`: CLI wrapper over `tddy_vm::build_image` — builds a Buildroot spec
//! into a VM image file at a caller-chosen path, independent of the daemon's
//! `BuildVmImage` RPC. Also wraps `tddy_vm::cloud_init::build_cloud_init_image` for
//! cloud-init based image-chaining builds via the `cloud-init` subcommand.

use clap::Parser;
use std::path::PathBuf;
use tddy_vm::cloud_init::{
    build_cloud_init_image, CloudInitBuildOptions, CloudInitUserData, IsoTool,
};
use tddy_vm::ImageFormat;

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
/// with image-chaining (immutable base + delta overlay).
#[derive(Parser, Debug, Clone)]
pub struct CloudInitBuildArgs {
    /// Name for the produced image pair (`<name>-base.qcow2` + `<name>.qcow2`).
    #[arg(long)]
    pub name: String,

    /// Path to the cached base cloud image to copy (never mutated, never downloaded
    /// by this command).
    #[arg(long, env = "TDDY_CLOUDINIT_BASE_IMAGE")]
    pub base_image: PathBuf,

    /// Directory to write the base/overlay/seed files into.
    #[arg(long)]
    pub output_dir: PathBuf,

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
    /// keypair is generated in `output_dir`.
    #[arg(long)]
    pub ssh_public_key: Option<PathBuf>,

    /// Seconds to wait for the cloud-init completion token before giving up.
    #[arg(long, default_value_t = 300)]
    pub timeout_secs: u64,
}

/// Parse the `--user-data` YAML file and build a cloud-init-provisioned, chained VM
/// image, printing progress to stderr.
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

    let opts = CloudInitBuildOptions {
        name: args.name,
        base_image_src: args.base_image,
        output_dir: args.output_dir,
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
        .map_err(|e| anyhow::anyhow!("{e}"))
}
