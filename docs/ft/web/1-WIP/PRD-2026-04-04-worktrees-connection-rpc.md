# PRD: Worktrees ConnectionService + tddy-web (local daemon)

**Date**: 2026-04-04  
**Status**: In progress  
**Product area**: Web / daemon

## Summary

Expose Git worktree listing, cached stats refresh, and secondary worktree removal through **ConnectionService** RPCs backed by **`tddy_daemon::worktrees`**, and drive **`WorktreesAppPage`** from real data for the **local** daemon. Multi-host routing is out of scope.

## Background

The worktrees **library** and a **shell UI** (hamburger, `/worktrees`) exist; **no wire protocol** connected them to the browser.

## Requirements

1. **Proto**: `ListWorktreesForProject`, `RemoveWorktree` on `ConnectionService` with documented request/response shapes.
2. **Daemon**: Resolve `project_id` → `main_repo_path` via **`main_repo_path_for_host`** with **local** `daemon_instance_id`; run cache list/refresh/remove; validate paths; map errors to gRPC status.
3. **Web**: Load projects and sessions token like Connection screen; project selector; **Refresh** runs expensive stats; table shows rows; **Delete** calls remove RPC.
4. **Performance**: List path serves **cached** snapshots unless **`refresh=true`** on list request.

## Success criteria

- `./web-dev`: select project → refresh → table matches `git worktree list` / cache for that repo path.
- Delete secondary worktree succeeds; primary refusal surfaces as user-visible error.
- `./dev ./verify` passes.

## Out of scope

- RPC routing to remote `daemon_instance_id`.
- Background refresh scheduler.

## Affected documentation

- [worktrees.md](../worktrees.md)
- [local-web-dev.md](../local-web-dev.md)
