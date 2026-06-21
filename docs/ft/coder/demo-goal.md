# Demo Goal — QEMU Deploy, Verify, and Share

**Product Area**: Coder (TDD Workflow)
**Status**: Draft
**Updated**: 2026-06-20

## Summary

The TDD workflow includes an optional **demo** goal/step that fires after **green** when the user opted
in during the interview. Today the demo step is hollow: its system prompt just says "follow demo-plan.md,
run it, and summarize." This feature gives the step real deployment infrastructure: the built application
is deployed to a **QEMU VM** (provisioned via nix), verified with agent-generated tests, and the user
receives a **shareable link** — either a **port-forward** to a web app or (in a future cycle)
a **screen-share session** — surfaced in the tddy web UI and posted to a **Telegram channel**.

**The QEMU VM is launched from the UI** (Updated: 2026-06-20). The user initiates VM startup via a
"Launch Demo VM" action in the session view; the daemon owns the QEMU process lifecycle. The agent
(demo step) then deploys the app to the already-running VM, verifies it, sets up the port-forward,
and posts the share link. The daemon shuts down the VM when the user signals they're done or the
session ends.

The demo "recipe" (build target, deploy steps, verify command, port maps, mode) is persisted in the
session directory as part of `demo-plan.md`, so it can be edited and replayed without re-running the
full TDD workflow.

## Background

The TDD recipe already wires the `demo` goal end-to-end:
- Interview elicits demo participation (`run_optional_step_x`) and options (`demo_options`); these are
  persisted to `changeset.yaml`.
- The plan goal produces a `DemoPlan` artifact (written as `demo-plan.md`) with `demo_type`, `setup_instructions`,
  and verification steps.
- The `demo` state transitions (`DemoRunning` → `DemoComplete`) fire the `before_demo` / `after_demo` hooks.
- The `DemoOutput` parser already has `demo_type`, `summary`, `steps_completed`, and `verification`.

What's missing is the deployment substrate (QEMU VM lifecycle), the recipe model (structured, replayable fields),
the port-forward execution and URL generation, the Telegram link post, and the web UI demo-link surface.

Prior art for QEMU lifecycle (ported approach, not code): `~/Code/makers-lt` uses QEMU user-mode networking
(`-netdev user,id=net0,hostfwd=tcp::<h>-:<g>`) + slirp for rootless SSH and port forwarding, a serial-console
completion token for headless provisioning, and a VM descriptor for persist/recreate. The same approach is
adopted here for the nix-provided `qemu-system-x86_64`.

## Requirements

### VM Lifecycle (UI-driven) (Added: 2026-06-20)

The QEMU VM lifecycle is owned by the **daemon** and initiated from the **web UI**, not by the agent
automatically during the demo step. The flow is:

1. **UI action** — user clicks "Launch Demo VM" in the session view; the UI sends an RPC to the daemon.
2. **Daemon boots VM** — daemon calls `QemuDemoVm::boot(config)` using the recipe from `demo-plan.md`;
   keeps the QEMU process running, managing its PID/monitor socket.
3. **Agent deploys** — the demo step (agent) sees the VM is running, reads `demo-plan.md`, and executes
   deploy steps via SSH; verifies the app is healthy; opens the port-forward; posts the share link.
4. **User invokes the demo** — the user interacts with the app via the share URL; the VM stays running.
5. **Shutdown** — when the user closes the demo (UI action) or the session ends, the daemon shuts down
   the QEMU process via the monitor socket.

The `DemoOrchestrator::run()` receives an already-running `RunningVm` — it never calls `boot()` itself.
The daemon-side VM management (boot + shutdown) is a separate concern from the agent-side
deploy/verify/forward cycle.

### Demo Modes

1. **PortForward** — the app exposes an HTTP port inside the guest; the agent opens a hostfwd from a
   free host port to the guest port; the share link is `http://localhost:<host_port>`. This is the first
   implemented mode (this cycle).
2. **ScreenShare** — the app is a GUI or TUI; the agent streams the VNC framebuffer to a LiveKit room and
   produces a viewer URL. Modeled and typed this cycle, but execution is deferred to a follow-up cycle.

The **plan** step decides the mode: web-app with exposed HTTP port → `PortForward`; GUI/UX demo → `ScreenShare`.
The mode is written into the `DemoPlan` (and thereby into `demo-plan.md`).

### Recipe model (extends `DemoPlan`)

