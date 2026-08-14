//! The image chain: one stock cloud image, one shared Nix-prepared parent, two children.
//!
//! ```text
//! images/01-base/<debian cloud image>       supplied on disk, never downloaded
//!   └── tddy-nix-base    Nix + flakes, the tddy and alice accounts, stock -cloud kernel
//!         ├── tddy-builder     + 9p-capable kernel, + warmed workspace dev shell
//!         └── tddy-test-host   + tddy-clients group, + staging dir
//! ```
//!
//! The shared parent exists because installing Nix is the expensive part of both bakes,
//! and doing it once is the difference between one long wait and two. The tree above is a
//! real qcow2 backing chain: each child names its parent by a relative path and stores only
//! its own delta, so the saving is in disk as well as bake *time* — the test host's few
//! additions cost tens of kilobytes rather than a second copy of Debian plus a Nix store.
//!
//! The split between the two children is the point. `tddy_host::build_tddy_host_image`
//! conflates building with installing — it runs `./release` *and* `./install --systemd`
//! in one guest — so the artefact under test carries a warmed compiler toolchain it would
//! never have in production, and every code change costs a re-bake measured in hours.
//!
//! Here the **builder** is a compiler on wheels that is never asserted against, and the
//! **test host** keeps Debian's stock `-cloud` kernel and gains nothing that could compile
//! anything. Binaries cross from one to the other through the host: 9p out of the
//! builder, scp into the test host.
//!
//! The test host does inherit `/nix` from the shared parent, which a production host would
//! not have. That is a deliberate trade: none of the properties under assertion —
//! delegation, enforced limits, `PR_SET_PDEATHSIG` across a privilege drop, cross-user
//! sessions — depend on whether `/nix` exists, whereas all of them depend on the kernel,
//! and the kernel is what this chain keeps stock.

use tddy_vm::cloud_init::{CloudInitUser, CloudInitUserData, CloudInitWriteFile};
use tddy_vm::tddy_host::ninep_capable_kernel_command;

/// The unprivileged account the daemon runs as on the test host.
pub const TDDY_SERVICE_USERNAME: &str = "tddy";

/// The session account whose sessions must actually run as itself while the daemon runs
/// as [`TDDY_SERVICE_USERNAME`] — the headline claim these tests exist to prove.
pub const ALICE_USERNAME: &str = "alice";

/// The password both guests' accounts accept on the serial console.
///
/// Debian cloud images ship no password at all, and the serial console is the only way in
/// before sshd and cloud-init have finished — it is how a bake that fails halfway is
/// diagnosed rather than guessed at.
pub const GUEST_PASSWORD: &str = "tddy-testkit";

/// The 9p mount tag carrying the working copy into the builder guest.
pub const SOURCE_MOUNT_TAG: &str = "tddy-src";

/// The 9p mount tag the builder guest writes finished binaries back through.
pub const DIST_MOUNT_TAG: &str = "tddy-dist";

/// Where [`SOURCE_MOUNT_TAG`] is mounted in the builder guest.
pub const GUEST_SOURCE_MOUNT: &str = "/mnt/tddy-src";

/// Where [`DIST_MOUNT_TAG`] is mounted in the builder guest.
pub const GUEST_DIST_MOUNT: &str = "/mnt/tddy-dist";

/// The writable checkout the builder guest actually builds in.
///
/// Lives in the guest's own overlay rather than on the share, because a build writes
/// `target/`, `node_modules/` and `.nix-profile`, and because keeping `target/` here is
/// what makes the *next* `./release` incremental instead of a cold rebuild.
pub const GUEST_CHECKOUT_DIR: &str = "/opt/tddy";

/// Where the test host stages the binaries scp'd into it.
pub const GUEST_STAGE_DIR: &str = "/opt/tddy-stage";

const NIX_INSTALLER_URL: &str = "https://nixos.org/nix/install";
const GUEST_NIX_EXTRA_CONF_PATH: &str = "/etc/tddy-nix-extra.conf";

/// Sources the Nix profile. cloud-init's `runcmd` shell is not a login shell, so nothing
/// the installer put in `/etc/profile.d` is on `PATH` without this.
pub const SOURCE_NIX_PROFILE: &str = ". /etc/profile.d/nix.sh";

