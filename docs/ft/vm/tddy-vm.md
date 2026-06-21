# tddy-vm: General-purpose VM build and lifecycle management

**Product area:** VM
**Updated:** 2026-06-21
**Status:** In development

## Summary

`tddy-vm` is a new package that provides a **general-purpose VM build and lifecycle capability** for the tddy-coder workspace. It decouples VM management from the "demo goal" concept and exposes it as a first-class, session-independent capability via:

- A **standalone VM registry** — define VMs by name (with image path or build target), list them, start/stop/remove them independently of any session or demo-plan.md.
- A **build wrapper** — invoke the existing `tddy-build-qemu` / `tddy-build-buildroot` plugins to produce qcow2 images from BUILD.yaml targets.
- An **RPC service** (`vm.VmService`) exposed over the daemon's HTTP and LiveKit transports, discoverable via gRPC reflection.
- A **`/vms` page** in the web dashboard — list, start, stop, and remove VMs from the UI.

## Relationship to the demo goal

The existing demo-goal infrastructure (`tddy-demo-runner`, `StartDemoVm`/`StopDemoVm` RPCs) continues to work unchanged externally but is now a thin layer over `tddy-vm`. The `DemoOrchestrator` uses `tddy_vm::Vm` (the generalised trait) instead of its own `DemoVm` copy. The daemon's session-scoped `StartDemoVm` RPC also uses `tddy_vm::QemuVm` for the actual QEMU process.

This means there is a single source of truth for VM lifecycle logic, and the demo goal gains robustness from improvements to the general VM layer.

## VM Registry

VMs are defined with a `VmSpec`:

| Field | Description |
|-------|-------------|
| `name` | Unique identifier |
| `build_target` | Optional — BUILD.yaml target ID to build the qcow2 image |
| `image_path` | Optional — path to an existing qcow2 image (mutually exclusive with `build_target`) |
| `port_forwards` | `Vec<PortForward { host_port, guest_port }>` — slirp hostfwd mappings beyond SSH |
| `ssh_host_port` | Host-side SSH port (default 2222; must be unique across running VMs) |

Specs are persisted to a JSON file in the daemon's data directory so definitions survive daemon restarts.

`VmState` lifecycle: `Defined → Booting → Running → Stopped` (or `Error`).

## RPC surface

New `vm.VmService` with 7 methods:

| Method | Description |
|--------|-------------|
| `BuildVmImage` | Invoke tddy-build system to produce a qcow2 from a build target |
| `DefineVm` | Register a VM spec in the registry |
| `ListVms` | List all VMs with their current state |
| `StartVm` | Boot a named VM (builds image first if `build_target` set) |
| `StopVm` | Graceful shutdown via QEMU monitor `system_powerdown` |
| `GetVmStatus` | Current state + SSH port + share URL |
| `RemoveVm` | Remove a stopped VM from the registry |

All methods require `session_token` for authentication (same pattern as `ConnectionService`).

## Web UI

A `/vms` page in the dashboard (accessed via the hamburger nav menu). (Updated: 2026-06-21)

### Build image panel

A two-step panel for building a disk image and creating a VM:

1. **Buildroot spec textarea** — the user pastes or types a Buildroot configuration fragment (e.g. `BR2_x86_64=y`, `BR2_TARGET_ROOTFS_EXT2=y`). Clicking **Build image** sends the spec to the `BuildVmImage` RPC. A "Building…" indicator shows while the build is in progress. Errors surface inline below the textarea.

2. **Create VM form** — once an image is available, a **dropdown** lists all successfully built image paths. The user selects an image, enters a VM name, and clicks **Create VM** to call `DefineVm`. The dropdown accumulates images across multiple builds in the same session.

### VM table

A table of all defined VMs with their state, SSH host port, share URL, and **Start / Stop / Remove** action buttons. The table refreshes after every mutating action (define, start, stop, remove).

## Architecture

```
tddy-vm (new)
├── vm.rs         — Vm trait (mockable boundary), VmConfig, RunningVm, VmError, PortForward
├── qemu.rs       — QemuVm (full impl), QemuVmArgs (pure arg builder), wait_for_ssh_port, send_monitor_command
├── mock.rs       — MockVm (recording test double)
├── build.rs      — build_vm_image() — wraps tddy-build-qemu/buildroot plugins
├── registry.rs   — VmSpec, VmState, VmManager (HashMap + JSON persistence)
└── service.rs    — VmServiceImpl (implements generated vm::VmService trait)

tddy-service
└── proto/vm.proto — VmService definition → generates Rust + TypeScript clients

tddy-demo-runner (refactored)
└── orchestrator.rs — DemoOrchestrator uses tddy_vm::Vm; vm.rs/qemu.rs/mock.rs deleted

tddy-web
└── src/components/vms/
    ├── VmsAppPage.tsx   — container: RPC wiring, state (building, availableImages)
    ├── DefineVmPanel.tsx — presentational: spec textarea + image dropdown + Create form
    └── VmsScreen.tsx    — presentational: VM table with Start/Stop/Remove
```

## Requirements

1. `tddy-vm` package compiles cleanly with stub implementations.
2. `vm.proto` defines all 7 RPCs with correct message types.
3. `VmServiceServer` is registered in `tddy-daemon` and appears in gRPC reflection.
4. `tddy-demo-runner` has no duplicated VM lifecycle code.
5. `/vms` page renders in the web app and appears in the nav menu.
6. `VmManager` persists specs to JSON; serde round-trip is correct.
7. `build_vm_image` invokes the tddy-build system (QemuPlugin + BuildrootPlugin registry).

## Out of scope for this changeset

- `VmManager` methods fully implemented (deferred to /green phase).
- `build_vm_image` fully implemented (deferred to /green phase).
- `VmsAppPage` wired to live RPC (deferred to /green phase).
- ScreenShare VM mode.
- Multi-host VM management.
