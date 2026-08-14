//! The guest under test: a real Linux host running the supervised stack on real cgroupfs.
//!
//! Its overlay is disposable by design. Every run gets a fresh one off the prepared base,
//! because the cgroup state these tests assert on — scopes created, populated, emptied and
//! removed — must never be inherited from the run before.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tddy_vm::vm::{VmAccel, VmArch};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};

use crate::bake::{ensure_prepared_base, BakeSpec};
use crate::builder_vm::{BuilderVm, BuiltBinaries};
use crate::guest::{BootedGuest, SSH_READY_TIMEOUT};
use crate::layout::{TestkitLayout, NIX_BASE_IMAGE_NAME, TEST_HOST_IMAGE_NAME};
use crate::recipes::{
    deployed_binaries, test_host_user_data, ALICE_USERNAME, GUEST_STAGE_DIR, TDDY_SERVICE_USERNAME,
};

const TEST_HOST_BAKE_PORT: u16 = 2253;
const TEST_HOST_RUN_PORT: u16 = 2254;
const TEST_HOST_BAKE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How long one `./install --systemd --headless` gets. A ceiling on a single run, not a
/// budget spent re-running it: the install writes systemd units, creates accounts and moves
/// binaries into place, so a second attempt over the top of a half-finished first one
/// diagnoses nothing.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

/// The daemon's guest-side config path. `./install` renders this and then, crucially,
/// keeps it if it already exists.
const GUEST_DAEMON_CONFIG: &str = "/etc/tddy/daemon.yaml";

/// The HMAC secret the guest daemon signs session tokens with.
///
/// `livekit.api_secret` doubles as that secret, so leaving the block absent is not the
/// clean escape it looks like: with no secret the daemon answers `Unauthenticated` to
/// every RPC and has no fallback. It is configured here even though LiveKit is never used,
/// and exposed so the host can mint its own tokens against it.
pub const SESSION_TOKEN_SECRET: &str = "tddy-testkit-session-secret";

/// A provisioned test host, ready to be asserted against.
pub struct TestHostVm {
    layout: TestkitLayout,
    guest: BootedGuest,
    vm_name: String,
}

impl TestHostVm {
    /// Bake if needed, boot a fresh guest, deploy freshly built binaries, and install the
    /// supervised stack.
    ///
    /// `binaries` comes from [`BuilderVm::build_release`] — they cannot be built on the
    /// host, so the builder guest is a prerequisite rather than an alternative.
    pub async fn start(
        layout: &TestkitLayout,
        binaries: &BuiltBinaries,
        progress: &(dyn Fn(&str) + Sync),
    ) -> Result<Self> {
        Self::ensure_image(layout, progress).await?;

        let vm_name = layout.test_host_vm_name(std::process::id());
        let manifest = Self::create_disposable_vm(layout, &vm_name).await?;

        progress("booting the test host");
        let mut guest = BootedGuest::boot(&layout.library(), &manifest, vec![]).await?;
        guest.login_on_console(TDDY_SERVICE_USERNAME).await?;

        let mut host = Self {
            layout: layout.clone(),
            guest,
            vm_name,
        };
        host.deploy(binaries, progress).await?;
        Ok(host)
    }

    /// Bake the test-host prepared base, reusing the shared Nix parent.
    ///
    /// The parent is baked by [`BuilderVm::ensure_images`] if it does not exist yet — the
    /// two children share it, and whichever runs first pays for it.
    async fn ensure_image(layout: &TestkitLayout, progress: &(dyn Fn(&str) + Sync)) -> Result<()> {
        let nix_base = layout.prepared_base_path(NIX_BASE_IMAGE_NAME);
        if !nix_base.exists() {
            BuilderVm::with_layout(layout.clone())
                .ensure_images(progress)
                .await?;
        }
        // Chain straight onto the shared parent: the test host is its own delta on top of
        // the Nix layer, which stays put and unmodified beneath both children.
        ensure_prepared_base(
            layout,
            BakeSpec::new(TEST_HOST_IMAGE_NAME, &nix_base, test_host_user_data())
                .with_ssh_host_port(TEST_HOST_BAKE_PORT)
                .with_timeout(TEST_HOST_BAKE_TIMEOUT),
            progress,
        )
        .await?;
        Ok(())
    }

