# 2026-07-03 — "New planned PR" form on the PR-Stack Chat Screen

**Type:** Feature

new `AddPlannedPrForm` (title/description/branch-suggestion + a multi-select ancestor picker over the orchestrator's existing planned-PR nodes) wired into `PrStackScreen` via `client.addPlannedPr(...)`; a `stackPlanOverride` state seam updates the visible list immediately from the RPC response without waiting on a parent session refetch. Feature [pr-stacking.md § Manually adding a planned PR](../../../../docs/ft/coder/pr-stacking.md#manually-adding-a-planned-pr). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web, tddy-service)
