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
use tddy_vm::qemu::uefi_firmware_for;
use tddy_vm::{ImageFormat, VmAccel, VmArch, VmLibrary};

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
///
/// The new layer's parent is named exactly one of two ways, and the choice is what makes a
/// chain deeper than one layer possible: `--base-image` starts a fresh chain from a
/// pristine cloud image, `--parent-layer` continues an existing one.
#[derive(Parser, Debug, Clone)]
#[command(group(
    // `required` alone, with `multiple` left on, so this group states only "name a parent".
    // Which parents may be named *together* is `base_image`'s own `conflicts_with`.
    clap::ArgGroup::new("layer_parent").required(true).multiple(true)
        .args(["base_image", "parent_layer"])
))]
pub struct CloudInitBuildArgs {
    /// Name for the produced layer (`<name>.qcow2` in `images/02-prepared-base/`) and, when
    /// `--base-image` starts a new chain, for the image imported into `images/01-base/`.
    #[arg(long)]
    pub name: String,

    /// Path to the pristine base cloud image to import and chain the new layer onto (never
    /// mutated, never downloaded by this command). Mutually exclusive with
    /// `--parent-layer`.
    ///
    /// Deliberately *not* wired to `TDDY_CLOUDINIT_BASE_IMAGE`: clap counts an
    /// env-supplied value as an explicit one, so an exported variable — which is how the
    /// VM testkit is configured — would conflict with every `--parent-layer` invocation
    /// and leave the second layer of a chain unbuildable.
    #[arg(long, group = "layer_parent", conflicts_with = "parent_layer")]
    pub base_image: Option<PathBuf>,

    /// Name of an already-baked layer in `images/02-prepared-base/` to chain the new layer
    /// onto. Mutually exclusive with `--base-image`.
    ///
    /// The parent is used where it lies rather than imported: it is a qcow2 delta that
    /// resolves *its* own parent through a path relative to its directory, so copying it
    /// into `images/01-base/` would strand it — which `VmLibrary::import_base_image`
    /// refuses outright.
    #[arg(long, group = "layer_parent")]
    pub parent_layer: Option<String>,

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

/// Resolve the image a new cloud-init layer chains onto.
///
/// Exactly one parent must be named, and the two are resolved differently:
///
/// - `base_image` is a **whole** cloud image from outside the library. It is imported into
///   `images/01-base/<name>.qcow2` and the new layer chains onto *that* copy rather than
///   the caller's path, so the finished layer's parent lives in the library and the two
///   relocate together.
/// - `parent_layer` names a layer already baked into `images/02-prepared-base/`. It is used
///   exactly where it lies: it is a delta that resolves its own parent through a path
///   relative to its directory, so importing it into `images/01-base/` would strand it —
///   which [`VmLibrary::import_base_image`] refuses outright, and which is why a chain used
///   to stop at one layer.
///
/// The CLI enforces the "exactly one" rule through an argument group, but this is a library
/// entry point too, so it states the rule itself rather than assuming a caller obeyed it.
pub fn resolve_layer_parent(
    library: &VmLibrary,
    name: &str,
    base_image: Option<&Path>,
    parent_layer: Option<&str>,
) -> anyhow::Result<PathBuf> {
    match (base_image, parent_layer) {
        (Some(base_image), None) => library
            .import_base_image(base_image, name)
            .map_err(|e| anyhow::anyhow!("failed to import base image into images/01-base: {e}")),
        (None, Some(parent_layer)) => {
            let path = library
                .prepared_base_dir()
                .join(format!("{parent_layer}.qcow2"));
            if !path.exists() {
                return Err(anyhow::anyhow!(
                    "no prepared layer named `{parent_layer}` in {} — bake it first, or pass \
                     --base-image to start a new chain from a pristine cloud image",
                    library.prepared_base_dir().display()
                ));
            }
            Ok(path)
        }
        _ => Err(anyhow::anyhow!(
            "exactly one of --base-image or --parent-layer must be given"
        )),
    }
}

/// Parse the `--user-data` YAML file and build a cloud-init-provisioned, chained VM
/// image into the VM & Image Library, printing progress to stderr.
///
/// 1. Resolves the library root (`--library-root`, or the profile/`$HOME` default).
/// 2. Resolves the new layer's parent with [`resolve_layer_parent`] — an imported
///    `--base-image` for the first layer of a chain, an existing `--parent-layer` for every
///    layer below it.
/// 3. Runs the `build_cloud_init_image` pipeline, which creates the provisioned delta
///    straight into `images/02-prepared-base/<name>.qcow2` — its backing reference is
///    relative to that directory, so it cannot be built elsewhere and moved — and seals it
///    read-only once baked.
/// 4. Everything else the pipeline produces (the NoCloud seed ISO, the generated SSH
///    keypair, the boot log) lands in the per-image scratch directory
///    `images/02-prepared-base/<name>/`, instead of cluttering `02-prepared-base/` with
///    non-image files.
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

    let parent_image = resolve_layer_parent(
        &library,
        &args.name,
        args.base_image.as_deref(),
        args.parent_layer.as_deref(),
    )?;

    let name = args.name;
    let paths = cloud_init_library_paths(&library, &name, &name);
    let scratch_dir = library.prepared_base_dir().join(&name);

    let arch = VmArch::host();
    let opts = CloudInitBuildOptions {
        name: name.clone(),
        base_image_src: parent_image,
        overlay_output: paths.prepared_overlay_output.clone(),
        output_dir: scratch_dir.clone(),
        user_data,
        disk_size: args.disk_size,
        memory: args.memory,
        cpus: args.cpus,
        ssh_host_port: args.ssh_host_port,
        timeout: std::time::Duration::from_secs(args.timeout_secs),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: args.ssh_public_key,
        arch,
        accel: VmAccel::host_default(),
        firmware: uefi_firmware_for(arch, &scratch_dir, &name)
            .map_err(|e| anyhow::anyhow!("failed to prepare the bake's UEFI firmware: {e}"))?,
        nine_p_shares: vec![],
    };

    build_cloud_init_image(&opts, &|line| eprintln!("{line}"))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(paths.prepared_overlay_output)
}
