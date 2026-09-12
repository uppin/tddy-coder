# 2026-09-12 — The catalogue, exec-tool and PR-stack services

The daemon's RPC surface finishes unbundling: tools, agents, exec-tool execution and PR-stack planning
leave `connection.ConnectionService` for dedicated services.

**`catalog.CatalogService`** (four methods) lists configured tools, coding backends, registry models
and attachable subagent defs. **`exec_tools.ExecToolService`** (four methods) runs the ten
worktree tools, streams large results, lists the catalog and reads the session tool-call log.
**`pr_stack.PrStackService`** (eight methods) drives planned-PR stacks — add, reorder, repoint, query
branches, pull base, link child sessions, and read PR status.

`connection.ConnectionService` keeps **17** methods: starting and connecting sessions, projects and
branches, demo VM control, worktree snapshots for session rooms, streaming session start with
attachments, and minting a local token over the Unix socket.

## For operators

Upgrade **daemon, web bundle and in-jail tool clients together**. Sixteen coordinates moved; an older
web bundle or tool IPC client that still calls `connection.ConnectionService` for catalogue, exec-tool
or PR-stack methods will fail at runtime.

In-jail tool execution and sandbox relays must target **`exec_tools.ExecToolService/ExecuteTool`**
(the allowlist operation set is unchanged).

Technical reference: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md),
[catalog-service.md](../../../packages/tddy-discovery/docs/catalog-service.md),
[pr-stack-service.md](../../../packages/tddy-daemon/docs/pr-stack-service.md),
[tddy-tool-engine README](../../../packages/tddy-tool-engine/README.md).
