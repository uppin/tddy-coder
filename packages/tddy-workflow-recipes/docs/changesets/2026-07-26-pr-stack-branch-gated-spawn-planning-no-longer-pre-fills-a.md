# 2026-07-26 — pr-stack-branch-gated-spawn: planning no longer pre-fills a node's branch

**Type:** Fix

`plan_pr_stack::planned_prs_into_stack_nodes` and `pr_stack::add_planned_pr_node` leave `branch = None` (they copied `branch_suggestion`, contradicting their own doc comments and unblocking spawns onto a ref nothing created). `pr_stack::reseed_stack_from_plan_if_unspawned` refuses once any node owns a **branch or** a session, since the branch outlives the session that created it. `orchestrate_pr_stack::assess::assemble_views` keys the PR lookup on the node's branch instead of `session_id.is_some()` and resolves it via `tddy_core::changeset::resolve_stack_node_branch` instead of inventing `feature/<node_id>`; a branchless node yields an empty `NodeView.branch`, which `effective_base_ref` skips like an absent parent. Feature [pr-stacking.md](../../../../docs/ft/coder/pr-stacking.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)
