# PRD: The catalogue, exec-tool and PR-stack services

**Date**: 2026-09-09
**PRD Type**: Architecture Change (breaking RPC change — 16 methods)
**Product Area**: daemon
**Stack**: `#unbundle` node 8 of 8 — the last

## Affected Features

- [pr-stacking.md](../../coder/pr-stacking.md), [pr-stack-live-status.md](../../web/pr-stack-live-status.md) — family P moves
- [remote-codebase-mode.md](../remote-codebase-mode.md) — family L moves; the in-jail relay allowlist changes again
- [specialized-subagents.md](../../coder/specialized-subagents.md) — `ListSubagents` moves
- [model-registry.md](../../../packages/tddy-model-registry/docs/model-registry.md) — `ListAgentModels` moves
- [rpc-playground.md](../rpc-playground.md) — the picker reaches its final shape
- [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md) — the 931-line
  endpoint reference reaches its final 17 entries

## Summary

The last 16 methods this stack moves leave `connection.ConnectionService`, taking it from 33 to **17**.
The tools-and-agents catalogue (family A, 4) becomes `catalog.CatalogService`; tool execution
(family L, 4) becomes `exec_tools.ExecToolService`; PR-stack orchestration (family P, 8) becomes
`pr_stack.PrStackService`.

**This node adds no new crates.** All three services are served from crates that already own their
domains after node 5 — `tddy-discovery`, `tddy-tool-engine` and `tddy-workflow-recipes`. It is the
node that finishes the job rather than the one that creates more structure.

## Background

### It closes the last duplication

`server::exec_tool_catalog()` in `tddy-tools` was a hand-copied `RemoteToolDef` clone of
`tddy_tool_engine::catalog::tool_catalog()`, kept in step by matched guard tests in both crates
(`server.rs:1578-1579` names them) plus a third guard that lived in the daemon as
`tool_catalog_sync.rs`. Node 3 relocated that third guard; node 5 moved the clone into
`tddy-tool-engine` and collapsed it to one catalog. **This node makes `tddy-tool-engine` the crate
that also *serves* the tools it defines and executes** — after which the catalog has one definition,
one executor and one served coordinate, and the guard tests have nothing left to guard.

### It changes the sandbox relay allowlist a second time

`packages/tddy-sandbox-runner/src/runner.rs:69` is the other half of the boundary node 7 touched:

```rust
if service == "connection.ConnectionService" && method == "ExecuteTool"
```

`tddy-sandbox-app/src/sandboxed_session.rs:708` carries the mirror guard. Both are family L, and both
move here. As in node 7, the acceptance test drives a real jail rather than inspecting the condition —
an allowlist that no longer matches the served coordinate fails *closed*, so an inspection test would
pass while every in-jail tool call was broken.

### It removes the last hard-coded URL

`packages/tddy-discovery/src/tools.rs:141` builds its request path by hand:

```rust
format!("{}/connection.ConnectionService/ExecuteTool", env.daemon_url.trim_end_matches('/'))
```

with four wiremock path assertions behind it. That is the only place in the repo where a
`ConnectionService` coordinate is a string literal in a URL rather than a generated client call, and
it is the one consumer that a proto change cannot break at compile time.

## Proposed Changes

### What changes

| New coordinate | Family | Methods | Served by | Source that moves |
|---|---|---:|---|---|
| `catalog.CatalogService` | A | `ListTools`, `ListAgents`, `ListAgentModels`, `ListSubagents` | `tddy-discovery` | `agent_list_mapping.rs` (94) |
| `exec_tools.ExecToolService` | L | `ExecuteTool`, `StreamExecuteTool`, `ListExecTools`, `ListSessionToolCalls` | `tddy-tool-engine` | `tool_call_log.rs` (276), `session_toolcall.rs` (218) |
| `pr_stack.PrStackService` | P | `AddPlannedPr`, `GetPrStatus`, `RepointPlannedPr`, `ReorderPlannedPr`, `PullBaseIntoBranch`, `QueryBranch`, `ResolveStackBase`, `LinkStackNode` | `tddy-workflow-recipes` | the PR-stack handlers |

