# 2026-09-12 — The catalogue, exec-tool and PR-stack services

**Type:** Architecture

Node 8 of the `#unbundle` stack ([#477](https://github.com/uppin/tddy-coder/pull/477), 8 of 9, based
on node 7's branch, [#476](https://github.com/uppin/tddy-coder/pull/476)). Sixteen RPCs leave
`connection.ConnectionService` — **33 → 17** — into three coordinates served from crates that already
owned those domains.

PRD changelog: [docs/ft/daemon/changelog/2026-09-12-unbundle-exec-prstack-services.md](../../ft/daemon/changelog/2026-09-12-unbundle-exec-prstack-services.md).

## The three coordinates

| Coordinate | Family | Methods | Served by |
|---|---|---:|---|
| `catalog.CatalogService` | A | 4 | [`tddy-discovery`](../../../packages/tddy-discovery/docs/catalog-service.md) |
| `exec_tools.ExecToolService` | L | 4 | [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md) |
| `pr_stack.PrStackService` | P | 8 | [`tddy-daemon` `pr_stack_rpc.rs`](../../../packages/tddy-daemon/docs/pr-stack-service.md) |

Family P is hosted on the daemon, not `tddy-workflow-recipes`: adding `tddy-service` there would
cycle. Orchestration, MCP tools and `github_pr` stay in the recipes crate; handlers and peer routing
live in `pr_stack_rpc.rs` and `svc_pr_stack_ports.rs`.

All three register on the local Unix socket (generated tonic adapters from node 6), the LiveKit
common room, **session rooms** (catalog, exec-tool and PR-stack beside session files/agents/activity),
and HTTP `/rpc`. In-jail and session-tool paths call `exec_tools.ExecToolService`; peer attach
forwards `catalog.CatalogService/ListSubagents`; clone mirrors stream tools at
`exec_tools.ExecToolService/StreamExecuteTool`.

## What closed

- One exec-tool catalog end to end — define, execute and serve from `tddy-tool-engine`; vacuous
  cross-crate guard tests removed.
- The last hand-built `connection.ConnectionService/ExecuteTool` URL in `tddy-discovery` replaced
  by a generated client.
- Sandbox relay allowlist and `sandboxed_session` mirror guard name `exec_tools.ExecToolService/ExecuteTool`.

## `connection.ConnectionService` endpoint

Seventeen methods remain — families C, D, O and Q (sessions, projects, demo VM, worktree snapshot,
`MintLocalToken`, `StreamStartSession`). See
[`connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md).

## Verification

CI on `c564e3bf`: Rust **6655/6655**, web **2630/2630**. Scoped local gates during development
matched touched packages; full workspace left to CI per repo policy.
