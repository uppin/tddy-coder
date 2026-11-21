# 2026-11-21 — Clean, Delete and Restore of a target session act on the target worktree

**Type:** Fix · **Stack:** `#target-parity` 2/7 (PR #970)

`target_shells::SessionTargetShells` implements the port over `shell_for_session_target`; `remove_target_worktree` / `target_checkout_root_of` moved to `tddy-worktree-service` and `DeleteSession` calls them there. Full record: [`docs/dev/changesets/2026-11-21-target-worktree-actions.md`](../../../../docs/dev/changesets/2026-11-21-target-worktree-actions.md).
