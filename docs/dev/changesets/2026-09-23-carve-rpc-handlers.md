# 2026-09-23 — Four RPC families leave the session host for `tddy-daemon-rpc`

**Type:** Architecture

`#carve` 11/12, PR [#520](https://github.com/uppin/tddy-coder/pull/520) — the first node of the
stack's size-reduction extension. Builds on `#carve` 10/11 pr-stack-crate
([#496](https://github.com/uppin/tddy-coder/pull/496)), whose `tddy-pr-stack` crate received the
PR-stack RPC trait here. Dependent: `#carve` 12/12, tddy-core
([#522](https://github.com/uppin/tddy-coder/pull/522)).

`DaemonSessionHost` was one `#[derive(Clone)]` struct with 31 fields and 41 `impl` blocks,
implementing seven RPC-family traits whose bodies all lived in `tddy-session-lifecycle`. This change
created **`packages/tddy-daemon-rpc`**, a crate **above** the lifecycle crate, and moved the
Project, Catalog, ExecTool and PR-stack families into it as four handler structs, each built from the
host by `from_host` and holding only the fields its family reads. No `.proto`, wire signature or
client changed.

Permanent docs: [tddy-daemon-rpc architecture](../../../packages/tddy-daemon-rpc/docs/architecture.md),
[session-service.md § RPC families served above this crate](../../../packages/tddy-session-lifecycle/docs/session-service.md#rpc-families-served-above-this-crate),
[tddy-pr-stack architecture § `rpc.rs`](../../../packages/tddy-pr-stack/docs/architecture.md#rpcrs--the-pr-stack-rpc-family),
[daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md),
[session-room.md § The hosting task](../../../packages/tddy-daemon-livekit/docs/session-room.md#the-hosting-task).

## What changed

| Package | Change |
|---|---|
| `tddy-daemon-rpc` (new) | `ProjectRpcHandler`, `CatalogRpcHandler`, `ExecToolRpcHandler`, `PrStackRpcHandler`; `RpcHandlers` (`from_host`, `install`, the four `*ServiceImpl`s, `entries()`) and `impl DaemonRpcFamilies`; `test_util::TestDaemon`; 35 suites |
| `tddy-session-lifecycle` | lost `svc_project_ports.rs`, `project_coordinate_handlers.rs`, `svc_catalog_ports.rs`, `svc_exec_tool_ports.rs`, `svc_pr_stack_ports.rs`, `svc_family_entries.rs`, the PR-stack half of `svc_pr_status_for_caller.rs`, and `TestDaemon`'s four family impls; gained the `DaemonRpcFamilies` port (`rpc_families.rs`), the `pub` shared components `RpcActivity`, `PeerRouting`, `LocalExecTools`, the free functions `resolve_os_user`, `authorize_exec_tool_caller`, `resolve_exec_tool_worktree`, `resolve_tddy_tools_path`, `resolvable_agent_defs`, the `handler_state.rs` accessors, and `test_util::RpcFamiliesNotUnderTest`; `pr_stack_rpc.rs` became a facade |
| `tddy-pr-stack` | gained `rpc` (`PrStackHandler`, `PrStackServiceImpl`, `build_pr_stack_entry`, `PR_STACK_SERVICE`) and the `tddy-rpc` / `tddy-service` dependencies |
| `tddy-workflow-recipes` | `PR_STACK_SERVICE` became `pub use tddy_pr_stack::PR_STACK_SERVICE` |
| `tddy-daemon` | `runtime::build` calls `RpcHandlers::install` after every `with_*` and takes the four local-socket services and entries from it; `BinaryLocalSocketServices` names the four handler types; new guard `tests/local_socket_family_wiring_acceptance.rs`; seven cross-host / relay suites plus `local_token_uds.rs` and `staging_forwarding_acceptance.rs` build their families from `RpcHandlers` |
| `tddy-daemon-livekit` | `SessionRoomRegistry::{ensure_open, open_measured_by}` take the room's service list as a builder, called after the LiveKit-credentials check |
| `tddy-tool-engine` | `tests/tool_call_log_acceptance.rs` moved to `tddy-daemon-rpc` with the ExecTool family |
| `tddy-daemon-kernel` | doc comment only: `user_paths.rs` names the new home of `project_path_under_home_from_user_relative` |
| `.config/nextest.toml` | `session_room_exec_tool_acceptance` joined the LiveKit serial group under its new package |

## Decisions

- **A crate above, not the families' domain crates** (developer, 2026-09-22). Moving each handler
  into `tddy-discovery`, `tddy-tool-engine` or `tddy-projects` closes three dependency cycles
  (`tddy-daemon-kernel → tddy-discovery`, `tddy-daemon-sandbox → tddy-tool-engine`,
  `tddy-daemon-livekit → tddy-worktree-service → tddy-projects`). A crate above the lifecycle crate
  has none of them and needs no kernel port.
- **Widened from an in-place split** ("Widen #520") and **kept as one node** rather than split along
  the port: an in-place split would have left the crate's size unchanged.
- **One port, pointing down.** `DaemonRpcFamilies` is defined in the lifecycle crate and filled by
  the composition root. It refuses with `FAILED_PRECONDITION` when unwired rather than using a
  late-bound `OnceLock` + `set_self_handle`, whose unwired state is the recorded cause of seventeen
  failing tests (backlog `2026-09-09-daemon-sandbox-suites-never-call-set-self-handle`, left open).
- **Move rather than widen.** A `pub(crate)` item only a handler used moved into `tddy-daemon-rpc`
  (`merge_listed_projects_with_peers`, `require_pr_stack_orchestrator`, `owner_repo_from_repo_root`,
  the `base_sync_*` helpers, the `agent_models_cache` group, the project-entry helpers from
  `hooks_and_urls`, `project_path_under_home_from_user_relative`). Only behaviour shared with session
  code became `pub`.
- **The runtime-socket guard is green by design** — a regression guard for a refactor, passing before
  and after. `local_socket_reachability_acceptance.rs` stayed unmodified and green.
- **AC11 re-baselined from 2,500 to 2,000 lines shed** (developer, 2026-09-23), after all four
  families had moved and measured 2,009. The 2,500 estimate counted helpers the families share with
  session code (peer routing, caller identity, local exec tools, agent definitions), which became
  `pub` components and stay in the lifecycle crate. The stack-level target of about 10k production
  lines for `tddy-session-lifecycle` is unchanged; successor nodes carry the rest.

## Measurements

Production lines: the lines of each `src/` file before the first `#[cfg(test)]` that opens a `mod`,
with `*_tests.rs`, `tests.rs` and `test_util.rs` excluded — the rule
`packages/tddy-daemon-rpc/tests/rpc_handlers_shape.rs` applies.

| Crate | Before (master `77187dbe`) | After (`88c5eaff`) | Cap |
|---|---:|---:|---|
| `tddy-session-lifecycle` | 22,067 | **20,067** (−2,000) | shed ≥ 2,000 (AC11) |
| `tddy-daemon-rpc` | — | 2,905 | ≤ 10,000 (AC12) |
| `tddy-pr-stack` | 2,916 | 3,077 | ≤ 10,000 (AC12) |
| `tddy-daemon` | 3,321 | 3,313 | ≤ 10,000 (AC12) |

The lifecycle crate measured 20,058 (−2,009) when AC11 was re-baselined; the validation fixes below
added nine lines to it (the `debug_assert` wiring guard among them), leaving it exactly at the
2,000-line criterion.

Tests: `tddy-session-lifecycle/tests` went from 83 to 59 suites (25 moved whole, one added:
`rpc_families_port_acceptance.rs`); `tddy-daemon-rpc/tests` holds 35 — the moved suites, the
family halves split out of mixed lifecycle suites and in-`src/` test modules, `tool_call_log_acceptance.rs`
from `tddy-tool-engine`, and the two new suites `rpc_handlers_acceptance.rs` (AC1–AC2) and
`rpc_handlers_shape.rs` (AC3–AC8, AC11–AC12). Moved test bodies were compared old against new by
script: no assertion changed or dropped.

## Validation findings closed

| Finding | Resolution |
|---|---|
| **C1** (critical) — `tddy-tool-engine/tests/tool_call_log_acceptance.rs` stopped compiling once the ExecTool `TestDaemon` impl left the lifecycle crate; also the red "Rust lint" CI check | moved to `tddy-daemon-rpc/tests/`, imports only; no other crate dev-depending on the lifecycle crate was affected |
| **W1** — `session_room_roster()?` was evaluated before the LiveKit-credentials check, so an unwired host with no LiveKit refused `ConnectSession` instead of opening no room | the room's service list became a builder called only once a room will open; `sandboxed_claude_cli_connect_session_returns_empty_livekit` went from `FailedPrecondition` to passing |
| **W2** — "install the port last" was enforced only by comments | `debug_assert_rpc_families_not_installed` in `with_model_registry`, `with_github_token_store`, `with_idle_tracker` and `set_eligible_daemon_source` |
| **W3** — shape tests could pass vacuously on an unreadable manifest | `source_of` / `normal_dependencies_of` fail loudly, naming the path |

## Code issues

- **Moved, not closed** — renamed into `packages/tddy-daemon-rpc/docs/code-issues/` with a
  `**Moved:**` line: `complexity-exec-tool-ports-list-exec-tools` (73 → 78 lines, re-wrapped paths),
  `complexity-exec-tool-ports-list-session-tool-calls` (94 → 101),
  `complexity-exec-tool-ports-stream-execute-tool` (73 → 82),
  `complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate` (178 → 174).
  Nesting and early exits unchanged in all four.
- **Narrowed** — `tddy-session-lifecycle`'s `oversized-file-connection-service`: ~1,931 → ~1,634
  production lines by the record's own count (total 1,945 → 1,647), still 3.3× over budget.
- **Grown by rustfmt re-wraps** of field reads that moved behind `PeerRouting`, and recorded:
  `start_session_core` 854 → 857 and `svc_start_session_core.rs` 908 → 911;
  `ensure_project_available_for_start` 157 → 158.
- **Unchanged, recorded**: `runtime.rs::build` 833; `spawn_split_agent` 251;
  `svc_spawn_split_agent.rs` 520. `runtime.rs` went 1,521 → 1,513.

## Backlog

No entry was resolved. Two were answered in part and kept:
`2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc`
(`require_pr_stack_orchestrator` lives in `tddy-daemon-rpc`'s `pr_stack` module; the trim-to-`Option`
duplication remains) and `2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use` (the shape
tests cut at the first `#[cfg(test)]` that opens a `mod`; the `/pr-wrap` gate is unchanged).
