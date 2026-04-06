# Changeset: Chain PR optional integration base (worktrees)

**Date**: 2026-04-05  
**Status**: Complete  
**Type**: Feature

## Affected Packages

- tddy-core
- tddy-integration-tests
- docs (feature + dev index)

## Related Feature Documentation

- [git-integration-base-ref.md](../../ft/coder/git-integration-base-ref.md)

## Summary

Library support for optional **chain PR** integration bases: multi-segment remote-tracking refs under `origin/` (for example `origin/feature/pr-base`), validation, fetch, session worktree setup with optional user-selected base, persisted fields on `changeset.yaml`, and resume resolution from persisted effective or user-selected refs.

## Scope

- [x] `tddy-core` `worktree`: `validate_chain_pr_integration_base_ref`, `fetch_chain_pr_integration_base`, `setup_worktree_for_session_with_optional_chain_base`, `resolve_persisted_worktree_integration_base_for_session`
- [x] `tddy-core` `changeset`: `effective_worktree_integration_base_ref`, `worktree_integration_base_ref`
- [x] Integration tests `chain_pr_base_acceptance`
- [x] Feature doc `docs/ft/coder/git-integration-base-ref.md`
- [x] Changelogs: `docs/ft/coder/changelog.md`, `docs/dev/changesets.md`, `packages/tddy-core/docs/changesets.md`
- [x] Architecture excerpt `packages/tddy-core/docs/architecture.md`

## Technical Changes (State B)

### tddy-core / worktree

- **`validate_chain_pr_integration_base_ref`**: Validates refs of the form `origin/<path>` where `<path>` may contain `/`; rejects empty input, `..`, `--`, whitespace, and shell-oriented metacharacters in the path.
- **`fetch_chain_pr_integration_base`**: After validation, runs `git fetch origin <path>` (no shell).
- **`setup_worktree_for_session_with_optional_chain_base(repo_root, session_dir, optional_chain_base_ref)`**: With `None`, resolves the default integration base (`resolve_default_integration_base_ref`), fetches via `fetch_integration_base`, creates the worktree, sets **`effective_worktree_integration_base_ref`** on the changeset, leaves **`worktree_integration_base_ref`** unset. With `Some(ref)`, validates with **`validate_chain_pr_integration_base_ref`**, fetches via **`fetch_chain_pr_integration_base`**, creates the worktree from that ref, sets both **`effective_worktree_integration_base_ref`** and **`worktree_integration_base_ref`**.
- **`resolve_persisted_worktree_integration_base_for_session`**: Returns persisted **`effective_worktree_integration_base_ref`**, else **`worktree_integration_base_ref`**, else **`resolve_default_integration_base_ref`**.

### tddy-core / changeset

- **`effective_worktree_integration_base_ref`**: Remote-tracking ref used to create the session worktree (default-resolved or explicit).
- **`worktree_integration_base_ref`**: Present when the user opted into a chain-PR base; absent when only default resolution applies.

### tddy-integration-tests

- **`chain_pr_base_acceptance`**: Covers default path, selected `origin/...` base, YAML persistence, validation, resume resolution.

## Acceptance Tests

- `packages/tddy-integration-tests/tests/chain_pr_base_acceptance.rs`
- `packages/tddy-core/src/worktree.rs` (`chain_pr_red_tests`, `integration_base_red_tests` modules)

## Out of Scope (follow-up)

- Daemon / RPC / web surfaces passing an optional chain base into session start (callers continue to use **`setup_worktree_for_session`** where not wired).
- Parity of **`effective_worktree_integration_base_ref`** writes on **`setup_worktree_for_session_with_integration_base`** (separate consistency task).
