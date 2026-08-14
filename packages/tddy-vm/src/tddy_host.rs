//! The cloud-init recipe that turns a stock Debian cloud image into a **tddy host**: a
//! guest whose `tddy-daemon`/`tddy-coder`/`tddy-tools` are built from the operator's own
//! working copy and installed as a systemd service.
//!
//! The working copy reaches the guest over a read-only virtio-9p share instead of a git
//! URL, so the image is built from exactly the tree on the operator's disk, with no
//! credentials and nothing to push first.
//!
//! [`tddy_host_user_data`] and the helpers around it are pure document rendering — they
//! describe *what the guest is told to do*. [`build_tddy_host_image`] is the orchestrator
//! that actually bakes that document into a prepared base, by driving the existing
//! [`crate::cloud_init::build_cloud_init_image`] pipeline with the 9p share attached.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::cloud_init::{
    build_cloud_init_image, cloud_init_library_paths, reset_cloud_init_and_reboot,
    CloudInitBuildOptions, CloudInitUser, CloudInitUserData, CloudInitWriteFile, IsoTool,
    NinePShare,
};
use crate::library::VmLibrary;
use crate::qemu::uefi_firmware_for;
use crate::vm::{VmAccel, VmArch, VmError};

/// The virtio-9p `mount_tag` reserved for the operator's working copy. The host side of
/// the bake must attach the share under this tag for [`tddy_host_user_data`]'s mount step
/// to find it.
pub const TDDY_SOURCE_MOUNT_TAG: &str = "tddy-src";

/// Where the read-only 9p share is mounted in the guest.
pub const GUEST_SOURCE_MOUNT: &str = "/mnt/tddy-src";

/// The writable copy of the working copy that the guest actually builds from — the share
/// itself is read-only, and a build writes (`target/`, `.nix-profile`, `node_modules/`).
pub const GUEST_CHECKOUT_DIR: &str = "/opt/tddy";

/// The daemon config path `./install` reads and, crucially, *keeps* when it already
/// exists. cloud-init applies `write_files` before `runcmd`, so the config written here
/// survives the install that follows.
const GUEST_DAEMON_CONFIG_PATH: &str = "/etc/tddy/daemon.yaml";

/// Extra `nix.conf` settings handed to the Nix installer. `./release`, `./dev` and
/// `./install` all go through `nix develop`, which needs the flakes experimental features
/// the multi-user installer does not enable on its own. Passed via the installer's
/// `--nix-extra-conf-file` rather than written directly to `/etc/nix/nix.conf`, which the
/// installer refuses to run over.
const GUEST_NIX_EXTRA_CONF_PATH: &str = "/etc/tddy/nix-extra.conf";

const GUEST_NIX_EXTRA_CONF_CONTENT: &str = "experimental-features = nix-command flakes\n";

/// Sources the Nix profile: cloud-init's `runcmd` shell is not a login shell, so nothing
/// the Nix installer put in `/etc/profile.d` is on `PATH` without this.
const SOURCE_NIX_PROFILE: &str = ". /etc/profile.d/nix.sh";

const NIX_INSTALLER_URL: &str = "https://nixos.org/nix/install";
const NIX_INSTALLER_PATH: &str = "/tmp/nix-installer.sh";

/// Install-time defaults of `./install` (`INSTALL_PREFIX=/usr/local`). The generated
/// daemon config has to name them itself: `./install` only substitutes its
/// `__INSTALL_BIN_DIR__` / `__WEB_BUNDLE_PATH__` placeholders when it creates a config
/// from `daemon.yaml.production`, and it never rewrites one that is already there.
const GUEST_BIN_DIR: &str = "/usr/local/bin";
const GUEST_WEB_BUNDLE_DIR: &str = "/usr/local/share/tddy/web";

/// The guest daemon's Connect/web port, forwarded to the host by the VM's `RunPolicy`.
const GUEST_DAEMON_WEB_PORT: u16 = 8080;

