# 2026-08-29 — `stack-progress.json` is documented as a host guarantee but nothing writes it

**Category:** Future enhancement
**Source:** pr-stack-docs changeset, 2026-08-29

`docs/ft/coder/pr-stacking.md` § "Progress tracking contract" states each child session is obliged to
maintain `artifacts/stack-progress.json`, written by a shared child hook's `after_task` — "a host
guarantee, not an agent promise". No production code writes that file; the only repo-wide match is a
scratch-directory label in `packages/tddy-workflow-recipes/tests/stack_progress_contract_acceptance.rs:16`.
The orchestrator in fact syncs through `sync_stack_node_from_child`, reading the child's
`changeset.yaml` for `state.current` and `workflow.github_pr_status`. PR-stack RPCs are served as
`pr_stack.PrStackService` on the daemon (`packages/tddy-daemon/docs/pr-stack-service.md`); recipe
logic and MCP tools remain in `tddy-workflow-recipes`. Either implement `stack-progress.json` or
correct the document — a reader currently believes a recipe-agnostic progress signal exists.
