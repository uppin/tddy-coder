# PRD — the RPC handlers leave `tddy-session-lifecycle` for `tddy-daemon-rpc`

**Date:** 2026-09-19 (replanned 2026-09-22)
**Type:** Refactor — handler decomposition and crate move, no wire change
**Stack:** `#carve` 11/N — the first node of the size-reduction extension
**Packages:** `packages/tddy-daemon-rpc` (new), `packages/tddy-session-lifecycle`,
`packages/tddy-pr-stack`, `packages/tddy-daemon`, `packages/tddy-workflow-recipes` (one const)
**Product area:** [`docs/ft/daemon/`](../../ft/daemon/)

## The goal this node serves

**Developer's target (2026-09-22): `tddy-session-lifecycle` and `tddy-core` each at about 10k
production lines.** In-`src/` tests do not count; they travel with the code they test. `#carve` is
extended with further nodes to get there, and tddy-core gets a node of its own.

| Crate | Production today | In-`src/` tests | Has to leave |
|---|---:|---:|---:|
| `tddy-session-lifecycle` | 22,067 | 9,597 | ~12k |
| `tddy-core` | 23,418 | 7,893 | ~13.4k |

**The developer ruled that four groups of `tddy-session-lifecycle` code do not belong in the crate,
and all four leave:**

| Group | Production lines | Node |
|---|---:|---|
| The four RPC families: PrStack (with its status and enrichment code), Project, Catalog, ExecTool | ~3,200 | **this one** |
| Session agents, rosters and clones: `svc_session_agent_ports`, `svc_provision_agent_clone`, `svc_start_hosted_agent_clone`, `svc_ensure_session_room_for_agents`, `agent_roster` | ~2,400 | a successor |
| Activity, files, terminal and demo-VM ports | ~1,600 | a successor |
| Task and action services, session admission | ~900 | a successor |

All four go to **one new crate above the lifecycle crate, `tddy-daemon-rpc`**. It depends on
`tddy-session-lifecycle` the way `tddy-daemon` does, so none of the cycles below arise. **This node
creates the crate and moves the first group into it**; the successor nodes repeat the pattern.

## Problem

`DaemonSessionHost` is one `#[derive(Clone)]` struct with **31 fields and 41 `impl` blocks**. It
implements seven RPC-family traits, and each of them needs only a handful of those fields. The wire
protocol is already split up: `session.proto`, `project.proto`, `catalog.proto`,
`exec_tools.proto` and `pr_stack.proto` are separate services, each served through a
`*ServiceImpl<H>` that is generic over its handler. The Rust type that satisfies them is not split
up. All five are built with `H = DaemonSessionHost`, and every handler body lives in
`tddy-session-lifecycle`.

## Why not move each handler to its domain crate

That was the first draft. It fails on three dependency cycles, measured on the tree and not removed
by any open `#carve` node:

| Handler | Would move to | Cycle |
|---|---|---|
| `CatalogHandler` | `tddy-discovery` | `tddy-daemon-kernel → tddy-discovery`, and the body needs `DaemonConfig` |
| `ExecToolHandler` | `tddy-tool-engine` | `tddy-daemon-sandbox → tddy-tool-engine`, and the body needs `WorkspaceSandboxRegistry` |
| `ProjectHandler` | `tddy-projects` | `tddy-daemon-livekit → tddy-worktree-service → tddy-projects`, and the body needs two LiveKit forwarders |

A crate **above** `tddy-session-lifecycle` can depend on all of those crates, so it has none of
these problems.

## What this PR delivers

### FR1 — `tddy-daemon-rpc`, holding four handler structs

The crate's normal dependency on `tddy-session-lifecycle` is one-way: `tddy-session-lifecycle` gets
no edge back, **not even a dev-dependency**. A dev-dependency would build a second copy of the
lifecycle crate into its tests, and its types would not unify.

| Struct | Serves | Field budget | Implementation it takes over |
|---|---|---:|---|
| `ProjectRpcHandler` | `tddy_projects::ProjectHandler` | 6 | `svc_project_ports.rs`, `project_coordinate_handlers.rs` |
| `CatalogRpcHandler` | `tddy_discovery::CatalogHandler` | 5 | `svc_catalog_ports.rs` |
| `ExecToolRpcHandler` | `tddy_tool_engine::ExecToolHandler` | 9 | `svc_exec_tool_ports.rs` |
| `PrStackRpcHandler` | `tddy_pr_stack::rpc::PrStackHandler` (FR3) | 7 | `svc_pr_stack_ports.rs`, the PR-stack half of `svc_pr_status_for_caller.rs` |

