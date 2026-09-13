# 2026-09-09 — Hosts and worktrees are services of their own

The daemon serves three gRPC services where it served one. `host.HostService` (8 methods) answers
everything about the machines a daemon knows — the durable registry, the tooling probe, host
telemetry, and the encrypted prompt channel that loads an ssh key. `worktree.WorktreeService` (9
methods) answers everything about the git worktrees it holds — listing, sizing, cleaning, restoring,
and browsing the files inside one. `connection.ConnectionService` keeps the other 73.

Nothing behaves differently: the same handlers answer the same requests, at a new service name, over
the same three transports. The one user-visible consequence is that a `tddy-web` bundle and a daemon
must come from the same side of the split, because 17 method coordinates moved.

What made it possible, and what the split is for:
[host-worktree-services.md](../host-worktree-services.md).
