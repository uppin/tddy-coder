# 2026-07-30 — PR-stack full control from the orchestrator chat

- The `pr-stack` orchestrator agent gains seven `mcp__tddy-tools__pr_*` tools and can now change the plan, not only grow it: `pr_update_planned` (edit title/description any time, `branch_suggestion` only while unspawned, opt-in `sync_pr` publishes to the PR), `pr_delete_planned` (remove a node, reparenting its children onto its parents; refuses an open PR), `pr_set_parents` (the plan-level move, and the only reorder primitive), `pr_read`, `pr_search`, `pr_comments`, and `pr_adopt` (bring an externally-created PR into the stack).
- Until now the plan became immutable the moment the stack became real — whole-plan rewrite refuses once any node owns a branch or a session, and no tool could edit, delete, reparent or adopt.
- `pr_repoint` is unchanged and still means "the base branch drifted"; `pr_set_parents` means "the plan changed". Both share one realignment tail (rebase, `--force-with-lease`, re-target the PR).
- Two documented limitations: a `pr_search` hit carries no head or base branch (GitHub's search does not report them — follow up with `pr_read`), and no comment thread is reported as resolved (that state is GraphQL-only).
- A search is always scoped to the orchestrator's own repository; the agent's text/author/base values are refused if they could inject a second `repo:` qualifier.
- See [pr-stacking.md § Full control over the plan](../pr-stacking.md#full-control-over-the-plan-added-2026-07-30).
