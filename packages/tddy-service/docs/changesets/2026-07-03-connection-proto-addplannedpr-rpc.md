# 2026-07-03 — `connection.proto`: `AddPlannedPr` RPC

**Type:** Feature

new `AddPlannedPr(AddPlannedPrRequest) returns (AddPlannedPrResponse)` on `ConnectionService`; request carries `session_id`, `title`, `description`, `branch_suggestion`, `parents`, `child_recipe`; response reuses the existing `stack_plan_json` wire shape (no new `StackNode` message). Feature [pr-stacking.md § Manually adding a planned PR](../../../../docs/ft/coder/pr-stacking.md#manually-adding-a-planned-pr). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service, tddy-daemon, tddy-workflow-recipes, tddy-web)
