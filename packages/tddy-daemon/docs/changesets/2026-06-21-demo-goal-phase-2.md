# 2026-06-21 — **Demo goal Phase 2

**Type:** Feature

daemon VM lifecycle RPCs** — `DemoVmHandle` enum (`Booting`, `Running { vm: RunningVm, share_url }`, `Error(String)`) as per-session state; `demo_vm_state: Arc<Mutex<HashMap<String, DemoVmHandle>>>` field in `ConnectionServiceImpl`; `StartDemoVm` reads session's `demo-plan.md`, builds `DemoVmConfig`, spawns `boot()` background task; `StopDemoVm` removes handle and calls `shutdown()`; `GetDemoVmStatus` returns `DemoVmState + ssh_host_port + share_url`; `tddy-demo-runner` dep added. Feature [coder/demo-goal.md](../../../../docs/ft/coder/demo-goal.md). (tddy-daemon)
