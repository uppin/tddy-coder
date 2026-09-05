# 2026-07-01 — `BackendInvokeTask` persists no-submit output to context

**Type:** Fix

the `Continue` branch taken when `WorkflowRecipe::goal_requires_tddy_tools_submit` is `false` and no host clarification gate fires now calls `context.set_sync("output", response.output.clone())` (mirroring the submit-success branch), so callers that only see `Context` after the engine call returns — not `TaskResult` — have the response available; enables `tddy-coder`'s plain-mode CLI fix for `free-prompting`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)
