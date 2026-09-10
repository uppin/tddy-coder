# 2026-09-09 — The host and worktree subsystems leave the daemon

**Type:** Architecture

Root node of the `#unbundle` stack ([#470](https://github.com/uppin/tddy-coder/pull/470)). Full
story in the cross-package entry: [2026-09-09-unbundle-host-worktree-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md).

21 modules left `tddy-daemon` for `tddy-host-service` and `tddy-worktree-service`, and `config.rs`
plus six symbol lifts left for `tddy-daemon-kernel`. The crate keeps 86 modules and no cycle that
crosses a destination crate. `connection_service/rpc_service.rs` fell 6,938 → 5,712 and
`connection_tonic_adapter.rs` 1,505 → 1,306.

**`run_server` takes `RunServerOptions`** and the `#[allow(clippy::too_many_arguments)]` is gone. The
struct has **no `Default`**: a defaulted `host: ""` would fail to bind at runtime instead of failing
to compile, so every caller states all twelve fields. Four call sites migrated — `main.rs` and three
of the daemon's own tests. `tddy-desktop` is **not** a caller, contrary to the plan; its only mention
of `run_server` is prose in a `TODO` comment.

**The daemon stays the wiring layer for all three services.** `runtime.rs` builds one
`HostServiceImpl` and one `WorktreeServiceImpl` and registers each from the same `Arc` on all three
transports; `local_socket_server.rs` mounts all three on the **same** `Server::builder()`, so a
caller that reached `GetHostTooling` over the local socket before the split still does.
`ConnectionServiceImpl::routing_view` is new `pub` API rather than a widening — the config, roster
and token resolver every pre-authentication routing decision is made from — and
`connection_tonic_adapter::to_tonic_status` is shared, because three adapters over one socket must
map a refusal to the same tonic code and three copies of that match is three chances to drift.

**What the daemon kept a hold on.** `session_reader::DaemonSessionListing` implements
`tddy_worktree_service::branch_owner::SessionListing`: a branch belongs to a worktree, but what
*claims* one is a session, and sessions stay here. Four `worktree_files` and `host_prompts` helpers
widened from `pub(crate)` to `pub` for callers that stayed — `context_files`, `context_sync`,
`host_documents` and `screen_sharing_service`.

Docs: [connection-service.md](../connection-service.md) lost the 17 moved endpoints;
[host-registry.md](../../../tddy-host-service/docs/host-registry.md),
[host-tooling-probe.md](../../../tddy-host-service/docs/host-tooling-probe.md),
[host-add-key.md](../../../tddy-host-service/docs/host-add-key.md),
[worktrees.md](../../../tddy-worktree-service/docs/worktrees.md) and
[remote-git-service.md](../../../tddy-worktree-service/docs/remote-git-service.md) moved with the
code they document.