/// The repo-relative paths `./install --systemd --headless` reads out of a checkout.
///
/// The builder copies these back alongside the binaries so the test host can run the
/// *real* install script. Reimplementing what it does would test the reimplementation:
/// one of the properties under assertion is that the `Delegate=yes` unit this script
/// writes really does produce a writable cgroup subtree, and only the script itself
/// writes that unit.
///
/// `packages/tddy-web/dist` is deliberately absent — the install runs `--headless`, which
/// skips the bundle check. The guest serves `/rpc` and `/api/config` with no UI, and the
/// UI is not what these tests are about.
///
/// Each entry is `(path in the checkout, filename in the flat dist directory)`. The two
/// differ for the AppArmor profile, whose basename is `tddy-daemon` — flattening it under
/// that name into the same directory as the release binaries would overwrite the
/// `tddy-daemon` binary with a text file, and the failure would surface much later as a
/// guest that installs cleanly and then cannot exec its own daemon.
pub fn install_bundle_paths() -> Vec<(&'static str, &'static str)> {
    vec![
        ("install", "install"),
        ("daemon.yaml.production", "daemon.yaml.production"),
        ("supervisor.yaml.production", "supervisor.yaml.production"),
        (
            "packages/tddy-daemon/apparmor/tddy-daemon",
            "apparmor-tddy-daemon",
        ),
    ]
}

/// The release binaries the test host needs on disk.
///
/// `./release` builds the first four. `tddy-sandbox-runner` is not in that list but is
/// the jailed payload every sandbox spawn execs, so a jail fails without it — it is built
/// explicitly alongside.
pub fn deployed_binaries() -> Vec<&'static str> {
    vec![
        "tddy-supervisor",
        "tddy-daemon",
        "tddy-coder",
        "tddy-tools",
        "tddy-sandbox-runner",
    ]
}

/// An account that can log in on the serial console and over SSH.
fn a_loginable_user(name: &str) -> CloudInitUser {
    CloudInitUser {
        name: name.to_string(),
        shell: Some("/bin/bash".to_string()),
        sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
        ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
        plain_text_passwd: Some(GUEST_PASSWORD.to_string()),
        lock_passwd: Some(false),
    }
}

/// The shared parent: Nix with flakes enabled, the two OS accounts, and the handful of
/// packages every later step assumes.
///
/// Deliberately keeps the stock `-cloud` kernel. Nix installs over the network, so
/// nothing here needs 9p, and leaving the kernel alone means the test host inherits the
/// one a real Debian cloud host runs.
pub fn nix_base_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("tddy-nix-base".to_string()),
        // Order is load-bearing for the accounts assertion: the service account the daemon
        // drops to, then the session account whose sessions must outlive that drop as
        // themselves.
        users: vec![
            a_loginable_user(TDDY_SERVICE_USERNAME),
            a_loginable_user(ALICE_USERNAME),
        ],
        packages: vec![
            "rsync".to_string(),
            "curl".to_string(),
            "xz-utils".to_string(),
            "git".to_string(),
            "sudo".to_string(),
            "ca-certificates".to_string(),
        ],
        write_files: vec![CloudInitWriteFile {
            path: GUEST_NIX_EXTRA_CONF_PATH.to_string(),
            // `./release`, `./dev` and `./install` all go through `nix develop`, which
            // needs the flakes experimental features the multi-user installer does not
            // enable on its own. Passed via `--nix-extra-conf-file` rather than written to
            // `/etc/nix/nix.conf` directly, which the installer refuses to run over.
            // Beyond flakes, this tunes Nix's downloader for QEMU's **slirp** user-mode
            // networking. Realising the dev shell pulls gigabytes, and at Nix's default
            // concurrency slirp starts dropping connections part-way through — observed as
            // `unable to download …: Failed sending data to peer` 44 minutes in, failing
            // the whole bake. Fewer parallel connections and longer patience for a stalled
            // one trade throughput for finishing at all.
            content: "experimental-features = nix-command flakes\n\
                      http-connections = 5\n\
                      connect-timeout = 30\n\
                      stalled-download-timeout = 300\n"
                .to_string(),
            permissions: Some("0644".to_string()),
            owner: None,
            defer: None,
        }],
        // `set -e` first: cloud-init concatenates these into one script with no error
        // handling of its own, so without it a failed step only skips the rest of its own
        // `&&` chain and the bake still reports success.
        runcmd: vec![
            "set -e".to_string(),
            // `HOME` is set explicitly because cloud-init runs `runcmd` with a minimal
            // environment that has none, and the Nix installer refuses outright:
            // `install: $HOME is not set`. It downloads the tarball first, so the failure
            // arrives ~50s in and looks like a network problem rather than an env one.
            format!(
                "export HOME=/root && curl -fsSL {NIX_INSTALLER_URL} -o \
                 /tmp/nix-installer.sh && sh /tmp/nix-installer.sh --daemon --yes \
                 --nix-extra-conf-file {GUEST_NIX_EXTRA_CONF_PATH}"
            ),
        ],
        bootcmd: vec![],
    }
}

