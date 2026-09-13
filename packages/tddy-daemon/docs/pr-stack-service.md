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

- **Handlers and peer routing** — `packages/tddy-daemon/src/pr_stack_rpc.rs` and
  `connection_service/svc_pr_stack_ports.rs` on `ConnectionServiceImpl`.
- **Orchestration and MCP tools** — `tddy-workflow-recipes` (`pr_stack`, `github_pr`, recipes).

`PrStackService` is served on the daemon's transports (HTTP `/rpc`, LiveKit common room, session
rooms, local socket). A separate `tddy-pr-stack-service` crate was not added: putting
`tddy-service` inside `tddy-workflow-recipes` would create a dependency cycle, so the coordinate is
hosted on the daemon while recipes stay in the workflow crate.

Product docs: [pr-stacking.md](../../../docs/ft/coder/pr-stacking.md),
[pr-stack-live-status.md](../../../docs/ft/web/pr-stack-live-status.md).
