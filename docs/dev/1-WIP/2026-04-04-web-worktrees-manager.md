# Changeset: Web Worktrees manager (library-first)

**Date**: 2026-04-04  
**Status**: Complete (documentation wrap)  
**Type**: Feature

## Affected packages

- `tddy-daemon`
- `tddy-web`
- `docs` (feature and dev)

## Related feature documentation

- [docs/ft/web/worktrees.md](../../ft/web/worktrees.md)
- [docs/ft/web/web-terminal.md](../../ft/web/web-terminal.md)

## Summary (State B)

The repository includes a **`tddy_daemon::worktrees`** library: **`git worktree list`** parsing, persisted per-project stats under **`TDDY_PROJECTS_STATS_ROOT`**, lexical path policy helpers, and **`git worktree remove`** for non-primary worktrees. **`tddy-web`** ships a **`WorktreesScreen`** component and Cypress coverage with mocked data. **ConnectionService** has no worktree RPCs; shell hamburger navigation and daemon-backed listing are outside this milestone.

## Scope

- [x] Daemon `worktrees` module with tests (`worktrees`, `worktrees_rpc`)
- [x] Web `WorktreesScreen` + component test
- [x] Feature and package documentation, changelogs, cross-package dev changeset index

## Follow-up (not in this changeset)

- `connection.proto` RPCs and `connection_service` handlers
- Background refresh scheduler with bounded concurrency
- Canonical path policy where symlinks matter
- Shell route and `ListEligibleDaemons` wiring for Worktrees

## References

- Package: [packages/tddy-daemon/docs/worktrees.md](../../../packages/tddy-daemon/docs/worktrees.md)
- Evaluation artifacts (when present): `workflow-free-prompting-validate/` reports (parallel validation run; distinct topic)
