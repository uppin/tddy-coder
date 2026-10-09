# 2026-10-09 — The shared LSP registry launches whatever `rust-analyzer` is on `PATH`, unpinned

**Category:** Future enhancement (launch policy; needs a developer decision)
**Source:** #reshape 12/19 (`feature/reshape/anchors-outline`), discovery Exploration 2

There are two ways rust-analyzer gets started, and they disagree:

- `RustBackend::start` (`packages/tddy-code-restructuring/src/backends/rust.rs:752-793`) refuses to
  start unpinned. It reads the default toolchain from `RUSTUP_HOME`, then sets `RUSTUP_TOOLCHAIN`,
  `CARGO` and `RUSTC` to the real binaries. Its comment explains why: through the rustup proxy, a
  sysroot discovery channel-syncs the repo's `rust-toolchain.toml` overlay over the network (a
  candidate for a 600 s stall).
- Every front end that uses the shared registry (the cold `tddy-tools restructure` command line, the
  index daemon and the test harness) launches `LaunchSpec::new("rust-analyzer")`
  (`restructure_cli.rs:257-266`, `tests/harness/mod.rs:540-547`). That is a bare name looked up on
  `PATH`, with none of that pinning. Outside the nix dev shell it is either missing or the rustup
  proxy.

#reshape 12 makes a missing binary *say so* (`language server not found: rust-analyzer: …`), but it
deliberately changes nothing about which binary is launched.

What would close it is one launch policy for both paths. That means either pinning in the registry's
`LaunchSpec`, the way `RustBackend::start` does, or a configured absolute path. Any lookup that tries
one binary and then another is a fallback and needs the developer's consent.

**Why deferred.** It is a launch-policy decision with environment consequences (jails, the desktop
app, CI). #reshape 12's boundary is "name the failure, change no launch".
