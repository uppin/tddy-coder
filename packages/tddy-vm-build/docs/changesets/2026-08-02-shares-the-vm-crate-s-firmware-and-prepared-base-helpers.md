# 2026-08-02 — Shares the VM crate's firmware and prepared-base helpers instead of duplicating them

**Type:** Refactor

`uefi_firmware_for` moved into `tddy-vm` (where the daemon's bake and `VmManager::start` also need it) and `promote_prepared_base_pair` was extracted from `run_cloud_init_build`, so the scratch-then-move-then-seal behaviour has one definition rather than two that must be kept in step by hand; the CLI now calls both. `CloudInitBuildOptions` gained `arch`/`accel`/`firmware`/`nine_p_shares`, populated here via the shared helper — firmware on aarch64 (mandatory, `virt` has no BIOS), `None` on x86_64, preserving the existing SeaBIOS boot. Net −25 lines, no behaviour change. Known gap: unlike the daemon's bake, this CLI still leaves its scratch directory in place by design. Feature [vm/tddy-vm.md](../../../../docs/ft/vm/tddy-vm.md) § Daemon-spawned tddy host VM. (tddy-vm-build)
