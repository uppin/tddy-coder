# Changeset: carve-rpc-handlers

**Date**: 2026-09-19 (replanned 2026-09-22)
**Status**: 🚧 In Progress — green; validation pending
**Type**: Refactor (handler decomposition and crate move, no wire change)
**Stack**: `#carve` 11/12 — the first node of the size-reduction extension; `#carve` 12/12 (#522, tddy-core) sits above it

## Initial Discovery

[2026-09-19-carve-rpc-handlers-initial-discovery.md](./2026-09-19-carve-rpc-handlers-initial-discovery.md)
— three passes: the drafted premises (most failed), the open `#carve` nodes' deliveries, and the
widened scope with its production-line measurements and reverse edges.

## Related documentation

- PRD: [2026-09-19-carve-rpc-handlers-prd.md](./2026-09-19-carve-rpc-handlers-prd.md)

## Affected Packages

- **`tddy-daemon-rpc`** (new): the four handler structs, `RpcHandlers`, the `DaemonRpcFamilies`
  implementation, and the four families' moved suites.
- **`tddy-session-lifecycle`**
  - loses five implementation files and the PR-stack half of `svc_pr_status_for_caller.rs`;
  - gains the `DaemonRpcFamilies` port and the `pub` shared components;
  - loses `TestDaemon`'s four family impls.
- **`tddy-pr-stack`**: gains `rpc` (`PrStackHandler`, `PrStackServiceImpl`, `build_pr_stack_entry`,
  `PR_STACK_SERVICE`) and the `tddy-rpc` / `tddy-service` dependencies.
- **`tddy-workflow-recipes`**: `PR_STACK_SERVICE` becomes a re-export.
- **`tddy-daemon`**: `runtime.rs` builds `RpcHandlers` and installs the port. Gains the runtime-socket
  guard.

## Summary

`DaemonSessionHost` implements seven RPC-family traits over 31 fields, and every handler body lives
in `tddy-session-lifecycle`. The developer's target is about 10k production lines for this crate
(22,067 today) and for `tddy-core` (23,418).

This node creates **`tddy-daemon-rpc`**, a crate **above** the lifecycle crate, and moves the
Project, Catalog, ExecTool and PrStack families' bodies into it. The drafted "move each handler to
its domain crate" plan closes three dependency cycles, so it was rejected; the evidence is in
discovery pass 1. Successor nodes move three more groups into the same crate (about 4.9k lines), and
`tddy-core` gets its own node.

## Responsibility

- Create `tddy-daemon-rpc` with `ProjectRpcHandler`, `CatalogRpcHandler`, `ExecToolRpcHandler` and
  `PrStackRpcHandler`. Each is built by `from_host(&DaemonSessionHost)`, holds only its own fields,
  and never holds the host.
- Move the families' implementation into it: `svc_project_ports.rs`,
  `project_coordinate_handlers.rs`, `svc_catalog_ports.rs`, `svc_exec_tool_ports.rs`,
  `svc_pr_stack_ports.rs`, `svc_family_entries.rs`, and the PR-stack-only half of
  `svc_pr_status_for_caller.rs`.
- Move `PrStackHandler`, `PrStackServiceImpl`, `build_pr_stack_entry` and `PR_STACK_SERVICE` to
  `tddy_pr_stack::rpc`, leaving facades behind.
- Define the `DaemonRpcFamilies` port in `tddy-session-lifecycle` and implement it in
  `tddy-daemon-rpc`. It serves the two reverse edges: session start's peer-owned stack-base and
  named-node paths, and a session room's roster.
- Expose, as small `pub` components, the behaviour the handlers share with session code. Move every
  `pub(crate)` item only a handler uses into the new crate, rather than widening it.
- Rewire `runtime.rs`, and move the families' suites and `TestDaemon` impls.
- Add the runtime-socket wiring guard.

## Boundaries

- Does **not** change any `.proto`, wire signature or client. `tddy-web`, `tddy-coder` and
  `tddy-sandbox-app` are untouched.
- Does **not** let a family leave the local socket. `local_socket_reachability_acceptance.rs` stays
  green and unmodified, and the new runtime-socket guard stays green throughout.
- Does **not** move `SessionHandler` / `SessionService`, the session or demo-VM coordinate handlers,
  `daemon_rpc_handler`, `family_proto_bridge` itself, or the three non-RPC traits
  (`StackParentHost`, `SessionTerminalBridge`, `RemoteSnapshotSource`).
- Does **not** move the other three groups the developer ruled out of the crate (session agents,
  rosters and clones; activity, files, terminal and demo-VM ports; task and action services and
  admission). Those are successor nodes.
- Does **not** touch `tddy-core`.
- Does **not** give `tddy-session-lifecycle` any edge to `tddy-daemon-rpc`, normal or dev.
- Does **not** fix the complexity of what moves.
- Does **not** use the `OnceLock` + `set_self_handle` shape for the port.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `10/11` pr-stack-crate | the `tddy-pr-stack` crate, holding the PR-stack data model (`a2bceddb`) | `PrStackHandler`, `PrStackServiceImpl`, `build_pr_stack_entry` and `PR_STACK_SERVICE` move into it as a new `rpc` module | re-create or move the data model, or change `stack_ops` |
| `8/11` telegram | replaces `DaemonSessionHost`'s `telegram` field with a kernel port | nothing directly: `telegram` is not in any of the four handlers' field sets | re-do the telegram extraction or touch its port |
| `4/11` test-homes (merged) | `move_test_binary_to_crate` | each family's suites travel with it | re-implement the operation |

## Draft PR contract

Published first, in the planning commit's successor:

- the `tddy-daemon-rpc` crate with its manifest;
- the four handler structs with their real fields, and `from_host` constructors whose bodies are
  `todo!()`;
- their trait impls, all `todo!()`;
- the failing tests below.

The port **signature** is in the draft too: `DaemonRpcFamilies`, `with_rpc_families` and
`rpc_families`, in `tddy-session-lifecycle/src/rpc_families.rs`, with `todo!()` bodies. Its test lives
in the lifecycle crate, and naming a missing method there would break the compile of that crate's
entire test run. The shared components and the `tddy-pr-stack` move are **not** in the draft; the
shape tests pin them. **This PR must not merge in that state.**

## Green wave

**Wave:** after `10/11`
**Greenable independently:** **yes, now.** `tddy-pr-stack` exists on the base (`a2bceddb`), and every
test here drives this node's own handlers, fixtures or the assembled runtime.
**Concurrent with:** nothing below it is blocked by it.
**Blocks:** the successor nodes that move the remaining three groups into `tddy-daemon-rpc`.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-exec-tool-ports-list-exec-tools.md` | ⚠ **MOVES** | The file moves to `tddy-daemon-rpc`. The record moves to `packages/tddy-daemon-rpc/docs/code-issues/` with its **Location** updated and a `**Moved:**` line. It is not closed |
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-exec-tool-ports-list-session-tool-calls.md` | ⚠ **MOVES** | as above |
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-exec-tool-ports-stream-execute-tool.md` | ⚠ **MOVES** | as above |
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate.md` | ⚠ **MOVES** | `project_coordinate_handlers.rs` now moves with the Project family, so this record moves as above |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` | ⚠ **DURING** | Free functions leave `connection_service.rs` with the handlers (`merge_listed_projects_with_peers`, `require_pr_stack_orchestrator`, `owner_repo_from_repo_root`, `base_sync_*`, the `agent_models_cache` group). **Narrow** the record at wrap with the new measurement |
| `packages/tddy-session-lifecycle/docs/code-issues/cycle-connection-service-telegram.md`, `oversized-file-telegram-session-control.md` | 🔒 **claimed by #494** (ancestor 8/11) | Nothing here |
| `packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md`, `oversized-file-runtime.md` | ⚠ **DURING** | `runtime.rs` changes in types and one install call. `build()` must not grow: the handler assembly lives in `RpcHandlers`, not inline |
| [2026-09-09-daemon-sandbox-suites-never-call-set-self-handle.md](../todo/2026-09-09-daemon-sandbox-suites-never-call-set-self-handle.md) | ⚠ **DURING** | The reason the port refuses loudly (`FAILED_PRECONDITION`) instead of using a late-bound `OnceLock`. Do not add a second handle with that failure mode |
| [2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md](../todo/2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md) | ℹ **ANSWERED, in part** | Its second paragraph asks for `require_pr_stack_orchestrator` in a PR-stack module; it moves to `tddy-daemon-rpc`'s `pr_stack` module here. The trim-to-`Option` part is untouched, so the entry stays, annotated |
| [2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md](../todo/2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md) | ⚠ **DURING** | `stack_child_spawn_tests.rs` stays in the lifecycle crate. A moved PR-stack suite that starts flaking beside it is this flake, not a regression; compare against the entry before diagnosing |
| [2026-09-09-tddy-daemon-untested-complexity-hotspots.md](../todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md) | ⚠ **DURING** | `relaunch_sandboxed_runner` (CRAP 650) and `split_context_from_codebase_host` (CRAP 506) sit next door, untested. Nothing here edits them |
| [2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md) | ℹ **ANSWERED, in part** | AC11's measurement cuts at the first `#[cfg(test)]` **followed by `mod`**, which is the fix the entry asks for. The gate itself is untouched |

`tddy-daemon-rpc` is new and `tddy-pr-stack` has no `docs/code-issues/`, so neither has been
analysed. Run `/analyze-code-issues` against `tddy-daemon-rpc` at wrap, so the moved complexity is
measured in its new home.

## Technical changes

### State A

- `DaemonSessionHost` (`connection_service.rs:130`) has 31 fields and 41 impl blocks. It implements
  `ProjectHandler` and `ProjectService` (`svc_project_ports.rs`), `CatalogHandler`
  (`svc_catalog_ports.rs`), `ExecToolHandler` (`svc_exec_tool_ports.rs`) and `PrStackHandler`
  (`svc_pr_stack_ports.rs`). The Project bodies are in `project_coordinate_handlers.rs`.
- `PrStackHandler` and `PrStackServiceImpl` are defined in `tddy-session-lifecycle/src/pr_stack_rpc.rs`,
  and `build_pr_stack_entry` names `tddy_workflow_recipes::PR_STACK_SERVICE`.
- `runtime.rs` builds one host. `BinaryLocalSocketServices` (`:277`) names `<DaemonSessionHost>` for
  all five families. The helper methods live on the host.
- Two reverse uses: `svc_pr_status_for_caller.rs:222` and `:327` call `PrStackHandler` on `self`, and
  `session_room_roster()` bundles the four families' entries.
- `TestDaemon` implements the four family services by forwarding to the host (`test_util.rs:244-373`).
- Nothing dials the socket `runtime::build` assembles.

### State B

```
tddy-daemon ──► tddy-daemon-rpc ──► tddy-session-lifecycle ──► tddy-pr-stack (rpc: trait, ServiceImpl, entry)
                  │                    │   ▲
                  │                    │   └── DaemonRpcFamilies (port, defined here)
                  └── impl DaemonRpcFamilies for RpcHandlers
                  └── ProjectRpcHandler · CatalogRpcHandler · ExecToolRpcHandler · PrStackRpcHandler
```

- `tddy_daemon_rpc::{ProjectRpcHandler, CatalogRpcHandler, ExecToolRpcHandler, PrStackRpcHandler}`
  each implement their family trait. `from_host(&DaemonSessionHost)` clones the host's `Arc`s and
  components.
- `tddy_daemon_rpc::RpcHandlers::from_host(&host)` builds all four and exposes:
  - `project_service()`, `catalog_service()`, `exec_tool_service()`, `pr_stack_service()` — each
    `*ServiceImpl<XRpcHandler>`;
  - `entries() -> Vec<ServiceEntry>`;
  - `impl DaemonRpcFamilies`.
- `tddy_session_lifecycle::DaemonRpcFamilies` is a trait with
  `fn pr_stack_handler(&self) -> Arc<dyn PrStackHandler>` and
  `fn service_entries(&self) -> Vec<ServiceEntry>`.
  - `DaemonSessionHost::with_rpc_families(Arc<dyn DaemonRpcFamilies>) -> Self` installs it.
  - `rpc_families() -> Result<&Arc<dyn DaemonRpcFamilies>, Status>` returns `FAILED_PRECONDITION`
    when it is unwired.
- The shared components are `pub` in `tddy-session-lifecycle` and held by both sides: caller identity,
  RPC activity, peer routing, local exec-tool execution and agent definitions. Names are green's to
  choose; the field budgets constrain them.
- `runtime.rs`: host with every `with_*` → `RpcHandlers::from_host(&host)` →
  `host.with_rpc_families(Arc::new(handlers.clone()))` → `Arc`. The local-socket services and
  transport entries come from `handlers`. `BinaryLocalSocketServices` names the four handler types.

### Delta

| Package | Change |
|---|---|
| `tddy-daemon-rpc` | new: `src/{lib,project,catalog,exec_tool,pr_stack,families,test_util}.rs`; the moved suites under `tests/` |
| `tddy-session-lifecycle` | − the five implementation files and `svc_family_entries.rs`; − the PR-stack half of `svc_pr_status_for_caller.rs`; − `TestDaemon`'s four family impls; + `rpc_families.rs` (port); + the `pub` components; `pr_stack_rpc.rs` → facade; `session_room_roster()` reads its four family entries from the port |
| `tddy-pr-stack` | + `src/rpc.rs`; + `tddy-rpc`, `tddy-service`, `async-trait` dependencies |
| `tddy-workflow-recipes` | `PR_STACK_SERVICE` → `pub use tddy_pr_stack::PR_STACK_SERVICE` |
| `tddy-daemon` | `runtime.rs` rewiring; + `tddy-daemon-rpc` dependency; + `tests/local_socket_family_wiring_acceptance.rs`; `local_token_uds.rs` and `staging_forwarding_acceptance.rs` build their family services from `RpcHandlers` |

## Implementation milestones

- [x] Draft surface: `tddy-daemon-rpc` crate, four structs, `from_host` and trait impls as `todo!()`
- [x] Runtime-socket guard green on the unchanged wiring
- [x] `tddy_pr_stack::rpc` holds the trait, `PrStackServiceImpl`, `build_pr_stack_entry` and
  `PR_STACK_SERVICE`; facades in session-lifecycle and recipes
- [x] Shared components extracted in `tddy-session-lifecycle`, and the host delegating to them
- [x] `ProjectRpcHandler` real, its suites moved and green — landed after the port, not before: the room roster bundles all four families, so the port had to come first
- [x] `CatalogRpcHandler` real, its suites moved and green
- [x] `ExecToolRpcHandler` real, its suites moved and green
- [x] `DaemonRpcFamilies` port with `FAILED_PRECONDITION` when unwired; `session_room_roster()` and
  the two stack paths read it
- [x] `PrStackRpcHandler` real, its suites moved and green
- [x] `runtime.rs` rewired; the guard still green; `local_socket_reachability_acceptance.rs` unmodified
  and green
- [x] Four code-issue records moved to `packages/tddy-daemon-rpc/docs/code-issues/`
- [x] AC11: production lines re-measured and recorded — 22,067 → 20,058 (−2,009); criterion re-baselined to ≥ 2,000

## Testing plan

**Test level.** This is a refactor, so the load-bearing checks are the existing family suites,
moved and unedited except for `use` paths. The new tests cover what the move itself introduces:

1. the handler types exist and answer through their own `*ServiceImpl`;
2. they share the host's state;
3. the crate shape (what left, where the trait lives, the one-way edge, the field budgets);
4. the port refuses loudly when it is unwired;
5. the assembled runtime still serves every family on its socket.

**Options considered.**

| Option | Trade-off | Verdict |
|---|---|---|
| Rely only on the moved suites | They prove behaviour but not the shape. They would pass with the bodies still in the lifecycle crate behind re-exports | insufficient on its own |
| Text shape tests, as `#carve` 7–10 did | cheap, and pin exactly what a reviewer checks; brittle to formatting | **used, for AC3–AC8 and AC11** |
| Compile-level tests against the new types | pin the public surface and state sharing; fail by `todo!()` panic until implemented | **used, for AC1 and AC2** |
| A wire test through the socket `runtime::build` assembles | the only test that sees `runtime.rs`'s real wiring | **used, for AC9** |

### Acceptance tests

`packages/tddy-daemon-rpc/tests/rpc_handlers_acceptance.rs` covers AC1 and AC2:

- `a_project_handler_lists_the_projects_registered_for_the_caller`
- `a_project_handler_refuses_a_caller_whose_session_token_is_unknown`
- `a_catalog_handler_lists_the_tools_the_daemon_allows`
- `a_catalog_handler_records_rpc_activity_on_the_hosts_own_idle_tracker`
- `an_exec_tool_handler_refuses_a_caller_whose_session_token_is_unknown`
- `a_pr_stack_handler_refuses_a_caller_whose_session_token_is_unknown`

`packages/tddy-daemon-rpc/tests/rpc_handlers_shape.rs` covers AC3–AC8 and AC11:

- `the_session_host_no_longer_implements_any_of_the_four_families`
- `the_family_implementation_files_have_left_the_lifecycle_crate`
- `no_handler_holds_the_session_host`
- `each_handler_holds_no_more_fields_than_its_budget`
- `the_pr_stack_handler_trait_is_defined_by_the_pr_stack_crate`
- `the_pr_stack_crate_serves_its_own_rpc_family`
- `the_pr_stack_crate_depends_on_neither_the_lifecycle_crate_nor_the_recipes`
- `the_lifecycle_crate_has_no_edge_to_the_crate_above_it`
- `the_binary_runtime_serves_the_four_families_through_their_own_handlers`
- `the_lifecycle_crate_sheds_at_least_2000_production_lines`
- `every_crate_receiving_code_stays_within_10k_production_lines` (AC12). **Green by design** at
  red time, because the receivers are small today. It is a cap that must hold after green, not a
  specification of missing behaviour

`packages/tddy-session-lifecycle/tests/rpc_families_port_acceptance.rs` covers FR4:

- `a_host_built_without_rpc_families_reports_the_missing_wiring`
- `a_host_given_rpc_families_hands_back_the_ones_it_was_given`

`packages/tddy-daemon/tests/local_socket_family_wiring_acceptance.rs` covers AC9. It is **green
before and after, by design**:

- `the_assembled_daemon_answers_every_family_on_its_local_socket`

### Coverage

- Every family method's behaviour stays covered by its moved suite (discovery §7).
- Session start's peer-owned stack-base path is covered by `cross_host_stack_parent_acceptance.rs`,
  which moves.
- The session-room roster keeping all four families is covered by the guard and the moved suites.
- **Known gap:** no test opens a real session room and asks it for a moved family. See Technical debt.

## Technical debt & production readiness

- **Session-room roster:** no test asserts that a room serves the four moved families. It becomes
  observable only through `MultiRpcService::service_names()` on `session_room_roster()`, which is
  `pub(crate)`. Green should decide whether widening it to test is worth it.

## Decisions & trade-offs

- **Crate above, not domain crates** — the developer's decision on 2026-09-22. The domain-crate plan
  closes three cycles.
- **Widened from an in-place split** — the developer's decision, "Widen #520". The in-place split
  would have left the crate's size unchanged.
- **One node, not split along the port** — the developer's decision. The trade is a larger PR.
- **No kernel ports.** A crate above needs none. The one port needed points **down** into the
  lifecycle crate, `DaemonRpcFamilies`.
- **The port refuses when unwired, and does not use a `OnceLock`.** See Prerequisites: the
  `set_self_handle` entry.
- **The runtime-socket guard is green by design.** It is a regression guard for a refactor, not a
  specification of new behaviour.
- **AC11 re-baselined from 2,500 to 2,000 lines shed** — the developer's decision (2026-09-23),
  after all four families had moved. They measured **2,009** (22,067 → 20,058): the 2,500 estimate
  counted helpers the families share with session code (peer routing, caller identity, local exec
  tools, agent definitions), which became `pub` components and stay in the crate. The stack-level
  target of about 10k is unchanged; the successor nodes carry the rest.
- **No unit tests beyond the acceptance set.** The node's new code is either moved code, which the
  moved family suites (16 suites, ~5k lines) already pin, or shared components whose names and shape
  this plan deliberately leaves to `/green`. Pinning those now would specify a design nobody has
  chosen. The port has its own two tests.
- **Move rather than widen.** A `pub(crate)` item only a handler uses moves to `tddy-daemon-rpc`.
  Widening it would keep the lines in the crate this node exists to shrink.

## Successor PRs (planned, not yet opened)

To be added with `/add-to-pr-stack` on top of this node.

**The 10k cap** (developer, 2026-09-22):
- Every node holds each crate it moves code **into** to ≤10k production lines, and shrinks the crate
  it moves code **out of**.
- `tddy-session-lifecycle ≤ 10k` is the acceptance criterion of the **last** lifecycle node.
- Measurement: lines before the first `#[cfg(test)]` followed by `mod`; `*_tests.rs` and
  `test_util.rs` excluded.

| # | Node | Moves (production) | Into | Receiver after | Lifecycle after |
|---|---|---:|---|---:|---:|
| — | **this node** | ~2.5k | `tddy-daemon-rpc`, `tddy-pr-stack` | ~2.5k / ~3.1k | ~19.5k |
| 1 | Session agents, rosters and clones | ~2.4k | `tddy-daemon-rpc` | ~4.9k | ~17.1k |
| 2 | Activity, files, terminal and demo-VM ports | ~1.6k | `tddy-daemon-rpc` | ~6.5k | ~15.5k |
| 3 | Task and action services, session admission | ~0.9k | `tddy-daemon-rpc` | ~7.4k | ~14.6k |
| 4 | **Split-session and sandboxed-start cluster**: `svc_spawn_split_agent` 520, `svc_split_context_from_codebase_host` 457, `split_session` 651, `svc_relaunch_sandboxed_runner` 303, `svc_start_sandboxed_{claude_cli,cursor_cli,codebase}_session` 1,433 | ~3.4k, plus the free functions that follow it | **a second crate above** (working name `tddy-session-split`), because it does not fit in `tddy-daemon-rpc`'s budget | ~3.5k | **~11k** |
| 5 | Last lifecycle node: whatever the re-measure after node 4 says is left, found with `/analyze-code-issues` | ~1k+ | its natural home, checked against the cap | ≤10k | **≤10k** (the stack AC) |
| 6 | `tddy-core`, first cut: session actions and catalog, session metadata, `log_backend`, utilities going down | ~4.8k | new crates, each ≤10k | — | core ~18.6k |

Constraints on those nodes, known now:

- **Node 4 needs a port.** `svc_start_session_core` chooses the start flavour by placement, so the
  lifecycle crate reaches a flavour in a crate above only through a start-flavour port. It is the same
  shape as `DaemonRpcFamilies`, and like it, it refuses when unwired.
- **`relaunch_sandboxed_runner` (CRAP 650) and `split_context_from_codebase_host` (CRAP 506) move in
  node 4 and are entirely untested.** Node 4 needs characterization tests before it moves them. That
  is its prerequisite, not this node's.
- **`tddy-daemon-rpc`'s headroom after node 3 is about 2.6k.** Anything else proposed for it must be
  weighed against that.
- **`tddy-core`'s later cuts** (presenter after #495 with `toolcall → presenter` cut first;
  `backend` + `stream` after the `backend ↔ workflow` cut) are planned when node 6 is. Its end state
  (about 10k, only a wiring point, or vanished) is the developer's to choose then.

## TODO

- [x] Record initial discovery (`2026-09-19-carve-rpc-handlers-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (`tddy-daemon-rpc` surface + failing tests)
- [x] Run acceptance tests (verify they fail for the right reason; the guard passes)
- [x] USER REVIEW — acceptance tests (approved 2026-09-22)
- [x] TDD Red — unit tests: none added beyond the acceptance set; see Decisions
- [x] TDD Green (`/green`)
- [x] Move the four code-issue records
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Wrap documentation (`/wrap-context-docs`)

## Verification

Scoped, per CLAUDE.md:

```bash
./test -p tddy-daemon-rpc -p tddy-pr-stack
./test -p tddy-session-lifecycle
./test -p tddy-daemon -- local_socket
cargo clippy -p tddy-daemon-rpc -p tddy-session-lifecycle -p tddy-pr-stack -p tddy-daemon -- -D warnings
```

**The load-bearing checks** are the moved suites, unedited, and
`local_socket_family_wiring_acceptance.rs`, green on both sides of the change.
