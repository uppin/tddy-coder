# 2026-06-20 — **tddy-build

**Type:** Feature

buildroot_image + qemu_disk_image plugins** — new crates `tddy-build-buildroot` (lowers to two `make` actions: defconfig then build; explicit `.config` intermediate wires sequencing; `O=` expressed relative to `buildroot_dir` so `make` writes to the repo-root-relative `output_dir`) and `tddy-build-qemu` (lowers to `qemu-img convert -f raw -O qcow2`; output inferred by swapping input extension to `.qcow2`). `pkgs.qemu` + `pkgs.gnumake` added to Nix dev shell. Architecture: [architecture.md](../architecture.md). (tddy-build-buildroot, tddy-build-qemu, flake.nix)
