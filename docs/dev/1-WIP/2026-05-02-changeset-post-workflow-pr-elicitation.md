# Changeset: post-workflow GitHub PR + worktree elicitation (persistence layer)

**Date:** 2026-05-02  
**Area:** Coder workflow durability, engine **`Context`**, **`tddy-tools`**

## Scope

Cross-package work that lands **post-workflow** fields on **`ChangesetWorkflow`**, extends **`urn:tddy:tool/changeset-workflow`**, merges them through **`merge_persisted_workflow_into_context`**, and ships **`tddy_core::post_workflow`** policy helpers plus integration tests (**`post_workflow_pr_elicitation_*`**, **`merge_red`**).

## Packages

- **`tddy-workflow-recipes`**: generated **`changeset-workflow.schema.json`** — **`post_workflow_open_github_pr`**, **`post_workflow_remove_session_worktree`**, **`github_pr_status`** (strict object).
- **`tddy-core`**: **`GithubPrStatus`**, **`ChangesetWorkflow`** fields, **`merge_post_workflow_into_context`**, **`post_workflow`** module, re-exports.
- **`tddy-tools`**: **`persist-changeset-workflow`** continues to validate against the embedded schema; acceptance covers round-trip + merge.

## Documentation transfer target

Feature reference: **`docs/ft/coder/post-workflow-github-pr-elicitation.md`**.  
Schema reference: **`docs/ft/coder/workflow-json-schemas.md`**.

Per repository rules, **package `packages/*/docs/`** remains unchanged here; cross-package index lives in **`docs/dev/changesets.md`** and **`docs/ft/coder/changelog.md`**.

## Follow-up (presenter / transport)

Wire ordered elicitation at **`WorkflowComplete`** boundary, GitHub execution, YAML phase progression, and worktree removal with **`tddy_core::worktree`** conventions; align plain mode, TUI streaming, gRPC, and Telegram with existing **`demo_options`** / **`run_optional_step_x`** persistence patterns.