Family A's four methods are answered from configuration rows and from the agent catalogue rather than
from the daemon's own state — `main.rs` already extracts the rows with
`agent_list_mapping::agent_allowlist_rows(&config, &[])` and hands them over, so `tddy-discovery`
takes rows rather than a `DaemonConfig`.

### What stays the same

- Every catalogue, execution and PR-stack behaviour.
- **The 17 methods that remain**, deliberately: family C (sessions lifecycle, 8), D (projects and
  branches, 5), O (demo VM, 3), Q (`MintLocalToken`, UDS-only, 1), and the 4 that stay with them. That
  is the irreducible core — a daemon that starts, resumes, signals and deletes sessions, owns
  projects, and mints a local token over a peer-credentialled socket.

## Impact Analysis

### Technical

| Area | Impact |
|---|---|
| `tddy-daemon` | −3 modules, −588 prod LoC; three `ServiceEntry` groups; `connection.ConnectionService` reaches its final 17 methods |
| `tddy-tool-engine` | defines, executes **and serves** the same ten tools; the guard tests become vacuous and are deleted |
| `tddy-discovery` | serves the catalogue it already resolves; its hard-coded URL becomes a generated client call |
| `tddy-workflow-recipes` | serves the PR-stack surface it already implements and already exposes as MCP tools |
| `tddy-sandbox-runner`, `tddy-sandbox-app` | the family-L allowlist and its mirror guard |
| `tddy-coder` | its family-L dispatch moves **in lockstep** |
| `tddy-service` | three protos appear; `connection.proto` loses 16 rpcs and reaches its final shape |
| `tddy-web` | `SessionInspectorDrawer`, `PrStackScreen`, `useQueryBranch`, `useAvailableAgents`, `useSelectableAgents`, `useAgentModels`, `CreateSessionPane` |

### User-facing

16 RPC coordinates move. After this node a web bundle must match the daemon on every service in the
stack; there is no partial-compatibility window left.

## Implementation Plan

1. Three protos created, importing `types.proto`.
2. `tddy-tool-engine` serves family L; the vacuous guard tests deleted.
3. **The sandbox-runner allowlist and the `tddy-sandbox-app` guard updated in the same commit.**
4. `tddy-discovery` serves family A and its hard-coded URL becomes a generated call.
5. `tddy-workflow-recipes` serves family P.
6. `tddy-coder`'s participant moved for family L.
7. `tddy-web` migrated; `connection.proto` and `connection-service.md` reach their final shape.

## Acceptance Criteria

- [ ] `catalog.CatalogService`, `exec_tools.ExecToolService` and `pr_stack.PrStackService` each serve
      their methods on all transports
- [ ] `connection.ConnectionService` declares exactly **17** methods — families C, D, O and Q
- [ ] **an in-jail agent executes a tool through the updated relay allowlist** — tested through a real jail
- [ ] `tddy-discovery` reaches the exec-tool coordinate through a generated client, with no
      hand-built URL anywhere in the repo
- [ ] `tddy-tool-engine` has one tool catalog, one executor and one served coordinate; the guard tests
      that compared two catalogs are gone
- [ ] `tddy-coder`'s participant serves family L at the new coordinate, and a session reached over
      LiveKit and over HTTP answers identically
- [ ] `tddy-web` lists tools and agents, inspects tool calls, and drives the PR-stack screen at the new
      coordinates
- [ ] `./test -p tddy-daemon -p tddy-tool-engine -p tddy-discovery -p tddy-workflow-recipes -p tddy-coder`
      matches baseline

## References

- Changeset: [2026-09-09-unbundle-exec-prstack-services.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-exec-prstack-services.md)
- Discovery: [2026-09-09-unbundle-exec-prstack-services-initial-discovery.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-exec-prstack-services-initial-discovery.md)
- Node 7's PRD, for the allowlist argument: [PRD-2026-09-09-session-agent-services.md](./PRD-2026-09-09-session-agent-services.md)
