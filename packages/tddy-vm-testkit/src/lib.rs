//! Reusable VM fixtures for the production tests that need a real Linux kernel.
//!
//! **Test support only — never a dependency of production code.** This crate is
//! `publish = false` and belongs in a `[dev-dependencies]` entry, nowhere else. It hands out
//! fixed credentials on purpose — [`recipes::GUEST_PASSWORD`] and
//! [`test_host_vm::SESSION_TOKEN_SECRET`] are baked into throwaway guests so a bake that
//! fails halfway can be logged into and diagnosed — and every one of them is a real secret
//! the moment something shipped links against it.
//!
//! The workspace's cgroups code is the least-tested code it has, and it is least-tested
//! exactly where its claims are strongest: on macOS `tddy-sandbox-cgroups` compiles to an
//! empty lib, and on Linux every function that touches `/sys/fs/cgroup` is either never
//! executed or exercised against a `tempfile::tempdir()` standing in for cgroupfs. A
//! temp directory accepts any bytes, never enforces a limit, and returns `ENOTEMPTY`
//! forever where the kernel returns `EBUSY` — so the retry path and the success path of
//! scope removal have never run.
//!
//! This testkit closes that by putting the real thing in a guest:
//!
//! - [`builder_vm`] bakes a guest carrying Nix and the Rust/Bun toolchain, mounts the
//!   working copy read-only over 9p, runs `./release`, and writes the resulting
//!   Linux/aarch64 binaries back to the host over a **writable** 9p share. This is not an
//!   optimisation — a macOS host cannot produce those binaries at all.
//! - [`test_host_vm`] boots a lean guest off the *same* base image, `scp`s those binaries
//!   in, and runs `./install --systemd`, so the assertions run against a real
//!   supervisor + daemon on a real cgroup v2 hierarchy.
//!
//! Both prepared bases are cached under the repo's `tmp/.tddy` ([`layout`]), so the
//! expensive bake happens once and re-testing a code change is a boot plus an scp.
//!
//! No image is ever downloaded. The base image is supplied on disk through
//! [`env_file::BASE_IMAGE_ENV`] — the same knob `tddy-vm-build cloud-init --base-image`
//! reads — set in the environment or in the repo-root `.env`.

pub mod bake;
pub mod builder_vm;
pub mod env_file;
pub mod guest;
pub mod layout;
pub mod recipes;
pub mod test_host_vm;

pub use builder_vm::{BuilderVm, BuiltBinaries};
pub use env_file::{configured_base_image, BASE_IMAGE_ENV};
pub use guest::{BootedGuest, GuestCommandOutput};
pub use layout::{
    linux_platform_dir, TestkitLayout, BUILDER_IMAGE_NAME, NIX_BASE_IMAGE_NAME,
    TEST_HOST_IMAGE_NAME,
};
pub use test_host_vm::{TestHostVm, SESSION_TOKEN_SECRET};

/// Re-exported because the testkit's own API is expressed in it: a caller asking which
/// architecture a guest was built for should not have to depend on `tddy-vm` directly.
pub use tddy_vm::vm::VmArch;
