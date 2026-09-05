# 2026-07-03 — **`pr_stack::add_planned_pr_node`

**Type:** Feature

append a manually-created planned PR to the stack DAG** — additive-only mutation (never touches existing nodes, unlike `reseed_stack_from_plan_if_unspawned`'s whole-plan overwrite): server-assigns the next free `"n<N>"` node id (`next_free_node_id`), rejects a dangling parent ref or a would-be cycle (`Stack::topo_order`), appends atomically via `update_stack_atomic`. Params grouped into `AddPlannedPrInput`. Feature [pr-stacking.md § Manually adding a planned PR](../../../../docs/ft/coder/pr-stacking.md#manually-adding-a-planned-pr). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes, tddy-core)
