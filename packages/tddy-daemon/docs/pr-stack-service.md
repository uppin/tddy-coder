# `pr_stack.PrStackService`

Family **P** — planned-PR stack mutations and branch resolution for `pr-stack` orchestrator sessions.

| RPC | Purpose |
|-----|---------|
| `AddPlannedPr` | Append a planned node to the orchestrator's `Changeset.stack`. |
| `GetPrStatus` | GitHub PR status for a stack branch. |
| `RepointPlannedPr` | Change a planned node's target base branch. |
| `ReorderPlannedPr` | Move a planned row up/down in display order. |
| `PullBaseIntoBranch` | Merge or rebase a node's branch onto its stack base inside that worktree. |
| `QueryBranch` | Resolve session, worktree, PR, dirty state and base-sync legs for a branch. |
| `ResolveStackBase` | Read-side stack base resolution for spawn dialogs. |
| `LinkStackNode` | Record a child session and branch on a planned node. |

## Where logic lives

- **Trait, adapter and transport entry** — `tddy_pr_stack::rpc`: `PrStackHandler`,
  `PrStackServiceImpl`, `build_pr_stack_entry`, `PR_STACK_SERVICE`
  ([tddy-pr-stack](../../tddy-pr-stack/docs/architecture.md#rpcrs--the-pr-stack-rpc-family)).
- **Handler and peer routing** — `tddy_daemon_rpc::PrStackRpcHandler`
  ([tddy-daemon-rpc](../../tddy-daemon-rpc/docs/architecture.md)): the eight RPC bodies, the
  orchestrator and repoint guards, and the PR and base-sync legs of a branch resolution.
- **Session start's stack paths** — `tddy-session-lifecycle` reaches the same handler through its
  `DaemonRpcFamilies` port to resolve a peer-owned stack base and to link a named stack node.
- **Orchestration and MCP tools** — `tddy-workflow-recipes` (`pr_stack`, `github_pr`, recipes).

`PrStackService` is served on the daemon's transports (HTTP `/rpc`, LiveKit common room, session
rooms, local socket); `runtime.rs` takes the service and its entry from `RpcHandlers`.

Product docs: [pr-stacking.md](../../../docs/ft/coder/pr-stacking.md),
[pr-stack-live-status.md](../../../docs/ft/web/pr-stack-live-status.md).
