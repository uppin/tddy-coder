# 2026-11-21 — Clean, Delete and Restore of a target session act on the target worktree

**Type:** Fix · **Stack:** `#target-parity` 2/7 (PR #970) · **Packages:** `tddy-worktree-service`,
`tddy-agent-launch`, `tddy-daemon` · Feature:
[session-worktree-inspector.md § On a target session](../../ft/web/session-worktree-inspector.md#on-a-target-session)

## Problem

Every worktree RPC resolved its path against the project's clone on the daemon host. A target
session's worktree (`/home/coder/.tddy/repos/<repo>/.worktrees/<id>`) is not there: Clean and Delete
failed with "not in git worktree list", and Restore cut a new worktree in the **local** clone and
rewrote the session's changeset to point at it while the agent kept working on the target.

## What changed

- **`tddy-worktree-service`**: `target_worktrees` — the `TargetShells` port (`NoTargetShells`
  default) and `WorktreeServiceImpl::with_target_shells`; `target_session_in` /
  `target_session_of_worktree`; `clean_target_worktree`, `remove_target_worktree` (moved here from
  agent-launch) and `restore_target_worktree`. `CleanWorktree`, `RemoveWorktree` and
  `RestoreSessionWorktree` route a target session there before touching the local clone.
- **`tddy-agent-launch`**: `target_shells::SessionTargetShells` over `shell_for_session_target`;
  `DeleteSession` calls the moved `remove_target_worktree`.
- **`tddy-daemon`**: `coder_credential_source::worktree_target_shells`, installed in
  `runtime/mount_host.rs`.

## Decisions

- **A port, not a dependency**: agent-launch already depends on worktree-service.
- **No proto change**: the target session is found from what each request already carries.
- **Restore checks a recorded branch out again** when the target's clone still has it, and only
  otherwise cuts it from the recorded integration base after `fetch --all` (rather than calling
  `setup_remote_worktree`, which the plan named).

## Wrap-time measurements

- `packages/tddy-worktree-service/src/service.rs`: 808 → **883** production lines (+75). Stack-shared
  with #975 (`#target-parity` 7/7); recorded as a regression in
  `packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`, not split.
- `target_worktrees.rs` (new): 241 production lines; `restore_target_worktree` split into
  `worktree_add_command` at the wrap so no function is over 40 lines.
- `runtime/mount_host.rs`: 456 → 462; `open_served` grows by 6 lines (the wiring lives in
  `coder_credential_source::worktree_target_shells`).

## Tests

`packages/tddy-worktree-service/tests/target_worktree_actions_acceptance.rs` (5 tests, a stand-in ssh
target through the port, real git on both sides); unit tests in `target_worktrees.rs` and
`tddy-agent-launch/src/target_shells.rs`.
