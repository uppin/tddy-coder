# tddy-daemon-rpc architecture

## Overview

Four of the daemon's RPC families are served by handler structs in this crate. Each family's wire
protocol is its own service, served through a `*ServiceImpl<H>` that is generic over its handler;
this crate supplies the `H`.

| Handler | Coordinate | Trait it implements (defined in) | RPCs |
|---|---|---|---|
| `ProjectRpcHandler` | `project.ProjectService` (family D) | `ProjectHandler` (`tddy-projects`) | `ListProjects`, `CreateProject`, `AddProjectToHost`, `ListProjectBranches`, `SetProjectDefaultBranch`, `SetProjectAccounts`, `SetProjectHomeAgent` |
| `CatalogRpcHandler` | `catalog.CatalogService` (family A) | `CatalogHandler` (`tddy-discovery`) | `ListTools`, `ListAgents`, `ListAgentModels`, `ListSubagents` |
| `ExecToolRpcHandler` | `exec_tools.ExecToolService` (family L) | `ExecToolHandler` (`tddy-tool-engine`) | `ExecuteTool`, `StreamExecuteTool`, `ListExecTools`, `ListSessionToolCalls` |
| `PrStackRpcHandler` | `pr_stack.PrStackService` (family P) | `PrStackHandler` (`tddy-pr-stack`) | `AddPlannedPr`, `GetPrStatus`, `QueryBranch`, `ResolveStackBase`, `LinkStackNode`, `RepointPlannedPr`, `ReorderPlannedPr`, `PullBaseIntoBranch` |

The session family (`session.SessionService`), the session-files, session-agents, activity,
terminal and demo-VM families, and the dispatch layer (`daemon_rpc_handler`, `family_proto_bridge`)
are served from `tddy-session-lifecycle`.

## Code navigation

`CodeNavigationServiceImpl` serves `code_navigation.CodeNavigationService` (the code pane's
`CodeSession`, the unary go to definition / references / hover, the header's `WatchCodeIndex`, and the
plan calls). It is not a `from_host` family: it holds the daemon's `WorktreeServiceImpl`, whose
`resolve_listed_worktree` authorises every request, and an optional `Arc<dyn IndexChannelSource>` — the
port through which it reaches the warm code-intelligence index. The daemon implements the port for the
registry that manages the index daemon process, so this crate never names that registry and
`tddy-daemon` keeps only wiring. Without a source every method answers `FailedPrecondition`; there is no
fallback to another language server. The request path and the end-to-end tests are in
[tddy-daemon's code-navigation-service.md](../../tddy-daemon/docs/code-navigation-service.md).

**`IndexChannelSource`**

| Method | Contract |
|---|---|
| `connect()` | a channel to the index daemon, starting it if nothing has yet — the forwarding path, and what refreshes the process's idle timer |
| `connect_if_running()` | a channel only if the index daemon is already running, `None` otherwise; never starts it and does not refresh its idle timer — for a passive watcher |
| `keep_alive()` | counts a long-lived holder of a `connect` channel as use of the running index daemon, so it is not stopped for sitting idle; never starts it |
| `keep_alive_every()` | how often such a holder calls `keep_alive` (the registry answers a third of its idle timeout, clamped to 100 ms–60 s) |

**Unary navigation.** `Definition`, `References` and `Hover` authorise and dial through
`authorise_and_connect`, put the question in the index's terms (`code_navigation/position_query.rs`,
`PositionQuery::{definition, references, hover}`) and pass the index's answer back with its `state`
(`code_navigation/convert.rs`: `web_definition`, `web_references`, `web_hover`, `web_locations`,
`web_progress`, `web_state`). The same query and converters serve the `CodeSession`.

**`WatchCodeIndex(session)`** — the header's passive follow. It authorises through the worktree service's
`resolve_owned_session_dir` — token → OS user → `<sessions base>/sessions/<id>` must exist — before
reading anything, so a foreign session and a missing one answer the same `NotFound`, and a progress
`error` (which can carry paths) is never reachable by another user. It then reads the session's worktree
from its `.session.yaml` (`tddy_core::read_session_metadata`; an unreadable record is
`FailedPrecondition`) and follows `code_index.WatchRoot` for it, converting each message, until the
client leaves or the index ends the stream. It never starts a load nor the index daemon: a session whose
record names no worktree yet, a worktree that is not a Rust workspace (`tddy_index_daemon::is_rust_workspace`),
a daemon without an `index_daemon:` section, or an index daemon not already running (`connect_if_running`
answers `None`) ends the stream at once with no message — the web re-watches with backoff. The record is
read directly rather than through `resolve_worktree_root_in_session_dir`, because that helper reports "no
record" and "no worktree recorded yet" as the same `FailedPrecondition`, and here the second is an empty
stream. Streams buffer eight messages (`WATCH_BUFFER`).

### `CodeSession`

`code_session.rs` (with `code_session/ask.rs`, `keep_alive.rs`, `without_an_index.rs`) serves the code
pane's bidirectional `CodeSession`.

