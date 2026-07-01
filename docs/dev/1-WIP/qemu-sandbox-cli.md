# Changeset: qemu-sandbox-cli — VM image builder + QEMU sandbox backend binaries

**Date:** 2026-07-01
**Branch:** `slow-fragment`
**Packages:** `tddy-vm`, `tddy-vm-build` (new), `tddy-sandbox-qemu` (new), `tddy-daemon`
**Feature PRD:** [docs/ft/vm/tddy-vm.md § Image builder CLI](../../ft/vm/tddy-vm.md#image-builder-cli-tddy-vm-build), [§ QEMU sandbox backend](../../ft/vm/tddy-vm.md#qemu-sandbox-backend-tddy-sandbox-qemu)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Implement `build_image(spec, output, format, progress)` in `tddy-vm/src/build.rs` for real; refactor `build_vm_image_from_spec` to call it (shared `run_buildroot_pipeline` helper)
- [x] Implement `tddy-vm-build` CLI (spec → image file)
- [x] Implement `tddy-sandbox-qemu` argv builders for real (`qemu_sandbox_argv`, `ninep_fsdev_args`, `overlay_create_argv`, `guest_plan_json`)
- [x] Implement `tddy-sandbox-qemu::spawn_plan` / `spawn_plan_with` — real overlay creation + real `qemu-system-x86_64` boot; guest-runner handshake intentionally left open (see below)
- [x] `tddy-sandbox-qemu` CLI (mount/env/cwd flags → `SandboxPlan` → `spawn_plan_with`)
- [ ] Connect to the in-guest `tddy-sandbox-runner` over the forwarded TCP control port and stream the guest command's output (blocked on the gRPC transport-mismatch open design point below)
- [ ] Cross-compile `tddy-sandbox-runner` for `x86_64-unknown-linux-musl`
- [ ] Document Buildroot 9p + init-hook fragment for guest images
- [ ] Wire QEMU backend selector into `tddy-daemon` (`sandbox_session.rs`, `sandbox_action.rs`)

## Acceptance tests

- [x] `packages/tddy-vm-build/tests/build_image_cli_acceptance.rs` — real code path exercised; both tests still FAIL on this dev box because Buildroot rejects macOS's `/usr/bin/gcc` (Apple clang trampoline) — a host-platform limitation, not a code defect. Expected to pass on a Linux build host with a real `gcc`.
- [x] `packages/tddy-sandbox-qemu/tests/sandbox_qemu_cli_acceptance.rs` — real code path exercised; both tests still FAIL because the fixture uses a placeholder (non-image) `--image` file, so `qemu-img create` genuinely rejects it (`"Image is not in qcow2 format"`) before QEMU ever boots. Expected to get further (and ultimately pass) once a real runner-capable guest image exists and the transport-mismatch design point is resolved.

## Unit tests

- [x] `packages/tddy-sandbox-qemu/tests/argv_unit.rs` — `qemu_sandbox_argv`, `ninep_fsdev_args`, `overlay_create_argv`, `guest_plan_json` (9 tests, all PASSING)

Deferred (not red-worthy yet — see rationale below):
- `parse_mount_spec` / `parse_env_vars` (`packages/tddy-sandbox-qemu/src/cli.rs`) are already correctly implemented (real CLI-parsing code, not stubbed); dedicated unit tests would pass immediately and add no red signal. Covered indirectly by the CLI acceptance tests.
- `build_image` format selection (`packages/tddy-vm/src/build.rs`) is a single unconditional stub (`Err(NotImplemented)` regardless of input) — a unit test asserting today's behavior would trivially pass, and a test asserting future behavior duplicates the two acceptance tests already covering both formats. Revisit once the real implementation exists.

## Open design points (tracked in PRD "Known gaps")

- gRPC transport: `SandboxHandle.grpc_socket_path` is a UDS; QEMU guest is reachable only over TCP hostfwd. Needs a bridge or a TCP-connecting runner client.
- No uid/user field on `SandboxPlan`; guest runs as root (matches existing `QemuVm` SSH-as-root). Uid mapping deferred.
- Existing Buildroot images lack 9p support; must be rebuilt with the new fragment before this backend can boot them.

## Delta summary

### `tddy-vm`

**Modified files:**
- `src/build.rs` — `ImageFormat` enum; `build_image(spec, output, format, progress)` runs the real Buildroot pipeline (private tempdir build tree, `make olddefconfig`, `make -j<nproc>`, then `qemu-img convert` or a raw copy) via a new shared `run_buildroot_pipeline` helper. `build_vm_image_from_spec` refactored to call the same helper — gRPC stage messages, cancellation (`SIGINT`), and the `tmp/buildroot/disks/build-<ts>` layout are unchanged (all 26 pre-existing `tddy-vm` tests still pass).
- `Cargo.toml` — `tempfile` promoted from dev-dependency to a regular dependency (used by `build_image`'s private build tree).
- `src/lib.rs` — re-export `ImageFormat`, `build_image`.

### `tddy-vm-build` (new)

- `Cargo.toml`, `src/lib.rs` (`BuildImageArgs`, `run_build_image`), `src/main.rs`.
- `tests/build_image_cli_acceptance.rs` — real code path; fails on this dev box only due to the macOS/gcc limitation noted above.

### `tddy-sandbox-qemu` (new)

- `Cargo.toml`, `src/lib.rs`, `src/spawn.rs` (`spawn_plan`, `spawn_plan_with` — real overlay + boot; `QemuBackendOptions`), `src/argv.rs` (`qemu_sandbox_argv`, `ninep_fsdev_args`, `overlay_create_argv`, `guest_plan_json`, `plan_config_dir`, `monitor_socket_path` — all real, pure, fully unit-tested), `src/cli.rs` (`SandboxQemuArgs`, `parse_mount_spec`, `parse_env_vars`), `src/main.rs`.
- `tests/sandbox_qemu_cli_acceptance.rs`, `tests/argv_unit.rs`.
- `run_sandbox_qemu` deliberately reports an error (not a fake `0`) when `spawn_plan_with` succeeds but the guest command was never actually executed — see "Open design points".

### Root `Cargo.toml`

- Added `packages/tddy-vm-build` and `packages/tddy-sandbox-qemu` to `[workspace] members`.