- Each budget is that handler's measured transitive field set. It is an upper bound: a shared
  component (FR2) bundling several fields counts as one.
- Each handler is built by `XRpcHandler::from_host(&DaemonSessionHost)`. It holds **clones of the
  host's `Arc`s**, so it shares the host's state (idle tracker, task registry, sandbox registry,
  hosted clones, token store) rather than copying it.
- **No handler holds a `DaemonSessionHost`**, either directly or behind an `Arc`.
- `RpcHandlers` bundles the four and serves them: the four `*ServiceImpl`s, their transport entries,
  and the local-socket services.
- **None of `ProjectHandler`, `ProjectService`, `CatalogHandler`, `ExecToolHandler` or
  `PrStackHandler` is implemented for `DaemonSessionHost` any more**, and the five implementation
  files leave `tddy-session-lifecycle/src`.

### FR2 — what `tddy-session-lifecycle` exposes for the handlers

The moved bodies reach two kinds of code in the lifecycle crate:

1. **Shared behaviour** that session code also uses. It becomes small `pub` components that both
   the host and the handlers hold:
   - caller identity: `resolve_os_user`, `authorize_exec_tool_caller`;
   - RPC activity: `record_rpc_activity`;
   - peer routing: `rpc_served_by_peer`, `stream_served_by_peer`,
     `classify_addressed_daemon_route`, `common_room_slot`;
   - local exec-tool execution: `run_exec_tool_locally`, `run_hosted_clone_tool`,
     `hosted_clone_for`, `resolve_exec_tool_worktree`, `exec_tool_route`;
   - agent definitions: `resolvable_agent_defs`.
2. **Free functions and modules** that are `pub(crate)` today:
   - on `connection_service`: `merge_listed_projects_with_peers`, `require_pr_stack_orchestrator`,
     `owner_repo_from_repo_root`, the `base_sync_*` helpers, the `agent_models_cache` group;
   - modules and items: `hooks_and_urls`, `service_util`, `family_proto_bridge::wire_same`,
     `session_list_enrichment::stack_plan_json_for_changeset`, `session_reader`.

   Each is either widened to `pub` or, when only a handler uses it, moved into `tddy-daemon-rpc`.
   **Moving is preferred**: it is what shrinks the crate.

### FR3 — `PrStackHandler` moves to `tddy-pr-stack`

- `PrStackHandler`, `PrStackServiceImpl` and `build_pr_stack_entry` move to `tddy_pr_stack::rpc`.
- `PR_STACK_SERVICE` moves with them, and `tddy-workflow-recipes` re-exports it.
- `tddy-pr-stack` gains `tddy-rpc` and `tddy-service` as dependencies. It already reaches
  `tddy-service` through `tddy-github`, so this adds no cycle.
- `pr_stack_rpc.rs` stays as a `pub use` facade.
- **The move is what makes the reverse edge possible (FR4):** `tddy-session-lifecycle` can name the
  trait without naming the crate that implements it.

### FR4 — the two reverse edges, through one port the composition root fills

Session code uses these families in two places. Once the bodies sit above the crate, the host can
only reach them through a port:

| Reverse use | Where | What it needs |
|---|---|---|
| A session start whose stack parent is owned by a peer, or which names a stack node | `svc_pr_status_for_caller.rs` `resolve_chain_base_ref_status`, `record_spawn_on_stack_node` | `PrStackHandler::{resolve_stack_base, link_stack_node}` |
| A session room's served roster | `session_room_roster()`, from `svc_spawn_split_agent.rs:159` and `svc_resolve_listed_worktree.rs:425` | the four families' `ServiceEntry`s |

`tddy-session-lifecycle` defines the port, **`DaemonRpcFamilies`**: one method returning an
`Arc<dyn PrStackHandler>`, one returning the families' `ServiceEntry`s. `DaemonSessionHost` holds it,
and `tddy-daemon-rpc`'s `RpcHandlers` implements it.

