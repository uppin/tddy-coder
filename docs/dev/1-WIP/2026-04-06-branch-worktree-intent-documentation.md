# Changeset: Branch/worktree intent — documentation

**Date**: 2026-04-06  
**Status**: Complete  
**Type**: Documentation

## Affected Packages

- docs (feature + dev index)
- packages/tddy-core (changesets history)
- packages/tddy-tools (changesets history)
- packages/tddy-workflow-recipes (changesets history)
- packages/tddy-service (changesets history)

## Related Feature Documentation

- [workflow-json-schemas.md](../../ft/coder/workflow-json-schemas.md)
- [workflow-recipes.md](../../ft/coder/workflow-recipes.md)
- [planning-step.md](../../ft/coder/planning-step.md)
- [git-integration-base-ref.md](../../ft/coder/git-integration-base-ref.md)
- [coder/changelog.md](../../ft/coder/changelog.md)

## Summary

Product and package documentation describe **`BranchWorktreeIntent`** on the durable **`changeset.yaml`** **`workflow`** block, JSON Schema validation via **`persist-changeset-workflow`**, **`tddy_core::changeset::merge_persisted_workflow_into_context`** and **`merge_branch_worktree_intent_into_context`**, worktree setup behavior in **`setup_worktree_for_session_with_integration_base`** and **`setup_worktree_for_session_with_optional_chain_base`**, and **`WorktreeElicitation`** optional fields in **`remote.proto`** that mirror the workflow keys.

## Scope

- [x] `docs/ft/coder/workflow-json-schemas.md` — **`changeset-workflow`** branch/worktree fields and merge behavior
- [x] `docs/ft/coder/workflow-recipes.md` — TDD changeset workflow bullet
- [x] `docs/ft/coder/planning-step.md` — **`workflow`** object and cross-links
- [x] `docs/ft/coder/git-integration-base-ref.md` — Related: changeset workflow intent
- [x] `docs/ft/coder/changelog.md` — 2026-04-06 entry
- [x] `docs/dev/changesets.md` — cross-package index entry
- [x] Package `changesets.md` for **tddy-core**, **tddy-tools**, **tddy-workflow-recipes**, **tddy-service**