- **Open.** The first frame must be `open {session_token, project_id, worktree_path}`, authorised with
  `resolve_listed_worktree`. A query before `open`, a second `open`, or an empty frame ends the stream
  `INVALID_ARGUMENT` (`misplaced_frame`). Opening dials `connect`, calls `code_index.Warm` for the
  worktree — drained through `drain_warm`, a trigger only; a refused warm is logged, since `WatchRoot`
  reports the root `FAILED` with the reason — and then follows `code_index.WatchRoot`, relaying every
  message as a `progress` event for as long as the stream is open. `WatchRoot` ending ends the session
  `UNAVAILABLE`.
- **No index.** A worktree the index does not serve (`without_an_index.rs`) — no `Cargo.toml` at its root,
  and no file an installed extension language with a configured server claims (`has_an_index`, asking
  `tddy_index_daemon::serves_workspace` over the registry `CodeNavigationServiceImpl::with_supports` and
  `IndexWarmupObserver::with_supports` were given; the host hands both `code_navigation::host_supports()`) —
  neither dials the
  index daemon nor warms or follows anything: it sends one `NONE` progress carrying why, and answers every
  query with that `error`.
- **Queries.** Each query is asked of the index on a task of its own (`ask.rs`), so a slow hover never
  delays a probe; every query also calls `keep_alive`. A hover, definition or references the index
  answers `LOADING` is **held**: the session sends `held {request_id}` and asks again once the root
  reports `READY` *after* the query was asked (a progress counter, so a stale `READY` never spins a re-ask
  loop and a `READY` that raced the `LOADING` answer is not missed). A root that reports `FAILED` answers
  every held query with `error: "the code index failed: <reason>"`; a held query stays held through
  `IDLE`. A **probe** never waits: `LOADING` → `UNKNOWN` (no locations, even if the index returned some);
  otherwise `DEFINABLE` with the definition's locations, or `NONE`. A query reusing a `request_id`
  replaces the earlier one; `cancel` drops a query held or in flight, so no answer is sent for it.
- **One loop.** Every event is sent from the one loop that owns the session's state, which reads the pane's
  frames first (`biased`), so a `cancel` sent before the index became ready is applied before the held
  query is released.
- **Keep-alive** (`keep_alive.rs`). An open session is use of both the index-daemon process and its
  root's language server. A heartbeat every `keep_alive_every()` calls `keep_alive` and, when the root is
  `READY` or `LOADING` and no warm of this session is in flight, calls `Warm` again — idempotent, and
  reaching the root's server through `WorkspaceIndex::client_for` is what the index daemon records as the
  root's use. An `IDLE` or `FAILED` root is not warmed by the heartbeat. Should the root be let go anyway —
  it reports `IDLE` after it held a server since the session's last warm (`served_since_warmed`) — the
  session warms it again; an `IDLE` before that starts nothing. A forgotten open tab therefore keeps
  rust-analyzer and the index daemon resident until it closes.

## Restructure plans

`code_navigation/plan.rs` serves the plan half of `CodeNavigationService` (`OpenPlan`, `WatchPlan`,
`RunPlan`). Every call authorises and dials through the same `authorise_and_connect` the navigation calls
use, so a plan file is reachable only where the worktree service would let it be read, and the same
`FailedPrecondition` / `Unavailable` answers apply without an `index_daemon:` section or a reachable
index.

The index daemon answers less than the dialog lists: `LoadPlans` carries an operation count and the stale
list, `PlanStatus` journal counts. So the rows come from the plan file:

| Step | What it does |
|---|---|
| `read_plan_rows` | reads the file through `read_worktree_file_utf8` (the listed-file gate and size cap of `WorktreeService`; a truncated read is refused), parses it with `tddy-code-restructuring`'s `Plan::parse`, assigns the ids the store assigns (`op-<n>`), and keeps each operation's id, kind, anchor item and file, and group |
| `open_snapshot` | `LoadPlans` for the worktree, then `current_snapshot` |
| `current_snapshot` | `PlanStatus`, then `snapshot_of` |
| `snapshot_of` | lays the journal counts over the rows in plan order — completed `APPLIED`, then in flight, then `FAILED`, the rest `PENDING` — and sets each row's `stale_reason` from the store's stale list by op id. Exact for a plan run from its start; approximate for one whose journal does not describe a prefix of the plan |

The plan file is read once per call, and its rows are reused by the watch and run loops. `WatchPlan`
sends the opening snapshot, then `follow` polls `current_snapshot` every second (the store has no change
feed) and sends a snapshot only when it differs from the last; a store failure is the stream's last item.
Streams buffer eight messages (`WATCH_BUFFER`).

`RunPlan` calls `code_index.Apply` with `dry_run`, `resume`, `from` and `stop_after` all unset, so a run
is always the whole plan. `relay_run` maps the index's events: an applied operation, a note and the
outcome carry over; indexing progress and findings are dropped. The outcome and a failure are terminal.
A transactional group that does not compile reaches this service only as a `FailedPrecondition` whose
message begins ``group `<name>` does not compile``; `run_failure` reads the group name from that wording
and lists the ids of every row in that group as `rolled_back`, since the group is restored together. Any
other stream error is a `failure` with no group.