/// The LiveKit common room the guest daemon announces itself on, so the operator's other
/// daemons discover it as a peer and can target it by `daemon_instance_id`.
#[derive(Debug, Clone)]
pub struct LiveKitCommonRoom {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub common_room: String,
}

/// What distinguishes one tddy host from another. Everything else about the recipe is
/// fixed by this module.
#[derive(Debug, Clone)]
pub struct TddyHostSpec {
    pub hostname: String,
    /// The guest's login account, also the identity `./install` runs the daemon service
    /// as (its `INSTALL_DAEMON_USER` default is `tddy`; an account cloud-init already
    /// created is reused rather than recreated).
    pub username: String,
    /// `None` renders a daemon config with no LiveKit section at all — a host that keeps
    /// to itself instead of joining a common room.
    ///
    /// **Known limitation, not an oversight: these credentials are baked into a shared,
    /// world-readable image.** [`build_tddy_host_image`] writes them into
    /// `/etc/tddy/daemon.yaml` inside the prepared base, and prepared bases are
    /// deliberately sealed `0444` in `images/02-prepared-base/` so nothing mutates them.
    /// Two consequences follow:
    ///
    /// - Any local account on the *host* can read the qcow2 and recover the `api_secret`.
    /// - Every VM cloned from the base shares one credential, so rotating it means baking
    ///   a new prepared base — hours, per the PRD's "the bake is slow" known gap — rather
    ///   than restarting a VM.
    ///
    /// The fix is to stop baking the secret and inject it per VM at
    /// `CreateVmFromPreparedBase` time through a small per-VM NoCloud seed
    /// ([`crate::vm::VmConfig::seed_iso`] already exists for exactly this and is currently
    /// unused). That is a design change, tracked in the PRD's known gaps for the
    /// daemon-spawned tddy host VM; until then, treat a baked tddy host base as carrying a
    /// live secret and do not hand one to anybody who should not have the common room.
    pub livekit: Option<LiveKitCommonRoom>,
}

/// Render the cloud-init provisioning document for a tddy host.
///
/// The `runcmd` steps run as root, in order, **as lines of a single shell script** —
/// that is how cloud-init executes `runcmd` — so the `cd` into [`GUEST_CHECKOUT_DIR`]
/// performed by the copy step is the working directory every later build step relies on:
///
/// 1. Mount the operator's working copy read-only from the reserved 9p tag.
/// 2. Copy it into a writable checkout and enter it.
/// 3. Install Nix, because `./release` and `./install` are defined in terms of the repo's
///    nix dev shell.
/// 4. `./release` — the Rust binaries.
/// 5. `bun run build` — the web bundle, which step 6 copies.
/// 6. `./install --systemd` — binaries, units, and service start.
pub fn tddy_host_user_data(spec: &TddyHostSpec) -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some(spec.hostname.clone()),
        users: vec![CloudInitUser {
            name: spec.username.clone(),
            shell: Some("/bin/bash".to_string()),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
            plain_text_passwd: None,
            lock_passwd: None,
        }],
        // Everything the build needs *before* Nix exists: curl + ca-certificates fetch the
        // installer, xz-utils unpacks its tarball, rsync copies the share, and git backs
        // both the flake and cargo's dependency fetches.
        packages: vec![
            "ca-certificates".to_string(),
            "curl".to_string(),
            "git".to_string(),
            "rsync".to_string(),
            "xz-utils".to_string(),
        ],
        runcmd: provisioning_runcmd(),
        write_files: vec![
            // The only tddy config that carries a live credential (the LiveKit
            // `api_secret`), so it is the only one not world-readable: `0640`, owned by
            // root and readable by the group `./install` runs the daemon service as.
            // `defer` is mandatory for that owner — cloud-init applies `write_files`
            // before it creates `users`, and chowning to a group that does not exist yet
            // fails the module outright. The deferred write still lands before `runcmd`.
            CloudInitWriteFile {
                path: GUEST_DAEMON_CONFIG_PATH.to_string(),
                content: daemon_config_yaml(spec),
                permissions: Some("0640".to_string()),
                owner: Some(format!("root:{}", spec.username)),
                defer: Some(true),
            },
            CloudInitWriteFile {
                path: GUEST_NIX_EXTRA_CONF_PATH.to_string(),
                content: GUEST_NIX_EXTRA_CONF_CONTENT.to_string(),
                permissions: Some("0644".to_string()),
                owner: None,
                defer: None,
            },
        ],
        bootcmd: vec![],
    }
}

