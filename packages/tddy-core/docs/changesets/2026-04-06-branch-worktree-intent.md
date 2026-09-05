# 2026-04-06 — Branch/worktree intent

**Type:** Feature

**`BranchWorktreeIntent`** on **`ChangesetWorkflow`**; **`branch_worktree_intent`** module (**`validate_workflow_branch_intent`**, **`resolve_branch_and_worktree_plan`**, **`merge_branch_worktree_intent_into_context`**); **`setup_worktree_for_session_with_integration_base`** / **`setup_worktree_for_session_with_optional_chain_base`** honor persisted **`workflow`** intent; **`merge_persisted_workflow_into_context`** merges intent keys into **`Context`**. Tests: **`branch_worktree_intent_acceptance`**, **`branch_worktree_intent_red`**. Feature docs: [workflow-json-schemas.md](../../../../../docs/ft/coder/workflow-json-schemas.md), [workflow-recipes.md](../../../../../docs/ft/coder/workflow-recipes.md), [git-integration-base-ref.md](../../../../../docs/ft/coder/git-integration-base-ref.md). (tddy-core, tddy-tools)