## Code index warm-up

`code_index_warmup.rs` starts a session's code-index load in the background once its worktree exists. It
is a **trigger** only: what the index is doing is kept per worktree root in the index daemon, which
`WatchCodeIndex` and `CodeSession` follow through `WatchRoot`, whoever started the load. Nothing is
recorded here.

| Item | What it is |
|---|---|
| `warm_for_session(index_daemon, session_id, worktree)` | starts `code_index.Warm` for `worktree` on its own task and returns its `JoinHandle`, or `None` when nothing was started: no `IndexChannelSource`, or not a Rust workspace. The warm dials through `IndexChannelSource::connect`, which starts the index daemon on first use, and drains the stream (`drain_warm`). A failed or short warm (the stream ends before `ready`) is logged; it never fails the session |
| `drain_warm(stream)` | reads a `Warm` stream to its end and answers whether its last message said `ready`; shared with `CodeSession` |
| `IndexWarmupObserver` | the implementation of `tddy-session-lifecycle`'s [`SessionWorktreeObserver`](../../tddy-session-lifecycle/docs/session-service.md#the-worktree-observer-port) port: `worktree_ready` calls `warm_for_session` and detaches the task |

"Ready" is the index daemon's `Warm` `ready`, which waits for the graph; the registry's own readiness
(its socket is bound) is not used for it. Without an `index_daemon:` section nothing warms. A session
started by a path that does not announce its worktree (see the session host's not-covered list) is not
warmed at start; its index loads when its Code pane opens or its agent first calls a code tool.

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
| `ProjectRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `eligible_daemon_source`, `spawn_client`, `common_room_livekit_room`, `model_registry` (the home-agent catalog check) | 7 |
| `CatalogRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `model_registry`, `rpc_activity` | 5 |
| `ExecToolRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `peer_routing`, `rpc_activity`, `local_exec_tools` | 9 |
| `PrStackRpcHandler` | `config`, `user_resolver`, `tddy_data_dir`, `credential_vaults`, `rpc_activity`, `peer_routing` | 7 |

The budget is each family's measured transitive field set on the host, as an upper bound; a shared
component that bundles several host fields counts as one. `DaemonConfig` is never mutated once the
host is built, so a handler's clone of it cannot diverge.

The shared components (`RpcActivity`, `PeerRouting`, `LocalExecTools`) and the caller-identity,
tool-path and agent-definition free functions the handlers call are defined in
`tddy-session-lifecycle`; see its [session-service.md](../../tddy-session-lifecycle/docs/session-service.md#rpc-families-served-above-this-crate).

### `RepointPlannedPr` and the GitHub token

`RepointPlannedPr` resolves the project's account itself, not through a login-keyed credential: the
orchestrator's project is read from its `.session.yaml`, its `accounts` from the project row, and
`project_github_token(vaults, user_resolver, caller's token, accounts)` runs one `acting_identity` over
the caller's own vault
([resolution](../../tddy-accounts/docs/github-identity-resolution.md)). Only a node that **owns a
branch** needs GitHub (it has a PR to re-target); a plan-only repoint reaches no GitHub call, so it is
never refused and never opens the vault (`guards::token_for_repoint`). A refusal is
`FAILED_PRECONDITION` carrying the resolver's words, returned **before** the plan is rewritten, so a
refused repoint changes nothing.

`RepointPlannedPr` of a **Local gh** session (its recorded `github-source.yaml`, else the project row's
`local_gh`) has no vault to open: the repoint closure calls
`local_gh_github_token(&GithubCli, os_user, git_url)`, asking the host's `gh` as the session's OS user,
with the same refusal-before-rewrite behaviour. `SetProjectAccounts` maps `local_gh` both ways and
returns the storage refusal of a GitHub assignment together with it as `InvalidArgument`
(`entries::project_entry_from` maps it onto `ProjectEntry`); there is no separate toggle RPC.
`GetHostTooling` passes `github_hostname` to the probe.

### Module layout

| Module | Holds |
|---|---|
| `project` | the struct; `ports.rs` (`ProjectHandler`), `coordinate_handlers.rs` (the RPC bodies), `coordinate_route.rs` (`coordinate_route`: local or forwarded to the named peer — the one routing rule every per-host setter uses), `home_agent_handler.rs` (`SetProjectHomeAgent`: route, catalog check, write), `entries.rs` (project row → `ProjectEntry`, `merge_listed_projects_with_peers`, default-remote resolution), `clone_destination.rs` (where a clone under the caller's home lands) |
| `catalog` | the struct; `ports.rs` (`CatalogHandler`), `agent_models.rs` (the `tddy-tools list-models` probe, its parse and the short-lived cache in front of it), `subagent_row.rs` (resolved agent def → `ListSubagents` row), `agent_rows.rs` (`listed_agents`: the `ListAgents` rows — allowlist, model-registry assistants, harness profiles — shared with the home-agent check) |
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
`with_credential_vaults`, `with_idle_tracker`, `with_eligible_daemon_source`). The host keeps a
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