- **No silent omission.** A host without the port refuses those two paths with
  `FAILED_PRECONDITION`, naming the missing wiring. It does not quietly build a room that has lost
  four families, and it does not skip the stack link. The drafted `OnceLock` + `set_self_handle`
  shape is explicitly avoided: its unwired state is the documented cause of 17 failing tests (see
  the changeset's prerequisites).
- **Construction order.** The handlers clone the host's `Arc`s, and the port is installed last. So
  `runtime.rs` builds the host with every `with_*`, then builds `RpcHandlers::from_host(&host)`,
  then calls `host.with_rpc_families(..)`. A `with_*` applied after that would diverge, and the
  changeset states the order as a constraint.

### FR5 — `runtime.rs` serves the handlers

- `BinaryLocalSocketServices` names `ProjectServiceImpl<ProjectRpcHandler>`,
  `CatalogServiceImpl<CatalogRpcHandler>`, `ExecToolServiceImpl<ExecToolRpcHandler>` and
  `PrStackServiceImpl<PrStackRpcHandler>`.
- The transport entries come from `RpcHandlers`.
- `tddy-daemon` gains a `tddy-daemon-rpc` dependency.
- `DaemonSessionHost` loses `project_service`, `project_entry`, `catalog_rpc_service`,
  `catalog_entry`, `exec_tool_rpc_service`, `exec_tool_entry`, `pr_stack_rpc_service` and
  `pr_stack_entry`.

### FR6 — tests travel with the code

- The suites that exercise the four families move to `tddy-daemon-rpc/tests/` with
  `move_test_binary_to_crate` from `#carve` 4. `TestDaemon`'s four family impls move into a
  `tddy_daemon_rpc::test_util` that wraps `tddy_session_lifecycle::test_util`.
- Measured: the 16 directly named suites total **4,991 lines**. `discovery.md` §7 lists every suite
  involved.

### FR7 — a guard that sees the real wiring

A new `tddy-daemon` acceptance test builds the daemon with `runtime::build(…,
RuntimeOptions::for_binary())`, starts it, dials the Unix socket `runtime.rs` assembled, and calls
one method of each of the five families.

- It asserts that each call is answered by its handler, not by `Unimplemented`.
- It **passes before this change and must still pass after it**, so it is the regression guard
  that `local_socket_reachability_acceptance.rs` was assumed to be. That file stays unmodified.

### FR8 — no behaviour change

No `.proto`, wire signature or client changes. `SessionHandler` / `SessionService`, the three
non-RPC traits, the session and demo-VM coordinate handlers, and the dispatch layer stay.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | Each of the four handlers, built by `from_host` and served through its own `*ServiceImpl`, answers its family's RPCs as the host did: happy path and refusals |
| AC2 | A handler shares the host's state: activity recorded through a `CatalogRpcHandler` is the host's idle tracker's |
| AC3 | No `impl {ProjectHandler, ProjectService, CatalogHandler, ExecToolHandler, PrStackHandler} for DaemonSessionHost` remains, and the five implementation files are gone from `tddy-session-lifecycle/src` |
| AC4 | Nothing in `tddy-daemon-rpc/src` holds a `DaemonSessionHost` field |
| AC5 | Each handler struct declares no more fields than its budget (6 / 5 / 9 / 7) |
| AC6 | `pub trait PrStackHandler` is defined in `tddy-pr-stack` and nowhere else; `pr_stack_rpc.rs` is a facade |
| AC7 | `tddy-pr-stack` depends on neither `tddy-session-lifecycle` nor `tddy-workflow-recipes`; `tddy-session-lifecycle` has no edge to `tddy-daemon-rpc` of any kind |
| AC8 | `runtime.rs`'s `BinaryLocalSocketServices` names no `<…DaemonSessionHost>` for the four families |
| AC9 | The runtime-socket guard (FR7) is green before and after; `local_socket_reachability_acceptance.rs` is green and unmodified |
| AC10 | Every moved suite passes unedited except for `use` paths and its fixture's crate |
| AC11 | `tddy-session-lifecycle` production lines drop by **at least 2,500** from 22,067 |
| AC12 | **Every crate that receives code in this PR stays at or under 10,000 production lines**: `tddy-daemon-rpc`, `tddy-pr-stack`, `tddy-daemon`. The same check is repeated by every successor node for the crates it feeds |

**The 10k cap across the stack** (developer's decision, 2026-09-22):
- Each node holds every crate it moves code into to ≤10k, and shrinks the crate it moves code out of.
- `tddy-session-lifecycle ≤ 10k` is an acceptance criterion of the stack's **last lifecycle node**,
  not of this one.
- The capacity plan, and the second crate above that the split-session and sandboxed-start cluster
  needs, are in the changeset's `## Successor PRs`.

## Out of scope

- The other three groups (session agents, rosters and clones; activity, files, terminal and
  demo-VM ports; task and action services and admission). Successor nodes move them into the same
  crate.
- `tddy-core`, which gets a successor node of its own.
- The complexity of what moves. The three `svc_exec_tool_ports` code issues follow their file.
- `relaunch_sandboxed_runner` and `split_context_from_codebase_host`. They are untested and sit
  next door, and nothing here touches them.
