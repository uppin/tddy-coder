//! The builder guest: the only thing in this workspace that can produce Linux binaries
//! from a macOS host.
//!
//! This is not a convenience. `tddy-supervisor`, `tddy-daemon` and `tddy-sandbox-runner`
//! have to be Linux/aarch64 ELF binaries to run in the guest under test, and an
//! Apple-Silicon host cannot emit those — so a guest that can is on the critical path.
//!
//! Its overlay is deliberately **long-lived**. Every other VM here is disposable; this one
//! keeps `/opt/tddy/target` between runs, which is the difference between an incremental
//! rebuild and a cold `./release` including `libwebrtc`.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tddy_vm::vm::{VmAccel, VmArch};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};

use crate::bake::{ensure_prepared_base, import_supplied_base, BakeSpec};
use crate::env_file::configured_base_image;
use crate::guest::{read_only_share, writable_share, BootedGuest};
use crate::layout::{TestkitLayout, BUILDER_IMAGE_NAME, NIX_BASE_IMAGE_NAME};
use crate::recipes::{
    builder_user_data, deployed_binaries, install_bundle_paths, nix_base_user_data, DIST_MOUNT_TAG,
    GUEST_CHECKOUT_DIR, GUEST_DIST_MOUNT, GUEST_SOURCE_MOUNT, SOURCE_MOUNT_TAG, SOURCE_NIX_PROFILE,
    TDDY_SERVICE_USERNAME,
};

/// The name the developer-supplied cloud image is imported under.
const SUPPLIED_BASE_NAME: &str = "tddy-testkit-base";

/// Ports reserved for the testkit's guests, chosen clear of the 2231-2243 band the
/// existing `tddy-vm` acceptance tests hand-assign. The monitor socket path is derived
/// from the port alone (`qemu.rs:349`), so a collision is a collision of sockets too.
const NIX_BASE_BAKE_PORT: u16 = 2250;
const BUILDER_BAKE_PORT: u16 = 2251;
const BUILDER_RUN_PORT: u16 = 2252;

/// How long the shared Nix bake gets. Installing Nix multi-user and populating the store
/// dominates it.
const NIX_BASE_BAKE_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// How long the builder bake gets. It swaps the kernel (with a reboot) and then realises
/// the flake's whole dev shell, which is a large download on a cold store.
const BUILDER_BAKE_TIMEOUT: Duration = Duration::from_secs(3 * 60 * 60);

/// How long one `./release` in the guest gets. A warm, incremental build is minutes; a
/// cold one after a toolchain bump is not.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(4 * 60 * 60);

/// The binaries and install-bundle files a builder run produced, on the host.
#[derive(Debug, Clone)]
pub struct BuiltBinaries {
    /// The host directory holding them — `tmp/.tddy/dist/linux-<host arch>`.
    pub dist_dir: PathBuf,
    /// Absolute host paths of the release binaries.
    pub binaries: Vec<PathBuf>,
    /// Absolute host paths of the files `./install` reads out of a checkout.
    pub install_bundle: Vec<PathBuf>,
}

impl BuiltBinaries {
    /// Adopt a dist directory somebody else produced, checking it holds everything a
    /// deployment needs.
    ///
    /// The builder guest exists because a macOS host cannot emit Linux ELF. That is not a
    /// universal constraint: on a Linux x86_64 machine — a CI runner, say — `./release`
    /// produces exactly the same binaries in minutes, and paying for a guest to rebuild
    /// them buys nothing. This is the seam for that case, and it validates rather than
    /// trusts: a dist directory missing a binary would otherwise fail much later, inside a
    /// guest, as an install that cannot find its own payload.
    ///
    /// The directory is flat and named the way [`BuilderVm::build_release`] leaves it —
    /// [`deployed_binaries`] by their own names, [`install_bundle_paths`] by their staged
    /// ones.
    pub fn from_dist_dir(dist_dir: impl Into<PathBuf>) -> Result<Self> {
        let dist_dir = dist_dir.into();
        let binaries = deployed_binaries()
            .into_iter()
            .map(|name| dist_dir.join(name))
            .collect::<Vec<_>>();
        let install_bundle = install_bundle_paths()
            .into_iter()
            .map(|(_, staged_name)| dist_dir.join(staged_name))
            .collect::<Vec<_>>();

        let missing = binaries
            .iter()
            .chain(install_bundle.iter())
            .filter(|path| !path.exists())
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(anyhow!(
                "{} is not a complete dist directory — missing: {}",
                dist_dir.display(),
                missing.join(", ")
            ));
        }

