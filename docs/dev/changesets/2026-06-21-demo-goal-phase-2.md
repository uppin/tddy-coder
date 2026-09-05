# 2026-06-21 — **Demo goal Phase 2

**Type:** Feature

QemuDemoVm SSH impl, daemon VM lifecycle RPCs, web UI** — `tddy-demo-runner`: full `QemuDemoVm` impl (boot/deploy/verify/forward/shutdown via SSH + QEMU monitor), `wait_for_ssh_port`/`send_monitor_command` helpers, per-instance monitor socket path, unit tests; `tddy-daemon`: `DemoVmHandle` state, `StartDemoVm`/`StopDemoVm`/`GetDemoVmStatus` RPCs; `tddy-service`: new RPCs + `DemoVmState` enum + `share_url` on status response, TypeScript codegen; `tddy-web`: `DemoVmControls` component (Launch/Stop/Retry + share URL) wired into `ConnectionScreen`; `tddy-workflow-recipes`: demo system prompt updated with 7-step QEMU deploy flow. Feature [coder/demo-goal.md](../../ft/coder/demo-goal.md). (tddy-demo-runner, tddy-daemon, tddy-service, tddy-web, tddy-workflow-recipes)