/// The builder guest, baked off [`nix_base_user_data`]'s output.
///
/// Adds the two things only a builder needs: a kernel that can mount 9p, and a warmed
/// workspace dev shell so the first real `./release` is a compile rather than a download.
///
/// The bake stops short of `./release` itself. Building at bake time would freeze the
/// binaries at whatever the working copy said that day, and the whole reason this guest is
/// long-lived is that each run rsyncs the *current* working copy in and rebuilds
/// incrementally against a `target/` that survived in the overlay.
pub fn builder_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("tddy-builder".to_string()),
        // Accounts, packages and Nix all arrive from the parent image.
        users: vec![],
        packages: vec![],
        write_files: vec![],
        runcmd: vec![
            "set -e".to_string(),
            ninep_capable_kernel_command(),
            format!(
                "mkdir -p {GUEST_SOURCE_MOUNT} && mount -t 9p -o \
                 trans=virtio,version=9p2000.L,ro {SOURCE_MOUNT_TAG} {GUEST_SOURCE_MOUNT}"
            ),
            // The exclusions are host build state, not source: `.nix-profile` points into
            // a /nix/store that does not exist here, while `target` and `node_modules` are
            // large and hold another platform's binaries.
            // `.git` is excluded, not just for size. In a git **worktree** it is a pointer
            // *file* reading `gitdir: <host path>/.git/worktrees/<name>`, which does not
            // exist in the guest — so `nix develop` resolves the flake as
            // `git+file:///opt/tddy`, chases that path, and dies with
            // `failed to resolve path`. Without `.git` the flake is a plain path and nix
            // copies the directory as-is, which is what a bake wants anyway: the guest
            // builds the tree it was given, not a revision it could check out.
            format!(
                "mkdir -p {GUEST_CHECKOUT_DIR} && rsync -a --exclude=/.git \
                 --exclude=/.nix-profile --exclude=/target --exclude=/node_modules \
                 --exclude=/tmp {GUEST_SOURCE_MOUNT}/ {GUEST_CHECKOUT_DIR}/"
            ),
            // `HOME` for the same reason the parent's Nix install needs it: cloud-init's
            // `runcmd` environment has none, and `nix develop` writes a profile and a
            // store-path GC root under it.
            // Retried, because realising this closure is a multi-gigabyte transfer over
            // slirp and a single dropped connection fails the whole bake after ~45
            // minutes. This is a retry, not a fallback: nix resumes from the store paths
            // it already has, each attempt makes progress, and the third failure still
            // fails the bake rather than proceeding with an incomplete shell.
            format!(
                "export HOME=/root && cd {GUEST_CHECKOUT_DIR} && for attempt in 1 2 3; do \
                 {SOURCE_NIX_PROFILE} && ./dev true && break; \
                 [ \"$attempt\" = 3 ] && exit 1; sleep 15; done"
            ),
            format!("chown -R {TDDY_SERVICE_USERNAME}: {GUEST_CHECKOUT_DIR}"),
        ],
        bootcmd: vec![],
    }
}

/// The test host, baked off [`nix_base_user_data`]'s output.
///
/// Nothing here installs tddy. The binaries do not exist yet when this bakes — they are
/// scp'd in per run, which is what makes re-testing a code change a boot instead of a
/// re-bake. And nothing here touches the kernel.
pub fn test_host_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("tddy-test-host".to_string()),
        users: vec![],
        packages: vec![],
        write_files: vec![],
        runcmd: vec![
            "set -e".to_string(),
            // The group `./install` puts the daemon's client-facing socket under. Created
            // here so the install has nothing left to guess about.
            "getent group tddy-clients || groupadd --system tddy-clients".to_string(),
            format!("usermod -a -G tddy-clients {ALICE_USERNAME}"),
            format!(
                "mkdir -p {GUEST_STAGE_DIR} && chown {TDDY_SERVICE_USERNAME}: {GUEST_STAGE_DIR}"
            ),
        ],
        bootcmd: vec![],
    }
}
