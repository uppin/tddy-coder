# 2026-11-02 — --exec-plan: the generic tool mode

**Type:** Feature

`exec_plan::{ExecPlan { argv, env, files, egress_shim }, ExecPlanFile, run_exec_plan}` and the `--exec-plan <file>` flag (exclusive with `--workspace-tools`): files written, the child env replaced (no `TDDY_SANDBOX_*`, `TDDY_SECRET_*` resolved, proxy env when a shim runs), the jail ending with the process. `run_sandbox_runner_inner` takes the plan; `ChildEnv` on the generic PTY. Gains `serde`. Cross-package: [agent-tools-claude-jail](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-jail.md).
