# 2026-06-20 — Demo goal: QEMU deploy architecture (Phase 1)

- New `tddy-demo-runner` crate: `DemoVm` trait, `QemuVmArgs` arg builder, `MockDemoVm`, `DemoOrchestrator` (deploy→verify→port-forward→Telegram link)
- VM lifecycle is UI/daemon-owned: `DemoOrchestrator::run()` receives an already-running `RunningVm`, never boots the VM
- Extended `DemoPlan` recipe with `mode` (`PortForward`/`ScreenShare`), `hostfwd` port maps, `deploy_steps`, `verify_command`, `build_target`; `DemoOutput` gains `share_url`
- `demo-plan.md` round-trips losslessly via JSON front-matter; `read_demo_plan_file` falls back for legacy files
- `BuildrootPlugin` + `QemuPlugin` registered in `tddy-tools` plugin registry
- Phase 2 (concrete SSH impl, daemon RPC `StartDemoVm`/`StopDemoVm`, web UI VM actions, nix guest image) tracked in [demo-goal.md](../demo-goal.md)
