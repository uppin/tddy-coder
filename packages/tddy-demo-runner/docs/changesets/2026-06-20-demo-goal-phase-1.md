# 2026-06-20 — **Demo goal Phase 1

**Type:** Feature

DemoVm trait, QemuVmArgs, MockDemoVm, DemoOrchestrator** — new crate; `DemoVm` trait (boot/deploy/verify/forward/shutdown, mockable VM boundary); `QemuVmArgs::build/hostfwd_spec/netdev_arg` (pure QEMU argv builder, unit-testable without process spawning); `MockDemoVm` recording test double; `DemoOrchestrator::run(recipe, RunningVm)` (validate PortForward → deploy → verify → forward → Telegram notify; never calls boot — VM lifecycle is UI/daemon-owned). `QemuDemoVm` concrete SSH/boot implementation deferred. Feature [coder/demo-goal.md](../../../../docs/ft/coder/demo-goal.md); PR [#214](https://github.com/uppin/tddy-coder/pull/214). (tddy-demo-runner)
