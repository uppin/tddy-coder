# 2026-07-03 — **`AddPlannedPr` handler

**Type:** Feature

manually add a planned PR to a pr-stack orchestrator** — `ConnectionServiceImpl::add_planned_pr` validates the session is a `"pr-stack"` orchestrator (`require_pr_stack_orchestrator`, resolves legacy aliases too) before delegating to `tddy_workflow_recipes::pr_stack::add_planned_pr_node`; `session_list_enrichment::stack_plan_json_for_changeset` narrowed to `pub(crate)` for reuse in the response. Feature [pr-stacking.md § Manually adding a planned PR](../../../../docs/ft/coder/pr-stacking.md#manually-adding-a-planned-pr). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon, tddy-service, tddy-workflow-recipes)
