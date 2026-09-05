# 2026-06-20 — **tddy-build

**Type:** Feature

buildroot_image + qemu_disk_image plugins** — new `tddy-build-buildroot`: `buildroot_image` lowers to two `make` actions (defconfig → build) with an explicit `.config` intermediate that wires sequencing via `action_overlap_edges`; `O=` computed relative to `buildroot_dir` so `make` writes to the correct repo-root-relative location. New `tddy-build-qemu`: `qemu_disk_image` lowers to `qemu-img convert -f <fmt> -O qcow2`; output path inferred by swapping input extension. Both ship example `BUILD.yaml` + real-tooling integration tests. `pkgs.qemu` + `pkgs.gnumake` added to Nix dev shell. PR [#212](https://github.com/uppin/tddy-coder/pull/212); architecture [tddy-build/architecture.md](../../../packages/tddy-build/docs/architecture.md). (tddy-build-buildroot, tddy-build-qemu, flake.nix)