        Ok(Self {
            dist_dir,
            binaries,
            install_bundle,
        })
    }

    /// Everything that has to reach the test host, binaries and bundle together.
    pub fn all_paths(&self) -> Vec<PathBuf> {
        self.binaries
            .iter()
            .chain(self.install_bundle.iter())
            .cloned()
            .collect()
    }
}

/// The builder guest.
pub struct BuilderVm {
    layout: TestkitLayout,
}

impl BuilderVm {
    /// A builder rooted at the repo this crate was compiled in.
    pub fn for_this_repo() -> Self {
        Self {
            layout: TestkitLayout::for_this_repo(),
        }
    }

    pub fn with_layout(layout: TestkitLayout) -> Self {
        Self { layout }
    }

    pub fn layout(&self) -> &TestkitLayout {
        &self.layout
    }

    /// Bake the shared Nix parent, then the builder image on top of it.
    ///
    /// Both steps are no-ops once their output exists, so this is cheap to call on every
    /// run and expensive exactly once.
    pub async fn ensure_images(&self, progress: &(dyn Fn(&str) + Sync)) -> Result<PathBuf> {
        let supplied = configured_base_image().ok_or_else(|| {
            anyhow!(
                "no base image configured — set {} in the environment or in the repo-root \
                 .env to a cloud image already on disk (nothing is ever downloaded)",
                crate::env_file::BASE_IMAGE_ENV
            )
        })?;

        let nix_base_output = self.layout.prepared_base_path(NIX_BASE_IMAGE_NAME);
        if !nix_base_output.exists() {
            let imported = import_supplied_base(&self.layout, &supplied, SUPPLIED_BASE_NAME)?;
            ensure_prepared_base(
                &self.layout,
                BakeSpec::new(NIX_BASE_IMAGE_NAME, &imported, nix_base_user_data())
                    .with_ssh_host_port(NIX_BASE_BAKE_PORT)
                    .with_timeout(NIX_BASE_BAKE_TIMEOUT),
                progress,
            )
            .await?;
        }

        // The builder bake mounts the working copy so `./dev true` has a flake to realise.
        let source_share = read_only_share(self.layout.repo_root(), SOURCE_MOUNT_TAG);
        ensure_prepared_base(
            &self.layout,
            BakeSpec::new(BUILDER_IMAGE_NAME, &nix_base_output, builder_user_data())
                .with_shares(vec![source_share])
                .with_ssh_host_port(BUILDER_BAKE_PORT)
                .with_timeout(BUILDER_BAKE_TIMEOUT),
            progress,
        )
        .await
    }

    /// Boot the builder, rebuild the workspace from the current working copy, and leave
    /// the binaries on the host.
    pub async fn build_release(&self, progress: &(dyn Fn(&str) + Sync)) -> Result<BuiltBinaries> {
        self.ensure_images(progress).await?;

        let dist_dir = self.layout.dist_dir();
        tokio::fs::create_dir_all(&dist_dir)
            .await
            .with_context(|| format!("creating {}", dist_dir.display()))?;

        let manifest = self.ensure_builder_vm().await?;
        let shares = vec![
            read_only_share(self.layout.repo_root(), SOURCE_MOUNT_TAG),
            // The first writable 9p share in the workspace. Every existing call site
            // passes `writable: false`; this is how the guest hands its output back to a
            // host that cannot compile it.
            writable_share(&dist_dir, DIST_MOUNT_TAG),
        ];

        progress("booting the builder guest");
        let mut guest = BootedGuest::boot(&self.layout.library(), &manifest, shares).await?;
        guest.login_on_console(TDDY_SERVICE_USERNAME).await?;

        let result = self.run_build(&mut guest, progress).await;
        // Shut down even when the build failed, so a red run does not leave a guest
        // holding the port the next one needs.
        let shutdown = guest.shutdown().await;
        result?;
        shutdown?;

        self.collect_output(dist_dir)
    }

