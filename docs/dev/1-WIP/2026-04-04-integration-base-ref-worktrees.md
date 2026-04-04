# Changeset: Git integration base ref for worktrees (documentation wrap)

**Date**: 2026-04-04  
**Status**: Complete (docs)  
**Type**: Documentation

## Affected packages

- `tddy-core` (architecture, changesets)
- `tddy-daemon` (changesets, connection-service)
- `docs/ft/coder`, `docs/ft/daemon`, `docs/dev`

## Summary

Documentation describes how integration base refs work for git worktrees: validation, default resolution, optional `main_branch_ref` on project rows, and pointers to follow-up wiring in daemon/workflow hooks.

## Technical content (State B)

See [git-integration-base-ref.md](../../ft/coder/git-integration-base-ref.md).

## Validation artifacts

Reports under [project-main-branch-ref-validate](./project-main-branch-ref-validate/): `evaluation-report.md`, `validate-tests-report.md`, `validate-prod-ready-report.md`, `analyze-clean-code-report.md`.

## References

- `packages/tddy-core/src/worktree.rs`
- `packages/tddy-daemon/src/project_storage.rs`
