# 2026-08-13 — `docs/ft/daemon/background-tasks.md` claims TaskService unary methods peer-forward

**Category:** Future enhancement
**Source:** remote-managed-worktree changeset, 2026-08-13

`docs/ft/daemon/background-tasks.md:153-155` states TaskService's unary methods "forward via
`livekit_peer_discovery::forward_to_peer`". They do not — `packages/tddy-daemon/src/task_service.rs`
contains no `forward_to_peer` call and no `classify_peer_route`, so `daemon_instance_id` on those RPCs
is silently ignored rather than routed or rejected. Either implement the forwarding or correct the doc
and reject a non-local id the way `WatchTask` already does (`task_service.rs:151-155`).
