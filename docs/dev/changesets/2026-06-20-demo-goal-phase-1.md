# 2026-06-20 — **Demo goal Phase 1

**Type:** Feature

DemoOrchestrator, QEMU arg builder, plugin registry** — new `tddy-demo-runner` crate: `DemoVm` trait, `QemuVmArgs` (hostfwd_spec/netdev_arg/build), `MockDemoVm`, `DemoOrchestrator::run(recipe, RunningVm)` (deploy→verify→forward→Telegram; VM lifecycle UI/daemon-owned); `tddy-workflow-recipes` gains `DemoMode`/`PortMap`, extended `DemoPlan`, `share_url` on `DemoOutput`, JSON front-matter round-trip; `tddy-tools` registers `BuildrootPlugin`+`QemuPlugin`. Phase 2 (QemuDemoVm SSH, daemon RPC, web UI) deferred. PR [#214](https://github.com/uppin/tddy-coder/pull/214); feature [coder/demo-goal.md](../../ft/coder/demo-goal.md). (tddy-demo-runner, tddy-workflow-recipes, tddy-tools)
