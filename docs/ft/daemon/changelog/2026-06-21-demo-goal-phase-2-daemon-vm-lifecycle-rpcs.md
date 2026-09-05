# 2026-06-21 — Demo goal Phase 2: daemon VM lifecycle RPCs

- `StartDemoVm` RPC: reads session's `demo-plan.md`, builds `DemoVmConfig`, spawns `QemuDemoVm::boot()` background task, tracks handle per session
- `StopDemoVm` RPC: removes handle and calls `shutdown()` via monitor socket
- `GetDemoVmStatus` RPC: returns `DemoVmState` (`BOOTING`/`RUNNING`/`STOPPED`/`ERROR`), `ssh_host_port`, and `share_url`
- Feature: [coder/demo-goal.md](../../coder/demo-goal.md). Cross-package: [docs/dev/changesets/](../../../dev/changesets/).
