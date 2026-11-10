# 2026-11-02 — DaemonActionSandbox runs hermetic build actions

**Type:** Feature

`build_action_sandbox::DaemonActionSandbox` implements `tddy_build::ActionSandbox` over `spawn_sandbox_plan` with `lower_tools`/`lower_network`; depends on `tddy-build`. Write/exec confinement tests are macOS-only (`FIXME(fs-confinement)`). Cross-package: [agent-tools-build-tools](../../../../docs/dev/changesets/2026-11-02-agent-tools-build-tools.md).
