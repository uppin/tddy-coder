# 2026-07-11 — Additive `SandboxPlan.cgroup` (`CgroupConfig`) for the unprivileged Linux jail

**Type:** Feature

a Default-empty `CgroupConfig` field on `SandboxPlan` (re-exported from `lib.rs`) carries the runtime-derived, config-overridable delegated cgroup base + controller list into `tddy-sandbox-cgroups`; additive so macOS/QEMU backends and every existing `build()`/`SandboxPlan` construction ignore it and still compile. Enables the cgroups jail to run under an unprivileged `User=tddy` daemon (`Delegate=yes` + AppArmor functional userns probe). Architecture [architecture.md § Linux cgroups jail](../architecture.md#linux-cgroups-jail). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox, tddy-sandbox-cgroups, tddy-daemon)
