# 2026-06-20 — **Demo goal Phase 1

**Type:** Feature

DemoMode/PortMap, DemoPlan extensions, JSON front-matter round-trip** — `DemoMode` (`PortForward`/`ScreenShare`, serde `snake_case`), `PortMap` (`host_port`/`guest_port`), `DemoPlan` extended with `mode`, `hostfwd`, `deploy_steps`, `verify_command`, `build_target` (all `#[serde(default)]`); `DemoOutput` gains `share_url`; `write_demo_plan_file`/`read_demo_plan_file` with JSON front-matter round-trip and legacy fallback. Feature [coder/demo-goal.md](../../../../docs/ft/coder/demo-goal.md); PR [#214](https://github.com/uppin/tddy-coder/pull/214). (tddy-workflow-recipes)