    /// Everything that happens inside the booted guest.
    async fn run_build(
        &self,
        guest: &mut BootedGuest,
        progress: &(dyn Fn(&str) + Sync),
    ) -> Result<()> {
        let mount_source = format!(
            "sudo mkdir -p {GUEST_SOURCE_MOUNT} && sudo mount -t 9p -o \
             trans=virtio,version=9p2000.L,ro {SOURCE_MOUNT_TAG} {GUEST_SOURCE_MOUNT}"
        );
        let mount_dist = format!(
            "sudo mkdir -p {GUEST_DIST_MOUNT} && sudo mount -t 9p -o \
             trans=virtio,version=9p2000.L {DIST_MOUNT_TAG} {GUEST_DIST_MOUNT}"
        );

        progress("mounting the working copy and the output directory");
        guest
            .run_on_console(&mount_source, Duration::from_secs(120))
            .await?;
        guest
            .run_on_console(&mount_dist, Duration::from_secs(120))
            .await?;

        // Re-sync rather than re-bake: this is what makes a code change cost a boot. The
        // exclusions keep host build state out — `target` in particular holds Mach-O
        // objects that would poison the guest's own `target`.
        progress("syncing the working copy into the guest");
        guest
            .run_on_console(
                &format!(
                    "sudo rsync -a --delete --exclude=/.nix-profile --exclude=/target \
                     --exclude=/node_modules --exclude=/tmp --exclude=/.git \
                     {GUEST_SOURCE_MOUNT}/ {GUEST_CHECKOUT_DIR}/ && sudo chown -R \
                     {TDDY_SERVICE_USERNAME}: {GUEST_CHECKOUT_DIR}"
                ),
                Duration::from_secs(900),
            )
            .await?;

        progress("running ./release in the guest (incremental against the kept target/)");
        guest
            .run_on_console(
                &format!("cd {GUEST_CHECKOUT_DIR} && {SOURCE_NIX_PROFILE} && ./release"),
                RELEASE_TIMEOUT,
            )
            .await?;

        // `./release` does not build the jailed payload, but a sandbox spawn execs it, so
        // a jail fails without it on disk.
        progress("building tddy-sandbox-runner");
        guest
            .run_on_console(
                &format!(
                    "cd {GUEST_CHECKOUT_DIR} && {SOURCE_NIX_PROFILE} && ./dev cargo build \
                     --release -p tddy-sandbox-runner"
                ),
                RELEASE_TIMEOUT,
            )
            .await?;

        progress("copying the binaries and install bundle out over the writable share");
        for binary in deployed_binaries() {
            guest
                .run_on_console(
                    &format!(
                        "sudo cp {GUEST_CHECKOUT_DIR}/target/release/{binary} \
                         {GUEST_DIST_MOUNT}/{binary}"
                    ),
                    Duration::from_secs(300),
                )
                .await?;
        }
        for (path, staged_name) in install_bundle_paths() {
            guest
                .run_on_console(
                    &format!(
                        "sudo cp {GUEST_CHECKOUT_DIR}/{path} {GUEST_DIST_MOUNT}/{staged_name}"
                    ),
                    Duration::from_secs(120),
                )
                .await?;
        }

        // 9p writes are not durable until the guest flushes them, and the host reads the
        // directory the moment this returns.
        guest
            .run_on_console("sync", Duration::from_secs(120))
            .await?;
        Ok(())
    }

    /// Check that everything the guest was asked to produce actually landed on the host.
    fn collect_output(&self, dist_dir: PathBuf) -> Result<BuiltBinaries> {
        let binaries = deployed_binaries()
            .into_iter()
            .map(|name| dist_dir.join(name))
            .collect::<Vec<_>>();
        let install_bundle = install_bundle_paths()
            .into_iter()
            .map(|(_, staged_name)| dist_dir.join(staged_name))
            .collect::<Vec<_>>();

        for path in binaries.iter().chain(install_bundle.iter()) {
            if !path.exists() {
                return Err(anyhow!(
                    "the builder guest reported success but {} is not on the host",
                    path.display()
                ));
            }
        }

        Ok(BuiltBinaries {
            dist_dir,
            binaries,
            install_bundle,
        })
    }

    /// The builder VM's manifest, creating its overlay the first time and reusing it after.
    ///
    /// `VmLibrary::create_vm` writes to a fixed `vm/<name>/<name>.qcow2` and `qemu-img
    /// create` fails outright if that file exists — so reuse has to be an explicit check
    /// rather than a retry.
    async fn ensure_builder_vm(&self) -> Result<VmManifest> {
        let library = self.layout.library();
        let name = self.layout.builder_vm_name();
        let overlay = library.vm_dir(&name).join(format!("{name}.qcow2"));

        if overlay.exists() {
            return library
                .read_manifest(&name)
                .map_err(|e| anyhow!("reading the builder VM's manifest: {e}"));
        }

        let manifest = VmManifest {
            name: name.clone(),
            prepared_base: Some(BUILDER_IMAGE_NAME.to_string()),
            image_path: None,
            run: RunPolicy {
                memory: "8192M".to_string(),
                cpus: 6,
                // Large enough for a Nix store, a checkout and a release `target/`.
                disk_size: "80G".to_string(),
                ssh_host_port: BUILDER_RUN_PORT,
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
            .create_vm(&manifest)
            .await
            .map_err(|e| anyhow!("creating the builder VM's overlay: {e}"))?;
        library
            .read_manifest(&name)
            .map_err(|e| anyhow!("reading back the builder VM's manifest: {e}"))
    }
}