/// Give the guest a kernel that can mount virtio-9p, rebooting into it if necessary.
///
/// Debian's *cloud* kernel flavour — what every `genericcloud` image runs — is trimmed and
/// ships no 9p modules at all (`/lib/modules/<ver>-cloud-<arch>/kernel/net/9p` does not
/// exist), so `mount -t 9p` fails with `unknown filesystem type '9p'`. Installing the
/// generic flavour is necessary but *not* sufficient: with both installed, GRUB still boots
/// the cloud one. The cloud packages therefore have to go, so only the 9p-capable kernel
/// remains to boot.
///
/// This runs as the first provisioning step and is a no-op once a non-cloud kernel is
/// running, which is what makes the reboot safe: it reboots through
/// [`reset_cloud_init_and_reboot`], so cloud-init re-runs `runcmd` on the next boot and the
/// second pass skips straight past this step to the real work. The `uname` guard is what
/// bounds it to exactly one reboot rather than a loop. The bake's boot argv deliberately
/// omits `-no-reboot` so that reset restarts the guest instead of ending the emulator; see
/// [`crate::cloud_init::cloud_init_boot_argv`].
///
/// cloud-init joins every `runcmd` entry into **one** shell script and runs it without
/// `set -e`, so this step both guards its own early exit and carries the install chain's
/// status out: a failed `apt-get` must abort the whole script with a non-zero status. Were
/// it to exit 0 instead, every later step (mount, copy, Nix, `./release`, `./install`)
/// would be skipped, cloud-init would record no error, and the host would seal and promote
/// a prepared base with no tddy in it — reported as a successful bake.
///
/// Verified end-to-end on `debian-12-genericcloud-arm64`: after this step the guest runs
/// `6.1.0-51-arm64` and mounts the share.
pub fn ninep_capable_kernel_command() -> String {
    format!(
        "if uname -r | grep -q -- '-cloud'; then \
         export DEBIAN_FRONTEND=noninteractive; \
         apt-get update -qq && \
         apt-get install -y -qq linux-image-$(dpkg --print-architecture) && \
         apt-get purge -y -qq $(dpkg-query -W -f='${{Package}}\\n' 'linux-image-*' | grep -- '-cloud') && \
         update-grub && \
         touch {NINEP_KERNEL_STAMP}; \
         kernel_status=$?; \
         if [ \"$kernel_status\" -ne 0 ]; then exit \"$kernel_status\"; fi; \
         {}; \
         exit \"$kernel_status\"; \
         fi",
        reset_cloud_init_and_reboot()
    )
}

/// Marker written once the guest has been given a 9p-capable kernel, so an operator
/// inspecting a baked image can tell the step ran.
const NINEP_KERNEL_STAMP: &str = "/var/lib/tddy-ninep-kernel-installed";

