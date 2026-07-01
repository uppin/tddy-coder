# Changesets Applied

Wrapped changeset history for tddy-vm-build.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-02** [Feature] **tddy-vm-build — image builder CLI (new crate)** — `BuildImageArgs`/`run_build_image`: `tddy-vm-build --spec <path> --output <path> --format qcow2|raw`, calling `tddy_vm::build::build_image` (the same core shared with the daemon's `BuildVmImage` RPC). Real, non-mocked acceptance tests (`#[ignore]`+`#[serial(buildroot_docker_vm)]`, run via `cargo test -p tddy-vm-build --test build_image_cli_acceptance -- --ignored --nocapture`) pass against a real Buildroot build on macOS through the Docker toolchain in `tddy-vm`. Feature [vm/tddy-vm.md](../../../docs/ft/vm/tddy-vm.md) § Image builder CLI; cross-package [changesets.md](../../../docs/dev/changesets.md). (tddy-vm-build, tddy-vm)