`DemoPlan` in `parser.rs` gains the following new fields (all `#[serde(default)]` for back-compat with
existing `demo-plan.md` files that don't have them):

| Field | Type | Description |
|---|---|---|
| `mode` | `Option<DemoMode>` | `PortForward` \| `ScreenShare` |
| `hostfwd` | `Vec<PortMap>` | Host ↔ guest port mappings |
| `deploy_steps` | `Vec<String>` | Shell commands to run in the guest after boot |
| `verify_command` | `Option<String>` | Command to assert the app is healthy |

`DemoMode` is a new enum: `PortForward` / `ScreenShare` (serde: `"port_forward"` / `"screen_share"`).
`PortMap` is a new struct: `host_port: u16`, `guest_port: u16`.

The `DemoOutput` parser output gains `share_url: Option<String>`.

### QEMU VM (new `tddy-demo-runner` crate)

A new package `packages/tddy-demo-runner` provides:

- **`DemoVm` trait** — `boot`, `deploy`, `verify`, `forward`, `shutdown`. Mockable boundary.
  - `boot` and `shutdown` are called by the **daemon** in response to UI actions.
  - `deploy`, `verify`, `forward` are called by the **agent** (demo step orchestrator) once the VM is running.
- **`QemuDemoVm`** concrete impl — builds `qemu-system-x86_64` args from `DemoVmConfig`:
  - Drive: `-drive file=<qcow2>,if=virtio,format=qcow2`
  - Network: `-netdev user,id=net0,hostfwd=tcp::<h>-:<g>` per `PortMap` (at minimum SSH: `tcp::2222-:22`)
  - VNC: `-vnc :<n>` (when `ScreenShare` mode; deferred)
  - Monitor socket: `-monitor unix:<path>,server,nowait` for graceful shutdown
  - Serial console to file for readiness detection
- **`DemoOrchestrator`** — takes `DemoVm + TelegramSender + DemoRecipe + RunningVm`; produces
  `DemoResult { share_url, steps_completed, verification }`. Does **not** call `boot()` — receives
  an already-running `RunningVm` from the caller (daemon). (Updated: 2026-06-20)
- **`MockDemoVm`** — records deploy/verify/forward/boot/shutdown calls; configurable return values.

The nix dev shell already provides `pkgs.qemu`. A new nix expression (`nix/demo-vm.nix`) builds a minimal
guest image (sshd, app runtime) using nixpkgs. Full cloud-init provisioning pipeline is deferred.

### Web UI "Launch Demo VM" action (Added: 2026-06-20)

The session view (`ConnectionScreen` / `SessionWorkflowStatusCells`) gains a **"Launch Demo VM"** button
that is shown when the session is in `DemoRunning` state. Clicking it sends a daemon RPC
(`/rpc StartDemoVm { session_id }`) which:
1. Loads the session's `demo-plan.md` recipe.
2. Calls `QemuDemoVm::boot(config)` and stores the `RunningVm` handle.
3. Returns a `DemoVmStatus { state: Booting | Running | Error }` response.

A corresponding **"Stop Demo VM"** button (or automatic shutdown on session end) sends
`/rpc StopDemoVm { session_id }` which calls `QemuDemoVm::shutdown(vm)`.

The agent's demo step detects the running VM (via daemon state or by querying the RPC) before deploying.

### Build plugin registration

`BuildrootPlugin` and `QemuPlugin` (from `tddy-build-buildroot` and `tddy-build-qemu`) must be registered
in the production plugin registries:
- `packages/tddy-tools/src/build_cli.rs` `plugin_registry()`
- `packages/tddy-coder/src/build_executor.rs` `plugin_registry()`

This unblocks `tddy-tools build --target <demo-vm:qcow2>`.

### Telegram notification

When the demo link is ready, call `TelegramSender::send_message(chat_id, link_text)` for every configured
`chat_id` in `TelegramConfig`. Reuse `send_daemon_lifecycle_message` pattern from
`packages/tddy-daemon/src/telegram_notifier.rs:276`.

### Web UI demo link

Surface `share_url` from `DemoOutput` as a "demo link" element in the session view (`ConnectionScreen` /
`SessionWorkflowStatusCells`) in `packages/tddy-web`. The element is only shown while the demo step is
active or the session has a `share_url` result.

### Demo system prompt

`packages/tddy-workflow-recipes/src/tdd/demo.rs` system prompt is updated to instruct the agent to:
1. Read `demo-plan.md` and load the recipe (mode, hostfwd, deploy_steps, verify_command)
2. Build the qcow2 via `tddy-tools build --target <build_target>` (from recipe)
3. **Wait for VM to be launched from the UI** — poll/query daemon until `DemoVmStatus == Running`;
   the agent does NOT boot the VM itself (Updated: 2026-06-20)
4. Run deploy steps via SSH to the already-running VM
5. Execute verify_command and assert success
6. For `PortForward`: confirm the forward is active and report `share_url`; post to Telegram
7. Submit `{"goal":"demo","summary":"...","demo_type":"port_forward","steps_completed":N,"verification":"...","share_url":"http://localhost:PORT"}`

## Acceptance Criteria (Testing Plan)

### Test level: Integration (mocked VM + Telegram boundaries)

| Test | Location |
|---|---|
| `demo_orchestrator_port_forward_deploys_verifies_and_posts_link` | `tddy-demo-runner/tests` |
| `demo_orchestrator_does_not_call_boot_it_receives_running_vm` | `tddy-demo-runner/tests` (Added: 2026-06-20) |
| `plan_step_decides_demo_mode_port_forward_for_web_app` | `tddy-workflow-recipes/tests` |
| `plan_step_decides_demo_mode_screen_share_for_gui_app` | `tddy-workflow-recipes/tests` |
| `demo_plan_recipe_roundtrips_through_demo_plan_md` | `tddy-workflow-recipes/tests` |
| `demo_plan_back_compat_existing_demo_plan_parses_without_mode` | `tddy-workflow-recipes/tests` |
| `buildroot_and_qemu_plugins_registered_in_cli_registry` | `tddy-tools/tests` |
| `demo_output_includes_share_url` | `tddy-workflow-recipes/tests` |
| `qemu_args_hostfwd_formats_correctly` | `tddy-demo-runner/tests` |
| `qemu_args_multiple_hostfwds_combined` | `tddy-demo-runner/tests` |
| `port_map_to_share_url_uses_host_port` | `tddy-demo-runner/tests` |

### Test level: Production (daemon-gated, `#[ignore]`)

| Test | Location |
|---|---|
| `real_qemu_demo_boots_deploys_and_forwards` | `tddy-demo-runner/tests` |

## Deferred to follow-up cycles

- Screen-share execution: VNC(guest) → LiveKit H264 bridge + viewer URL. Modeled this cycle.
- Full cloud-init provisioning pipeline + nix guest image library.
- Recipe re-run UX: edit `demo-plan.md` in the web UI, replay demo without re-running the workflow.