/// The ordered provisioning steps documented on [`tddy_host_user_data`].
///
/// The first entry is `set -e`, because cloud-init concatenates these entries into a single
/// shell script with no error handling of its own. Without it a failed step only skips the
/// rest of *its own* `&&` chain: a failed `rsync`, for instance, would skip only the `cd`
/// into the checkout and leave every later build step running in `/`, where `./release`
/// does not exist — and the script would still exit 0, so cloud-init would report a clean
/// provision of an image with no tddy in it.
fn provisioning_runcmd() -> Vec<String> {
    vec![
        "set -e".to_string(),
        ninep_capable_kernel_command(),
        format!(
            "mkdir -p {GUEST_SOURCE_MOUNT} && mount -t 9p -o trans=virtio,version=9p2000.L,ro \
             {TDDY_SOURCE_MOUNT_TAG} {GUEST_SOURCE_MOUNT}"
        ),
        // The excluded paths are host build state, not source: `.nix-profile` points into
        // a /nix/store that does not exist here (and would wedge `nix develop`), while
        // `target` and `node_modules` are large and may hold another platform's binaries.
        // The `cd` at the end is the working directory for every step below.
        // `.git` is excluded for a reason beyond size: in a git **worktree** it is a
        // pointer *file* naming `<host path>/.git/worktrees/<name>`, which does not exist
        // in the guest — so `nix develop` resolves the flake as `git+file://<checkout>`,
        // follows that path and dies with `failed to resolve path`. Without it the flake
        // is a plain path, which is what a bake wants: the guest builds the tree it was
        // handed, not a revision it could check out.
        format!(
            "rsync -a --exclude=/.git --exclude=/.nix-profile --exclude=/target \
             --exclude=/node_modules {GUEST_SOURCE_MOUNT}/ {GUEST_CHECKOUT_DIR}/ && cd \
             {GUEST_CHECKOUT_DIR}"
        ),
        // `HOME` is exported explicitly for every Nix-touching step: cloud-init runs
        // `runcmd` with a minimal environment that has none, and the installer refuses
        // outright with `install: $HOME is not set` — *after* downloading its tarball, so
        // the failure arrives a minute in and reads like a network problem. `nix develop`
        // needs it too, for the profile and the store-path GC root it writes there.
        format!(
            "export HOME=/root && curl -fsSL {NIX_INSTALLER_URL} -o {NIX_INSTALLER_PATH} && sh \
             {NIX_INSTALLER_PATH} --daemon --yes --nix-extra-conf-file \
             {GUEST_NIX_EXTRA_CONF_PATH}"
        ),
        format!("export HOME=/root && {SOURCE_NIX_PROFILE} && ./release"),
        format!(
            "export HOME=/root && {SOURCE_NIX_PROFILE} && ./dev bun install && ./dev bun run build"
        ),
        format!("export HOME=/root && {SOURCE_NIX_PROFILE} && ./install --systemd"),
    ]
}

/// Render the guest's `daemon.yaml`.
///
/// Deliberately smaller than `daemon.yaml.production`: no `log:` block (under systemd the
/// daemon's output belongs in the journal) and no `auth_storage` (the daemon refuses to
/// start if it is set but unwritable, and `./install` creates only its parent).
pub fn daemon_config_yaml(spec: &TddyHostSpec) -> String {
    let doc = GuestDaemonConfigDoc {
        listen: GuestListen {
            web_port: GUEST_DAEMON_WEB_PORT,
            web_host: "0.0.0.0",
        },
        web_bundle_path: GUEST_WEB_BUNDLE_DIR,
        daemon_instance_id: &spec.hostname,
        livekit: spec.livekit.as_ref().map(|lk| GuestLiveKit {
            url: &lk.url,
            api_key: &lk.api_key,
            api_secret: &lk.api_secret,
            common_room: &lk.common_room,
        }),
        allowed_tools: vec![
            GuestAllowedTool {
                path: format!("{GUEST_BIN_DIR}/tddy-coder"),
                label: "tddy-coder",
            },
            GuestAllowedTool {
                path: format!("{GUEST_BIN_DIR}/tddy-tools"),
                label: "tddy-tools",
            },
        ],
    };

    // Infallible by construction: every field of the document below is an owned `String`,
    // `&str`, `u16`, or a `Vec`/`Option` of those, and YAML can represent all of them. A
    // panic here would mean the struct grew a field serde cannot render — a programming
    // error to fix, not a runtime condition to fall back from.
    let body = serde_yml::to_string(&doc).expect("the guest daemon config must serialize");
    format!("# tddy-daemon config baked into this VM by tddy-vm.\n{body}")
}

