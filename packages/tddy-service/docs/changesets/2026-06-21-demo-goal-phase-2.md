# 2026-06-21 — **Demo goal Phase 2

**Type:** Feature

proto additions** — `connection.proto`: `StartDemoVm`/`StopDemoVm`/`GetDemoVmStatus` RPCs; `DemoVmState` enum (`UNKNOWN=0`, `BOOTING=1`, `RUNNING=2`, `STOPPED=3`, `ERROR=4`); `GetDemoVmStatusResponse` with `state`, `ssh_host_port`, `message`, `share_url`; TypeScript codegen regenerated. Feature [coder/demo-goal.md](../../../../docs/ft/coder/demo-goal.md). (tddy-service)
