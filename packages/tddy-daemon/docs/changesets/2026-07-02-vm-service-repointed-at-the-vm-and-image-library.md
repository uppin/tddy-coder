# 2026-07-02 — VM service repointed at the VM & Image Library

**Type:** Feature

`main.rs:437-459`: VM lifecycle service construction now builds a `tddy_vm::VmLibrary` at the same resolved data root every other per-user store here uses (`user_sessions_path::tddy_data_root_matching_child`), logs (does not crash) on `init()` failure, and constructs `VmManager::from_library` instead of the old `vm-registry.json`-backed `VmManager::new`. Additive at the type level (`VmManager::new` still exists); the daemon's own VM service is the only call site moved. Feature [vm/tddy-vm.md](../../../../docs/ft/vm/tddy-vm.md) § VM & Image Library; cross-package [changesets/](../../../../docs/dev/changesets/). (tddy-daemon, tddy-vm)