/// Serialization shape of the guest's `daemon.yaml`, mirroring the fields
/// `tddy_daemon::config::DaemonConfig` deserializes.
#[derive(Debug, Serialize)]
struct GuestDaemonConfigDoc<'a> {
    listen: GuestListen<'a>,
    web_bundle_path: &'a str,
    daemon_instance_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    livekit: Option<GuestLiveKit<'a>>,
    allowed_tools: Vec<GuestAllowedTool<'a>>,
}

#[derive(Debug, Serialize)]
struct GuestListen<'a> {
    web_port: u16,
    web_host: &'a str,
}

#[derive(Debug, Serialize)]
struct GuestLiveKit<'a> {
    url: &'a str,
    api_key: &'a str,
    api_secret: &'a str,
    common_room: &'a str,
}

#[derive(Debug, Serialize)]
struct GuestAllowedTool<'a> {
    path: String,
    label: &'a str,
}

// ── Bake orchestrator ─────────────────────────────────────────────────────────────

/// The account `./install` runs the guest daemon service as, and therefore the account a VM
/// created from a tddy host prepared base logs in with. Matches `./install`'s own
/// `INSTALL_DAEMON_USER` default.
pub const DEFAULT_TDDY_HOST_USERNAME: &str = "tddy";

/// The progress line [`build_tddy_host_image`] emits when it starts importing the caller's
/// base image into `images/01-base/`.
///
/// Crate-internal: the stage lines exist so `crate::service` can map a progress line onto an
/// RPC stage, not as part of this module's public contract.
pub(crate) const IMPORTING_PROGRESS_LINE: &str = "Importing the base image into the library…";

/// The progress line emitted when the tddy host cloud-init document is rendered and the
/// bake's inputs are prepared. Crate-internal, like [`IMPORTING_PROGRESS_LINE`].
pub(crate) const SEEDING_PROGRESS_LINE: &str = "Rendering the tddy host cloud-init recipe…";

/// The progress line emitted just before the baking boot starts. Every line after it is
/// serial-console output from the guest. Crate-internal, like [`IMPORTING_PROGRESS_LINE`].
pub(crate) const BAKING_PROGRESS_LINE: &str = "Baking the tddy host image…";

/// Everything [`build_tddy_host_image`] needs to turn a stock cloud image into a tddy host
/// prepared base.
#[derive(Debug, Clone)]
pub struct TddyHostBuildOptions {
    /// The library the imported base, the prepared base pair, and the bake's scratch
    /// artifacts all land in.
    pub library: VmLibrary,
    /// Name of the produced prepared base pair (`<name>-base.qcow2` + `<name>.qcow2`).
    pub name: String,
    /// Name the caller's cloud image is cataloged under in `images/01-base/`. Distinct from
    /// `name`: several prepared bases can be baked from one imported base image.
    pub base_image_name: String,
    /// The caller-supplied cloud image. Copied, never mutated, never downloaded.
    pub base_image_src: PathBuf,
    /// The operator's working copy, exported to the guest read-only over virtio-9p under
    /// [`TDDY_SOURCE_MOUNT_TAG`].
    pub source_dir: PathBuf,
    pub spec: TddyHostSpec,
    pub disk_size: String,
    pub memory: String,
    pub cpus: u32,
    pub ssh_host_port: u16,
    pub timeout: Duration,
}

