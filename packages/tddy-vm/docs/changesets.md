# Changesets Applied

Wrapped changeset history for tddy-vm.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-21** [Feature] **tddy-vm — general-purpose VM build and lifecycle management** — new crate: `Vm` trait, `QemuVm` (QEMU process lifecycle), `MockVm` (test double), `QemuVmArgs` (pure arg builder), `VmManager` (JSON-persisted define/list/start/stop/status/remove), `VmSpec`/`VmState`; `build.rs`: `VmImageRecord`, `built_images_dir()`, `list_built_images_in()`, `list_built_images()` (directory scan of `tmp/buildroot/disks/**/images/*.qcow2`), `build_vm_image_from_spec` (Buildroot streaming build with concurrent stdout/stderr drain, STAGE_DONE delivers qcow2 path); `service.rs`: `SessionUserResolver` type (local, avoids circular dep), `VmServiceImpl` (8 RPCs, all token-validated via `authenticate()`). Tests: `vm_registry_acceptance`, `build_image_acceptance`, `list_vm_images_unit` (6 tests), `qemu_args_unit`, `registry_unit`. Feature [vm/tddy-vm.md](../../../docs/ft/vm/tddy-vm.md). (tddy-vm)
