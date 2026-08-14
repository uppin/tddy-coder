//! Where the testkit keeps its images, overlays and built binaries.
//!
//! Everything lands under the repo's own `tmp/.tddy` — gitignored, and the same dev data
//! directory `./web-dev` populates, so a developer has one place to look and one place to
//! delete.
//!
//! ```text
//! tmp/.tddy/
//!   images/01-base/<name>.qcow2              imported from $TDDY_CLOUDINIT_BASE_IMAGE
//!   images/02-prepared-base/
//!     tddy-nix-base.qcow2   (+ -base.qcow2)  Nix + flakes, tddy + alice; stock kernel
//!     tddy-builder.qcow2    (+ -base.qcow2)  from nix-base: + 9p kernel, + warm dev shell
//!     tddy-test-host.qcow2  (+ -base.qcow2)  from nix-base: + tddy-clients, stage dir
//!   vm/tddy-builder/                         long-lived: /opt/tddy/target persists
//!   vm/tddy-test-<pid>/                      disposable: one per test run
//!   dist/linux-<host arch>/                  binaries the builder guest writes back
//! ```

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tddy_vm::vm::VmArch;
use tddy_vm::VmLibrary;

/// The shared prepared base both images below derive from — Nix, flakes, and the two OS
/// accounts. Baked once; installing Nix is the expensive half of both child bakes.
pub const NIX_BASE_IMAGE_NAME: &str = "tddy-nix-base";

/// The prepared base that carries the build toolchain.
pub const BUILDER_IMAGE_NAME: &str = "tddy-builder";

/// The prepared base the cgroups assertions run against.
pub const TEST_HOST_IMAGE_NAME: &str = "tddy-test-host";

/// The `linux-<arch>` directory name binaries built for `arch` are collected under.
///
/// Spelled the way the Rust target triples and `uname -m` do, so a developer reading
/// `dist/linux-x86_64` recognises what is in it.
pub fn linux_platform_dir(arch: VmArch) -> &'static str {
    match arch {
        VmArch::Aarch64 => "linux-aarch64",
        VmArch::X86_64 => "linux-x86_64",
    }
}

/// Derive the repo root from a crate's `CARGO_MANIFEST_DIR`.
///
/// Two components up from `<repo>/packages/<pkg>`. This is why the testkit never calls
/// `tddy_core::output::default_tddy_data_dir()`: that returns the *relative* `tmp/.tddy`,
/// which `./web-dev` resolves correctly only because it `cd`s to the repo root first
/// (`web-dev:36`). `cargo test` runs with the CWD set to the package directory, so the
/// same relative path would scatter caches into `packages/<pkg>/tmp/.tddy`.
pub fn repo_root_from_manifest_dir(manifest_dir: &Path) -> PathBuf {
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest_dir)
        .to_path_buf()
}

/// The repo root this crate was compiled in, with the repo-root `.env` applied once.
///
/// `CARGO_MANIFEST_DIR` is baked in at compile time and always names *this* crate, so the
/// answer is the same whichever package's test binary is asking — and inside a git
/// worktree it is that worktree's root, so two worktrees never share one image cache.
pub fn repo_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = repo_root_from_manifest_dir(Path::new(env!("CARGO_MANIFEST_DIR")));
        crate::env_file::load_repo_env_file(&root);
        root
    })
}

/// The testkit's cache layout, rooted at a repo.
#[derive(Debug, Clone)]
pub struct TestkitLayout {
    repo_root: PathBuf,
}

impl TestkitLayout {
    /// The layout for the repo this crate was compiled in.
    pub fn for_this_repo() -> Self {
        Self::for_repo_root(repo_root().to_path_buf())
    }

    /// The layout for an explicit repo root — the seam the unit tests drive.
    pub fn for_repo_root(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    /// The repo this layout is rooted at.
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// The VM & Image Library root: the repo's own `tmp/.tddy`.
    pub fn library_root(&self) -> PathBuf {
        self.repo_root.join("tmp").join(".tddy")
    }

    /// The library the images live in.
    pub fn library(&self) -> VmLibrary {
        VmLibrary::new(self.library_root())
    }

    /// Where the builder guest writes the binaries it built.
    ///
    /// Named for the *guest* platform, not the host's: these are Linux ELF binaries produced
    /// in a VM precisely because a macOS host cannot produce them. The architecture is the
    /// host's own — the guest is accelerated, so it runs nothing else — which is why it is
    /// derived rather than written out.
    pub fn dist_dir(&self) -> PathBuf {
        self.dist_dir_for(VmArch::host())
    }

    /// Where binaries built for `arch` are collected — the seam the unit tests drive.
    pub fn dist_dir_for(&self, arch: VmArch) -> PathBuf {
        self.library_root()
            .join("dist")
            .join(linux_platform_dir(arch))
    }

    /// The builder guest's name — stable across runs, so its overlay (and with it
    /// `/opt/tddy/target`) survives and the next `./release` is incremental rather than a
    /// cold rebuild costing hours.
    pub fn builder_vm_name(&self) -> String {
        BUILDER_IMAGE_NAME.to_string()
    }

    /// The path a prepared base of `name` occupies once baked.
    ///
    /// Its existence is the cache key: a bake that already produced this file is never
    /// repeated, which is what turns a multi-hour chain into a one-time cost.
    pub fn prepared_base_path(&self, name: &str) -> PathBuf {
        self.library()
            .prepared_base_dir()
            .join(format!("{name}.qcow2"))
    }

    /// A disposable guest name for one test run.
    ///
    /// Distinct per run so every run gets a fresh overlay off the prepared base: the
    /// cgroup state these tests assert on must never be inherited from the previous run.
    pub fn test_host_vm_name(&self, run_id: u32) -> String {
        format!("tddy-test-{run_id}")
    }
}
