# tddy-daemon-rpc architecture

## Overview

Four of the daemon's RPC families are served by handler structs in this crate. Each family's wire
protocol is its own service, served through a `*ServiceImpl<H>` that is generic over its handler;
this crate supplies the `H`.

| Handler | Coordinate | Trait it implements (defined in) | RPCs |
|---|---|---|---|
| `ProjectRpcHandler` | `project.ProjectService` (family D) | `ProjectHandler` (`tddy-projects`) | `ListProjects`, `CreateProject`, `AddProjectToHost`, `ListProjectBranches`, `SetProjectDefaultBranch` |
| `CatalogRpcHandler` | `catalog.CatalogService` (family A) | `CatalogHandler` (`tddy-discovery`) | `ListTools`, `ListAgents`, `ListAgentModels`, `ListSubagents` |
| `ExecToolRpcHandler` | `exec_tools.ExecToolService` (family L) | `ExecToolHandler` (`tddy-tool-engine`) | `ExecuteTool`, `StreamExecuteTool`, `ListExecTools`, `ListSessionToolCalls` |
| `PrStackRpcHandler` | `pr_stack.PrStackService` (family P) | `PrStackHandler` (`tddy-pr-stack`) | `AddPlannedPr`, `GetPrStatus`, `QueryBranch`, `ResolveStackBase`, `LinkStackNode`, `RepointPlannedPr`, `ReorderPlannedPr`, `PullBaseIntoBranch` |

The session family (`session.SessionService`), the session-files, session-agents, activity,
terminal and demo-VM families, and the dispatch layer (`daemon_rpc_handler`, `family_proto_bridge`)
are served from `tddy-session-lifecycle`.

## Why a crate above the lifecycle crate

Each handler's body needs session-host state *and* crates that sit between the lifecycle crate and
the family's domain crate. Moving a handler into its domain crate closes a dependency cycle:

| Handler | Domain crate | Cycle it would close |
|---|---|---|
| `CatalogHandler` | `tddy-discovery` | `tddy-daemon-kernel → tddy-discovery`; the body needs `DaemonConfig` |
| `ExecToolHandler` | `tddy-tool-engine` | `tddy-daemon-sandbox → tddy-tool-engine`; the body needs `WorkspaceSandboxRegistry` |
| `ProjectHandler` | `tddy-projects` | `tddy-daemon-livekit → tddy-worktree-service → tddy-projects`; the body needs two LiveKit forwarders |

A crate that depends on `tddy-session-lifecycle` can depend on all of those, so it has none of these
cycles.

**The edge is one-way.** `tddy-session-lifecycle` has no dependency on this crate of any kind —
not normal, not dev. A dev-dependency would build a second copy of the lifecycle crate into its
tests, and the two copies' types would not unify. Session code reaches these families only through
the `DaemonRpcFamilies` port (below).

## The handlers

Each handler is built by `XRpcHandler::from_host(&DaemonSessionHost)` and holds only the fields its
family reads. Shared state is held as **clones of the host's `Arc`s**, or of the lifecycle crate's
shared components built on them, so a handler reads and writes the very idle tracker, task registry,
workspace jails, hosted clones, peer roster and token store the host does. **No handler holds a
`DaemonSessionHost`**, directly or behind an `Arc`.

| Handler | Fields | Budget |
|---|---|---:|
| `ProjectRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `eligible_daemon_source`, `spawn_client`, `common_room_livekit_room` | 6 |
| `CatalogRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `model_registry`, `rpc_activity` | 5 |
| `ExecToolRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `peer_routing`, `rpc_activity`, `local_exec_tools` | 9 |
| `PrStackRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `github_token_store`, `rpc_activity`, `peer_routing` | 7 |

The budget is each family's measured transitive field set on the host, as an upper bound; a shared
component that bundles several host fields counts as one. `DaemonConfig` is never mutated once the
host is built, so a handler's clone of it cannot diverge.

