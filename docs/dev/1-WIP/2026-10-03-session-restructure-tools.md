# Changeset: Restructure tool calls in sessions

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-session-restructure-tools-initial-discovery.md).

## Stack

`#live-plan` 14/15 — branch `feature/live-plan/session-restructure-tools`, base `feature/live-plan/plan-dialog`.
PR: [#573](https://github.com/uppin/tddy-coder/pull/573)

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Tools `restructure_{load,check,apply,status,plans,anchors}`; host execution with worktree-bound plan paths; structured JSON results; in-jail relay entries; advertisement audit update.

## Boundaries

- Does **not** change the restructure CLI.
- Does **not** add LSP tools (`session-lsp-tools`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `session-lsp-tools` (12) | host-side index executor and worktree binding | restructure tools reuse both | add a second binding path |
| `plan-store` (#538) | `LoadPlans`/`PlanStatus` | load/status tools | change the RPCs |
| `live-plans` (#539) | stale ops | apply refuses stale ops by id | change stale detection |
| `transactional-groups` (8) | group outcomes | apply result reports a rolled-back group | change group events |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

**Sequencing facts (recorded at the red phase, 2026-10-03).**

- `session-lsp-tools` is in this branch's history as **stubs**: `IndexChannel`, `IndexLspExecutor`
  and `bind_to_session_worktree` exist, but `bind_to_session_worktree` refuses every path with
  `TODO(session-lsp-tools)`. This node's host executor calls it for every plan and file path, so
  `a_plan_path_outside_the_session_worktree_is_refused` can go green only once session-lsp-tools'
  binding is implemented (or rebased in). The other three acceptance tests do not depend on it
  beyond a path *inside* the worktree binding successfully.
- The daemon registers nothing index-backed yet (session-lsp-tools' own `TODO` in `runtime.rs`);
  this node's registration of `IndexRestructureExecutor` sits beside it in the same block.
- `transactional-groups` is consumed only as a refusal class: a rolled-back group reaches the tool as
  a `failed_precondition` refusal (`RestructureError::GroupDoesNotCompile` → `status_of`), carried
  in the run object's `refusal`. No group event is read.
- `live-plans` (#539): the apply refusal for a stale op is the index's
  (`RestructureError::StaleOperation` → `FailedPrecondition`); the tool names the op by asking
  `PlanStatus` for the plan's stale list after a `failed_precondition`. Stale detection is not
  changed here.

## Draft PR contract

Published by commit 2 (red phase):

- **Port** — `tddy_core::toolcall::restructure` (`packages/tddy-toolcall/src/toolcall/restructure.rs`):
  `RESTRUCTURE_TOOLS_ENV` (`TDDY_RESTRUCTURE_TOOLS`), `RESTRUCTURE_TOOL_NAMES`,
  `restructure_tools_enabled()`, `is_restructure_tool()`, async trait `RestructureExecutor`
  (`execute(worktree, tool_name, args) -> Result<Value, String>`), `register_restructure_executor`
  / `restructure_executor` — the `LspExecutor` registry shape.
- **Host executor** — `tddy_lsp_executor::restructure_via_index::IndexRestructureExecutor::new(Arc<dyn IndexChannel>)`
  (`packages/tddy-lsp-executor/src/restructure_via_index.rs`); module docs pin the tool → RPC table
  and the result JSON. Body `TODO(session-restructure-tools)`.
- **Dispatch** — `tddy_tool_engine::execute_tool_with_env` routes the six names to the registered
  executor (`src/restructure_tools.rs`; refusal `no warm index available` without one); the
  `RemoteShell` engine refuses them like the `Lsp*` tools. Implemented — it is wiring, not behaviour.
- **MCP surface** — `packages/tddy-tools/src/restructure_tools.rs`: `restructure_tool_defs()` (six
  defs with JSON schemas) and `restructure_tool_router()`, merged in `PermissionServer::new` only
  when `TDDY_RESTRUCTURE_TOOLS` is set (three lines in `server.rs`). Implemented.
- **Host gate / registration** — `TODO(session-restructure-tools)` markers in
  `tddy-daemon/src/runtime.rs` (register the executor when `index_daemon:` is configured) and
  `tddy-session-lifecycle/.../jail_env_builders.rs` (export `TDDY_RESTRUCTURE_TOOLS`). Nothing sets
  the gate yet, so today's advertised set is unchanged.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — needs session-lsp-tools' host executor, #539 and transactional-groups.
**Concurrent with:** plan-dialog, signature-rewrites
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/signature-rewrites` — next in the line.

## Prerequisites

### ℹ REFERENCE — a jail can name another session's worktree over the host bridge — [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)

Plan paths are resolved inside the host-bound worktree; nothing from the jail names a root.

## Affected Packages

- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — new `restructure_tools` module, gated by `TDDY_RESTRUCTURE_TOOLS`
- **tddy-toolcall** (re-exported as `tddy_core::toolcall`) — `restructure` port: gate, names, `RestructureExecutor` registry
- **tddy-lsp-executor**: `restructure_via_index::IndexRestructureExecutor` — host execution against the index
- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — dispatches the six names to the registered executor
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — sets the gate; binds the worktree
- **tddy-session-lifecycle** — sets `TDDY_RESTRUCTURE_TOOLS` in the jail env (`jail_env_builders.rs`)
- ~~tddy-sandbox-runner `IN_JAIL_RELAYABLE_EXEC_TOOLS` entries~~ — not needed; see Technical Debt

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-session-restructure-tools.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

An agent restructures only via `tddy-tools restructure` over `TDDY_INDEX_SOCKET` (unset in every session, unreachable from a jail); `index_client` is private to the binary and renders to the console; new MCP tools belong in their own module (`server.rs` over budget); the advertised set is pinned by `mcp_tool_advertisement_audit.rs`; in-jail exec tools are allowlisted by `IN_JAIL_RELAYABLE_EXEC_TOOLS`.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-lsp-executor — `tests/restructure_tools_via_index.rs`

The host executor lives in `tddy-lsp-executor` (beside `IndexChannel` and the worktree binding), so
its acceptance tests do too. A fake `code_index` server on an AF_UNIX socket records every request,
dialled through an `IndexChannel` the way the daemon's registry dials the real one
(`lsp_tools_via_index.rs` pattern).

| Test | Pins | Status |
|---|---|---|
| `restructure_check_returns_findings_as_json` | `Check` rooted at the worktree with the relative plan; findings + outcome as the run object | ✅ passes |
| `restructure_apply_returns_per_operation_outcomes` | `Apply` request; one outcome per `OperationApplied` with `op_id` | ✅ passes |
| `restructure_apply_refuses_a_stale_operation_by_id` | applied ops kept, `outcome: null`, `refusal {class: failed_precondition, message, stale: [{op, reason}]}` via a follow-up `PlanStatus` | ✅ passes |
| `a_plan_path_outside_the_session_worktree_is_refused` | `<path> is outside the session's worktree`, index never asked | ✅ passes — `bind_to_session_worktree` (session-lsp-tools) is real on the base |

### tddy-tools — `tests/mcp_tool_advertisement_audit.rs`

| Test | Status |
|---|---|
| `restructure_tools_are_advertised_only_when_the_host_sets_the_gate` | ✅ passes by design — the gate and the six defs are this commit's surface. `TDDY_RESTRUCTURE_TOOLS` joins `ADVERTISEMENT_ENV_KEYS`; the existing 47/44 audits still pass with the gate unset. |

## Technical Debt & Production Readiness

**Stubs:** none left. The executor body, the daemon registration (`runtime.rs`, beside the `Lsp*`
registration, from the same `IndexChannel`) and the `TDDY_RESTRUCTURE_TOOLS` jail export
(`restructure_tools_env`, set only when an executor is registered) are implemented.

**Not written, and why:**

- **No `IN_JAIL_RELAYABLE_EXEC_TOOLS` entries.** The premise was wrong: that list holds
  `(service, method)` RPC coordinates relayed *beside* `ExecuteTool` (today only
  `ConversationWorktree`), and `tddy-sandbox-runner`'s `bind_to_this_session` decodes every entry as
  a `ConversationWorktreeRequest`. The restructure tools are tool *names* carried inside
  `ExecuteTool`, which already crosses the jail on the runner's tool slot — exactly as the `Lsp*`
  names do, none of which is listed. An entry there would be wrong, not missing.
- **Handlers are not in `tddy-tool-engine`.** The planned `tddy_tool_engine::restructure_via_index`
  would make `tddy-tool-engine` depend on `tddy-lsp-executor` (for `IndexChannel`), dragging
  `tddy-index-daemon` and `tddy-code-restructuring` into `tddy-sandbox-runner` and `tddy-coder`,
  which pull neither today. The port went to `tddy-toolcall` (the `LspExecutor` precedent) and the
  executor to `tddy-lsp-executor`; no new crate edge was added.
- **No dispatch test for the tool-engine arm** — it is wiring over a first-wins global; the
  executor is pinned directly instead, like the `Lsp*` index executor.
- **No test for the daemon registration or the jail env export** — both are wiring over the first-wins global, verified by compile and clippy only; no in-jail end-to-end test.
- `restructure_load` / `status` / `plans` / `anchors` are implemented to their documented shapes but have no acceptance test
  (the PRD's criteria name check, apply, stale and binding only).

## Decisions & Trade-offs

_(populated during development)_

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### @validate-changes (2026-10-04, rebased onto `plan-dialog` b7fcc57b)

- **Stack gate:** current on base; `origin/<base>..HEAD` is this PR's 4 commits only; diff is 21 files, all owned here.
- **Responsibility delivered:** six tools, host execution with worktree-bound paths, structured JSON, daemon registration, jail gate. No `TODO(session-restructure-tools)` left.
- **Boundaries / Dependencies:** no parent symbol re-implemented (binding, `IndexChannel`, RPCs, group/stale refusals consumed as-is); CLI, LSP tools, proto untouched.
- **Scoped build/tests:** `cargo check --all-targets` clean on tddy-tools, tddy-tool-engine, tddy-lsp-executor, tddy-daemon, tddy-session-lifecycle; `tddy-lsp-executor` and `tddy-tool-engine` suites and `mcp_tool_advertisement_audit` pass.
- **Warning:** `run_json` returns `Err` when the follow-up `PlanStatus` call fails, discarding the operations the run already applied. A refusal should keep them.
- **Info:** the load/status/plans/anchors tools and the registration/env export have no test.
- **Fixed in `/pr-wrap`:** the `PlanStatus` failure is now reported as `refusal.stale_error` beside the refusal (key present only on failure; documented in the module docs), keeping the applied operations. `execute` (115 lines) split into one method per tool. Eight tests added (load, plans, status, anchors, a failed stale lookup, a non-`failed_precondition` refusal, two outside-worktree refusals); the executor suite is 12 tests.
- **Still untested:** the daemon registration and the jail env export (compile and clippy only); no in-jail end-to-end test, so the PRD's jail criterion is **deferred** with the developer's agreement (2026-10-04) — `docs/dev/todo/2026-10-04-session-restructure-tools-no-jail-end-to-end-test.md`.

### File length gate (`/pr-wrap` 3.5)

| File | Production lines | Outcome |
|---|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1,668 → 1,677 | 🔴 grew; split deferred under the stack rule (parents touch it); recorded in its code-issue record |
| `packages/tddy-tool-engine/src/lib.rs` | 869 → 873 | 🔴 grew; split deferred with the developer's consent (2026-10-04); recorded in its code-issue record |

Both are tracked by `docs/dev/todo/2026-10-04-session-restructure-tools-grew-two-oversized-files.md`.
`restructure_via_index.rs` is 0 → ~450 production lines, under budget.

## TODO

- [x] Record initial discovery (`2026-10-03-session-restructure-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [x] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-session-restructure-tools-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