/// Bake a tddy host prepared base: import the caller's cloud image, render the tddy host
/// cloud-init recipe, and drive [`build_cloud_init_image`] with the operator's working copy
/// attached read-only over virtio-9p. Returns the finished overlay path,
/// `images/02-prepared-base/<name>.qcow2`.
///
/// Every serial-console line the bake produces is forwarded to `progress`, prefixed by an
/// importing, a seeding, and a baking stage line, so a streaming caller can attribute output
/// to a stage.
///
/// The bake runs on the host's own architecture and accelerator: emulating a foreign
/// architecture for a build that installs Nix and compiles the whole workspace is not
/// viable, so the caller's image must be a host-architecture one.
///
/// Everything the bake produces other than the overlay itself — the NoCloud seed and its ISO
/// (both of which carry the guest's `daemon.yaml`, LiveKit `api_secret` included), the
/// generated SSH keypair and the boot log — lives in a `0700` scratch directory that is
/// removed on **both** the success and the failure path. The overlay is created straight
/// into `images/02-prepared-base/`, because a layer that names its parent relatively cannot
/// be built somewhere else and moved.
pub async fn build_tddy_host_image(
    options: &TddyHostBuildOptions,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<PathBuf, VmError> {
    let library = &options.library;
    library.init()?;

    progress(IMPORTING_PROGRESS_LINE);
    let imported_base =
        library.import_base_image(&options.base_image_src, &options.base_image_name)?;

    progress(SEEDING_PROGRESS_LINE);
    let paths = cloud_init_library_paths(library, &options.base_image_name, &options.name);
    let scratch_dir = library.prepared_base_dir().join(&options.name);
    create_private_scratch_dir(&scratch_dir)?;
    let arch = VmArch::host();

    let opts = CloudInitBuildOptions {
        name: options.name.clone(),
        base_image_src: imported_base,
        overlay_output: paths.prepared_overlay_output.clone(),
        output_dir: scratch_dir.clone(),
        user_data: tddy_host_user_data(&options.spec),
        disk_size: options.disk_size.clone(),
        memory: options.memory.clone(),
        cpus: options.cpus,
        ssh_host_port: options.ssh_host_port,
        timeout: options.timeout,
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch,
        accel: VmAccel::host_default(),
        firmware: uefi_firmware_for(arch, &scratch_dir, &options.name)?,
        nine_p_shares: vec![NinePShare {
            host_path: options.source_dir.display().to_string(),
            mount_tag: TDDY_SOURCE_MOUNT_TAG.to_string(),
            writable: false,
        }],
    };

    progress(BAKING_PROGRESS_LINE);
    let baked = build_cloud_init_image(&opts, progress).await;
    remove_scratch_dir(&scratch_dir, progress).await;
    baked?;
    Ok(paths.prepared_overlay_output)
}

/// Create the bake's scratch directory restricted to its owner (`0700`).
///
/// It holds the rendered NoCloud seed and the seed ISO, both of which carry the guest's
/// `daemon.yaml` — LiveKit `api_secret` included — as well as the generated SSH private key.
/// None of that may be readable by other local accounts while the bake runs.
fn create_private_scratch_dir(scratch_dir: &std::path::Path) -> Result<(), VmError> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(scratch_dir).map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to create the bake scratch directory {}: {e}",
            scratch_dir.display()
        ))
    })?;
    // `mode` above applies only to a directory this call creates; an abandoned one from an
    // earlier bake may be more permissive.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(scratch_dir, std::fs::Permissions::from_mode(0o700)).map_err(
            |e| {
                VmError::BuildFailed(format!(
                    "failed to restrict the bake scratch directory {} to its owner: {e}",
                    scratch_dir.display()
                ))
            },
        )?;
    }
    Ok(())
}

/// Remove the bake's scratch directory, reporting a failure to do so through `progress`
/// rather than as an error.
///
/// Called on both the success and the failure path. A cleanup failure must never replace the
/// bake's own error — that error is what tells the operator why the bake failed, whereas a
/// leftover scratch directory is a (loud, reported) housekeeping problem.
async fn remove_scratch_dir(scratch_dir: &std::path::Path, progress: &(dyn Fn(&str) + Sync)) {
    // Async: on a failed bake this deletes a full copy of the base image plus the overlay,
    // which is tens of gigabytes and must not block the runtime.
    if let Err(e) = tokio::fs::remove_dir_all(scratch_dir).await {
        progress(&format!(
            "Warning: failed to remove the bake scratch directory {} — it holds the guest \
             daemon config and the generated SSH key: {e}",
            scratch_dir.display()
        ));
    }
}
