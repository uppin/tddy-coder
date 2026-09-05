# 2026-07-02 — **tddy-vm-build

**Type:** Feature

image builder CLI (new crate)** — `BuildImageArgs`/`run_build_image`: `tddy-vm-build --spec <path> --output <path> --format qcow2|raw`, calling `tddy_vm::build::build_image` (the same core shared with the daemon's `BuildVmImage` RPC). Real, non-mocked acceptance tests (`#[ignore]`+`#[serial(buildroot_docker_vm)]`, run via `cargo test -p tddy-vm-build --test build_image_cli_acceptance -- --ignored --nocapture`) pass against a real Buildroot build on macOS through the Docker toolchain in `tddy-vm`. Feature [vm/tddy-vm.md](../../../../docs/ft/vm/tddy-vm.md) § Image builder CLI; cross-package [changesets/](../../../../docs/dev/changesets/). (tddy-vm-build, tddy-vm)