The shared components (`RpcActivity`, `PeerRouting`, `LocalExecTools`) and the caller-identity,
tool-path and agent-definition free functions the handlers call are defined in
`tddy-session-lifecycle`; see its [session-service.md](../../tddy-session-lifecycle/docs/session-service.md#rpc-families-served-above-this-crate).

### Module layout

| Module | Holds |
|---|---|
| `project` | the struct; `ports.rs` (`ProjectHandler`), `coordinate_handlers.rs` (the RPC bodies), `entries.rs` (project row → `ProjectEntry`, `merge_listed_projects_with_peers`, default-remote resolution), `clone_destination.rs` (where a clone under the caller's home lands) |
| `catalog` | the struct; `ports.rs` (`CatalogHandler`), `agent_models.rs` (the `tddy-tools list-models` probe, its parse and the short-lived cache in front of it), `subagent_row.rs` (resolved agent def → `ListSubagents` row) |
| `exec_tool` | the struct; `ports.rs` (`ExecToolHandler`), `path_guard.rs` (the traversal check every path-bearing tool's arguments pass before any I/O), `result_frames.rs` (`StreamExecuteTool` framing, `EXEC_TOOL_FRAME_BYTES`) |
| `pr_stack` | the struct; `ports.rs` (`PrStackHandler`), `guards.rs` (`require_pr_stack_orchestrator`, `validate_repoint_target` — the refusals a mutation runs before it touches an orchestrator's plan), `pr_status.rs` (the PR leg of a branch resolution, read with the calling operator's GitHub credential), `branch_legs.rs` (the base-sync and worktree legs `QueryBranch` / `PullBaseIntoBranch` report) |
| `families` | `RpcHandlers` and its `DaemonRpcFamilies` implementation |
| `test_util` | the test fixture (below) |

Behaviour at the wire is the lifecycle host's contract: `ExecuteTool` routes to the owning peer
before it authenticates; each family's `RpcActivity` record counts are Catalog 1, ExecTool 3,
PrStack 2, Project 0 per call path; error codes, messages, timeouts and forwarding names are those
the product docs specify.

## `RpcHandlers`

`RpcHandlers` bundles the four handlers, each behind an `Arc`. `Clone` shares them: a clone serves
the same handlers as the original.

| Method | Returns |
|---|---|
| `from_host(&host)` | every handler, built from `host`'s state |
| `install(host) -> (host, handlers)` | `from_host`, then `host.with_rpc_families(Arc::new(handlers.clone()))`; returns both, so the composition root serves the same handlers on its own transports |
| `project_service()` · `catalog_service()` · `exec_tool_service()` · `pr_stack_service()` | `ProjectServiceImpl<ProjectRpcHandler>`, `CatalogServiceImpl<CatalogRpcHandler>`, `ExecToolServiceImpl<ExecToolRpcHandler>`, `PrStackServiceImpl<PrStackRpcHandler>` |
| `entries()` | the four families' `ServiceEntry`s, built with `build_catalog_entry`, `build_exec_tool_entry`, `build_project_entry` and `build_pr_stack_entry` |

**Install order is the contract.** Every handler clones the host's `Arc`s when it is built, so
`RpcHandlers::install` runs **after the host's last `with_*`**: a `with_*` applied later would
replace a value the handlers still hold, splitting the host from its handlers. The lifecycle crate
guards this with a `debug_assert` in the setters that feed handler state (`with_model_registry`,
`with_github_token_store`, `with_idle_tracker`, `with_eligible_daemon_source`). The host keeps a
clone of the bundle, which holds handlers and never the host, so there is no `Arc` cycle.

### The `DaemonRpcFamilies` implementation

`impl DaemonRpcFamilies for RpcHandlers`:

- `pr_stack_handler()` returns the bundle's `PrStackRpcHandler` as `Arc<dyn PrStackHandler>`. Session
  start's peer-owned stack-base resolution and named-node link go through it, so they take the same
  path — peer routing and refusals included — as a client's `ResolveStackBase` / `LinkStackNode`.
- `service_entries()` returns `entries()`, which a session room's roster serves beside the families
  the lifecycle crate serves itself.

## The composition root

`tddy-daemon`'s `runtime::build` builds the host with every `with_*`, calls
`RpcHandlers::install(host)`, and takes the four local-socket services and the four transport
entries from the returned handlers. Its `BinaryLocalSocketServices` names the four handler types.
See [daemon-endpoint.md](../../tddy-daemon/docs/daemon-endpoint.md).

## Tests

**`test_util`** wraps `tddy_session_lifecycle::test_util`:

- `TestDaemon` — a lifecycle `TestDaemon` with this crate's `RpcHandlers` installed as its
  `DaemonRpcFamilies` (what `runtime::build` assembles), answering the four families through those
  same handlers. `from_host(host)` installs the handlers on a host that already carries every
  `with_*` the suite wants; `serving(lifecycle_test_daemon)` serves them for a host already behind
  its `Arc` (one a suite installed itself and also serves over LiveKit), built from that same host
  so every `Arc` is shared. `handlers()` exposes the bundle; `with_roster_keepalive_interval` and
  `with_workspace_sandbox_provisioner` forward to the lifecycle fixture.
- `test_service(sessions_base)` — the default fixture: the lifecycle `test_host` with the handlers
  installed and the sandbox RPC bridge set up.

`pub mod test_util` is compiled unconditionally.

**Suites.** `tests/` holds 35 integration suites: the four families' behaviour suites — the project,
catalogue, session-agent roster, exec-tool (workspace jail, seatbelt, hosted clone, session room,
tool-call log) and PR-stack (planned-PR mutation, query-branch, repoint, cross-host stack parent,
link-stack-node) suites — plus two that pin the crate itself:

| Suite | Asserts |
|---|---|
| `rpc_handlers_acceptance.rs` | each handler, built by `from_host` and served through its own `*ServiceImpl`, answers its family (happy path and refusals); a `CatalogRpcHandler` records activity on the host's own idle tracker |
| `rpc_handlers_shape.rs` | no `impl` of the five family traits for `DaemonSessionHost` and the implementation files absent from the lifecycle crate; no handler holds the host; each handler within its field budget; `PrStackHandler` defined only in `tddy-pr-stack`, which serves its own family and depends on neither the lifecycle crate nor the recipes; the lifecycle crate has no edge to this one; `runtime.rs` names the four handler types; the lifecycle crate at least 2,000 production lines under its 22,067 baseline; every crate receiving code (`tddy-daemon-rpc`, `tddy-pr-stack`, `tddy-daemon`) at or under 10,000 production lines |

The shape suite's production-line measure is the lines of each `src/` file before the first
`#[cfg(test)]` that opens a `mod`, with `*_tests.rs`, `tests.rs` and `test_util.rs` excluded. The
manifest and source readers fail loudly, naming the path, rather than passing on an unreadable file.

Unit tests beside the code: `catalog/agent_models/{parse,probe}_tests.rs` and
`pr_stack/add_planned_pr_unit_tests.rs`.

`session_room_exec_tool_acceptance` drives a real LiveKit server and is in `.config/nextest.toml`'s
LiveKit serial group.
