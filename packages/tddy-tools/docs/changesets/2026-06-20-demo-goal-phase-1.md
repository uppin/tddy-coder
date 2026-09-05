# 2026-06-20 — **Demo goal Phase 1

**Type:** Feature

BuildrootPlugin + QemuPlugin registered** — `plugin_registry()` in `build_cli.rs` registers `BuildrootPlugin` and `QemuPlugin`; new Cargo.toml deps on `tddy-build-buildroot` and `tddy-build-qemu`; acceptance tests `buildroot_plugin_registered_in_cli_dry_run`, `qemu_disk_image_plugin_registered_in_cli_dry_run`, `buildroot_and_qemu_plugins_registered_in_cli_registry`. Feature [coder/demo-goal.md](../../../../docs/ft/coder/demo-goal.md); PR [#214](https://github.com/uppin/tddy-coder/pull/214). (tddy-tools)
