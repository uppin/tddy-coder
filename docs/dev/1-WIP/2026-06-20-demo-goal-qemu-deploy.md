# Demo goal — QEMU deploy, verify, and share (port-forward path)

## Changeset summary

**Date**: 2026-06-20
**Feature**: Demo goal — QEMU deploy, verify, port-forward, Telegram link, web UI surface
**PRD**: [docs/ft/coder/demo-goal.md](../../ft/coder/demo-goal.md)

> **Requirements change (2026-06-20):** The QEMU VM is launched from the UI, not by the agent during
> the demo step. The daemon owns the QEMU process lifecycle (boot/shutdown), triggered by UI RPC actions.
> The `DemoOrchestrator::run()` receives an already-running `RunningVm` — it no longer calls `boot()`.
> New TODO items added below reflect the UI/daemon surface.

## TODO

### Phase 1 — Data model + orchestration architecture (this PR) ✅

- [x] Create/update PRD documentation (`docs/ft/coder/demo-goal.md`)
- [x] Create changeset
- [x] Extend `DemoPlan` with `mode: Option<DemoMode>`, `hostfwd: Vec<PortMap>`, `deploy_steps`, `verify_command`
- [x] Add `DemoMode` enum and `PortMap` struct to `tddy-workflow-recipes/src/parser.rs`
- [x] Add `share_url: Option<String>` to `DemoOutput` in `parser.rs`
- [x] Update `demo.schema.json` to accept `share_url`
- [x] Add `read_demo_plan_file` to `writer.rs`
- [x] Create new crate `packages/tddy-demo-runner` with `DemoVm` trait, `QemuDemoVm` (stubs), `MockDemoVm`, `DemoOrchestrator`
- [x] Register `BuildrootPlugin` + `QemuPlugin` in `tddy-tools/src/build_cli.rs` `plugin_registry()`
- [x] Implement `DemoOrchestrator::run(recipe, vm: RunningVm)` — receives running VM, no `boot()` call
- [x] Wire Telegram link post in demo orchestrator
- [x] Write failing acceptance tests (red phase)
- [x] Write failing test `demo_orchestrator_does_not_call_boot_it_receives_running_vm`
- [x] Implement passing production code (green phase)

### Phase 2 — Concrete QEMU + daemon + UI wiring (future PR)

- [ ] Register `BuildrootPlugin` + `QemuPlugin` in `tddy-coder/src/build_executor.rs` `plugin_registry()`
- [ ] Create nix guest image expression (`nix/demo-vm.nix`)
- [ ] Update demo system prompt (`tdd/demo.rs`) — agent waits for VM from UI, does not boot itself
- [ ] Implement `QemuDemoVm` (boot via `qemu-system-x86_64`, deploy via SSH, verify, forward, shutdown)
- [ ] Add daemon RPC endpoints `StartDemoVm` / `StopDemoVm`
- [ ] Add "Launch Demo VM" + "Stop Demo VM" UI actions to session view
- [ ] Add `DemoVmStatus` (Booting | Running | Stopped | Error) to daemon state + propagate to UI
- [ ] Add "demo link" surface to `tddy-web` session view (shown when VM is Running + share_url available)

## Acceptance tests created

| Test | File | Status |
|---|---|---|
| `demo_orchestrator_port_forward_deploys_verifies_and_posts_link` | `tddy-demo-runner/tests/demo_orchestrator_acceptance.rs` | 🟢 passing |
| `demo_orchestrator_does_not_call_boot_it_receives_running_vm` | `tddy-demo-runner/tests/demo_orchestrator_acceptance.rs` | 🟢 passing |
| `demo_orchestrator_errors_when_mode_is_missing` | `tddy-demo-runner/tests/demo_orchestrator_acceptance.rs` | 🟢 passing |
| `demo_orchestrator_errors_when_verify_fails` | `tddy-demo-runner/tests/demo_orchestrator_acceptance.rs` | 🟢 passing |
| `plan_step_decides_demo_mode_port_forward_for_web_app` | `tddy-workflow-recipes/tests/demo_plan_recipe_unit.rs` | 🟢 passing |
| `plan_step_decides_demo_mode_screen_share_for_gui_app` | `tddy-workflow-recipes/tests/demo_plan_recipe_unit.rs` | 🟢 passing |
| `demo_plan_recipe_roundtrips_through_demo_plan_md` | `tddy-workflow-recipes/tests/demo_plan_recipe_unit.rs` | 🟢 passing |
| `demo_plan_back_compat_existing_demo_plan_parses_without_mode` | `tddy-workflow-recipes/tests/demo_plan_recipe_unit.rs` | 🟢 passing |
| `demo_output_includes_share_url` | `tddy-workflow-recipes/tests/demo_plan_recipe_unit.rs` | 🟢 passing |
| `buildroot_plugin_registered_in_cli_dry_run` | `tddy-tools/tests/demo_build_plugin_acceptance.rs` | 🟢 passing |
| `qemu_disk_image_plugin_registered_in_cli_dry_run` | `tddy-tools/tests/demo_build_plugin_acceptance.rs` | 🟢 passing |
| `buildroot_and_qemu_plugins_registered_in_cli_registry` | `tddy-tools/tests/demo_build_plugin_acceptance.rs` | 🟢 passing |
| `qemu_args_hostfwd_formats_correctly` | `tddy-demo-runner/tests/qemu_args_unit.rs` | 🟢 passing |
| `qemu_args_app_hostfwd_formats_correctly` | `tddy-demo-runner/tests/qemu_args_unit.rs` | 🟢 passing |
| `qemu_args_netdev_includes_ssh_forward` | `tddy-demo-runner/tests/qemu_args_unit.rs` | 🟢 passing |
| `qemu_args_multiple_hostfwds_combined` | `tddy-demo-runner/tests/qemu_args_unit.rs` | 🟢 passing |
| `qemu_args_full_argv_has_required_elements` | `tddy-demo-runner/tests/qemu_args_unit.rs` | 🟢 passing |
| `port_map_to_share_url_uses_host_port` | `tddy-demo-runner/tests/qemu_args_unit.rs` | 🟢 passing |

## Packages touched

- `tddy-workflow-recipes` — `parser.rs`, `writer.rs`, `src/tdd/demo.rs`, `generated/tdd/demo.schema.json`
- `tddy-demo-runner` (new crate) — `DemoVm`, `QemuDemoVm`, `MockDemoVm`, `DemoOrchestrator`
- `tddy-tools` — `src/build_cli.rs` plugin_registry
- `tddy-coder` — `src/build_executor.rs` plugin_registry
- `tddy-web` — "Launch Demo VM" / "Stop Demo VM" actions + demo link surface in session view (Updated: 2026-06-20)
- `tddy-daemon` — `StartDemoVm` / `StopDemoVm` RPC endpoints + QEMU process lifecycle + Telegram send trigger (Updated: 2026-06-20)
- `flake.nix` — nix guest image output
