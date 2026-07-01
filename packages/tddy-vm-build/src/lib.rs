//! `tddy-vm-build`: CLI wrapper over `tddy_vm::build_image` — builds a Buildroot spec
//! into a VM image file at a caller-chosen path, independent of the daemon's
//! `BuildVmImage` RPC.

use clap::Parser;
use std::path::PathBuf;
use tddy_vm::ImageFormat;

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
#[command(name = "tddy-vm-build")]
#[command(about = "Build a QEMU VM image from a Buildroot spec and write it to a file")]
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
