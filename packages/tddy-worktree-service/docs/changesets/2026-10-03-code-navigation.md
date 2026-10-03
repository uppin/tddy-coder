# 2026-10-03 — `resolve_listed_worktree` is public

**Type:** Feature

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574). Cross-package entry:
[2026-10-03-code-navigation.md](../../../../docs/dev/changesets/2026-10-03-code-navigation.md).

`WorktreeServiceImpl::resolve_listed_worktree` (token → OS user → project main repo → `git worktree
list` membership) is `pub`, so `tddy-daemon`'s code navigation service reuses the exact authorisation
instead of copying it. No behaviour change. See [worktree-service.md](../worktree-service.md).

Code issues: `oversized-file-service` and the other records in this package are unchanged — the edit is
a visibility keyword.