    /// Create this run's own overlay off the prepared base.
    async fn create_disposable_vm(layout: &TestkitLayout, vm_name: &str) -> Result<VmManifest> {
        let library = layout.library();
        // A previous run that died before its teardown would have left this behind, and
        // `qemu-img create` refuses to overwrite.
        let _ = library.remove_vm(vm_name);

        let manifest = VmManifest {
            name: vm_name.to_string(),
            prepared_base: Some(TEST_HOST_IMAGE_NAME.to_string()),
            image_path: None,
            run: RunPolicy {
                memory: "4096M".to_string(),
                cpus: 4,
                disk_size: "40G".to_string(),
                ssh_host_port: TEST_HOST_RUN_PORT,
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
            .map_err(|e| anyhow!("creating the test host's overlay: {e}"))?;
        library
            .read_manifest(vm_name)
            .map_err(|e| anyhow!("reading back the test host's manifest: {e}"))
    }

    /// Copy the binaries in and run the real `./install --systemd`.
    async fn deploy(
        &mut self,
        binaries: &BuiltBinaries,
        progress: &(dyn Fn(&str) + Sync),
    ) -> Result<()> {
        // Once, before anything is copied or executed. Everything below this line runs
        // exactly once, so a failure is reported as it happened rather than as the last of
        // several hundred re-runs.
        progress("waiting for sshd in the test host to answer");
        self.guest.wait_for_ssh_ready(SSH_READY_TIMEOUT).await?;

        progress("copying the freshly built binaries into the test host");
        // scp rather than 9p: this guest runs Debian's stock `-cloud` kernel, which ships
        // no 9p modules at all, and giving it the generic kernel would diverge the kernel
        // under test from the one a real host runs.
        self.guest
            .copy_in(&binaries.all_paths(), GUEST_STAGE_DIR)
            .await?;

        // `./install` reads its inputs relative to `$(pwd)`, so the staged files are
        // arranged into the checkout shape it expects rather than the flat directory scp
        // delivered.
        progress("arranging the staging directory into the layout ./install expects");
        let binary_names = deployed_binaries().join(" ");
        self.guest
            .run_over_ssh(&format!(
                "set -e; cd {GUEST_STAGE_DIR}; \
                 mkdir -p target/release packages/tddy-daemon/apparmor; \
                 for b in {binary_names}; do \
                 mv -f \"$b\" target/release/\"$b\"; chmod 0755 target/release/\"$b\"; done; \
                 mv -f apparmor-tddy-daemon packages/tddy-daemon/apparmor/tddy-daemon; \
                 chmod 0755 install"
            ))
            .await?;

        progress("running ./install --systemd --headless in the guest");
        let install = self
            .guest
            .run_over_ssh_once(
                &format!("cd {GUEST_STAGE_DIR} && sudo ./install --systemd --headless"),
                INSTALL_TIMEOUT,
            )
            .await?;
        if install.exit_code() != 0 {
            return Err(anyhow!(
                "./install --systemd failed in the guest: {install}"
            ));
        }

        self.configure_session_token_secret(progress).await
    }

    /// Give the daemon a session-token secret and restart it.
    ///
    /// Appended after the install rather than written before it: the rendered config
    /// carries the `__INSTALL_*__` substitutions this testkit has no business
    /// reproducing, and the `livekit:` block is commented out in the template, so there is
    /// no key to collide with.
    async fn configure_session_token_secret(&self, progress: &(dyn Fn(&str) + Sync)) -> Result<()> {
        progress("configuring the session-token secret and restarting the supervisor");
        self.guest
            .run_over_ssh(&format!(
                "set -e; printf '\\nlivekit:\\n  url: \"ws://127.0.0.1:7880\"\\n  api_key: \
                 \"devkey\"\\n  api_secret: \"{SESSION_TOKEN_SECRET}\"\\n' | sudo tee -a \
                 {GUEST_DAEMON_CONFIG} > /dev/null; \
                 sudo systemctl restart tddy-supervisor.service"
            ))
            .await?;
        Ok(())
    }

    /// The guest, for tests that need to drive it directly.
    pub fn guest(&self) -> &BootedGuest {
        &self.guest
    }

    pub fn guest_mut(&mut self) -> &mut BootedGuest {
        &mut self.guest
    }

    /// The host port the guest's SSH is forwarded to — the channel an `ssh -L` tunnel to
    /// the daemon's socket rides over.
    pub fn ssh_host_port(&self) -> u16 {
        self.guest.ssh_host_port()
    }

    /// The session account whose sessions must run as itself.
    pub fn session_username(&self) -> &'static str {
        ALICE_USERNAME
    }

    /// The private key SSH reaches this guest with.
    pub fn ssh_private_key(&self) -> PathBuf {
        self.layout
            .library()
            .vm_dir(&self.vm_name)
            .join(format!("id_{}", self.vm_name))
    }

    /// Shut the guest down and delete its overlay.
    ///
    /// The overlay is removed rather than kept: it is one run's worth of mutated cgroup
    /// and systemd state, and reusing it would undermine the reason this VM is disposable.
    pub async fn shutdown(self) -> Result<()> {
        let (layout, vm_name) = (self.layout.clone(), self.vm_name.clone());
        self.guest.shutdown().await?;
        layout
            .library()
            .remove_vm(&vm_name)
            .map_err(|e| anyhow!("removing the disposable test host {vm_name}: {e}"))
    }
}
