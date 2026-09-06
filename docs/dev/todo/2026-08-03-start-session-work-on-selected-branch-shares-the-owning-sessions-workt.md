# 2026-08-03 — `start_session_work_on_selected_branch_shares_the_owning_sessions_worktree` leaks a fixed-name directory

**Category:** Future enhancement

`packages/tddy-daemon/tests/session_branch_conflict_acceptance.rs:477` builds the owner worktree as
`world.repo.parent().unwrap().join("feat-auth-owner")` — a **sibling** of the temp repo, at a fixed
name. When the temp repo lands directly under `/tmp`, that sibling is `/tmp/feat-auth-owner`, outside
the `TempDir` and so never cleaned up. The next run then fails with

```
git worktree add -b feat/auth /tmp/feat-auth-owner main failed:
fatal: '/tmp/feat-auth-owner' already exists
```

and keeps failing until somebody removes it by hand — which is how it surfaced: a run showed it as a
new failure that was really the residue of an earlier one. Costly because it looks exactly like a
regression in the session-start path.

Fix: put the owner worktree *inside* the test's `TempDir` (a child of `world.repo`'s tempdir root
rather than a sibling of `world.repo`), so it dies with the fixture.
