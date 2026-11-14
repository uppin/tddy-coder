# Changeset: a stack node's session sees its stack read tools in a jail and on another host

**Date**: 2026-11-14
**Status**: 🚧 In Progress — draft-PR contract written (commit 2, 2026-11-14)
**Type**: Feature
**Complexity**: 2 — three crates, no proto or schema change. The work moves one existing classifier down a layer, appends its result in the jail bridge, and adds one branch in the forwarded-caller confirmation. Re-scored at 6g: unchanged
**Stack**: `#orchestrators` 13/13 · branch `feature/orchestrators/stack-node-reads` · base `feature/orchestrators/sweep` (#901) · PR [#902](https://github.com/uppin/tddy-coder/pull/902)

## Initial Discovery

[2026-11-14-stack-node-reads-initial-discovery.md](./2026-11-14-stack-node-reads-initial-discovery.md)

## Prerequisites

### ✅ RESOLVED HERE — The jail `ListExecTools` bridge does not offer a stack node's two read tools — [`2026-11-14-jail-list-exec-tools-misses-stack-node-reads.md`](../todo/2026-11-14-jail-list-exec-tools-misses-stack-node-reads.md)

Closed by moving the record-reading classification down into `tddy-tool-engine`. The TODO's other
route is the binding recorded on the session at spawn; it is not taken (see Decisions). Wrap deletes
the file.

### ✅ RESOLVED HERE (stack section only) — [`2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md`](../todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md)

Only the section **"Also: a stack node's read tools on another host (`#orchestrators` 7/11)"** is
closed here, by the owning host checking its own stack record. The file's main item, the
conversation spawner's start tools, stays open. **Wrap removes that one section and keeps the file.**

### ⚠ DURING — [`complexity-daemon-rpc-handler-handle-rpc.md`](../../../packages/tddy-session-lifecycle/docs/code-issues/complexity-daemon-rpc-handler-handle-rpc.md)

`handle_rpc`'s `ListExecTools` arm is not touched. The change stays inside
`list_exec_tools_from_jail`.

### ⚠ DURING — `tddy-daemon-rpc` `exec_tool/ports.rs` complexity records

`complexity-exec-tool-ports-execute-tool.md` and `complexity-exec-tool-ports-list-exec-tools.md`:
`ports.rs` is not edited. The forwarded path changes in `project_home_remote.rs`.

## Affected Packages

- [tddy-tool-engine](../../../packages/tddy-tool-engine/) — `stack_tools::caller_records`: record-reading classification, the forwarded caller's role, the combined offer
- [tddy-session-lifecycle](../../../packages/tddy-session-lifecycle/) — the jail bridge's `ListExecTools`; `BoundJailSession::sessions_base`
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — `stack_caller_of` delegates down; the forwarded caller is confirmed as a node from this host's own store

## Related Feature Documentation

- PRD: [PRD-2026-11-14-stack-node-reads.md](../../ft/coder/1-WIP/PRD-2026-11-14-stack-node-reads.md)
- [PR stacking § PR-management tools](../../ft/coder/pr-stacking.md)

## Summary

A session bound to a node of a PR stack is offered `pr_stack_status` and `pr_read` for that stack
(#884). With this change it gets them in two more places:

- **In a jail.** The jail's `ListExecTools` bridge lists exactly what the daemon lists.
- **On another host.** The host that holds the stack serves the two reads to a caller held elsewhere
  when a node of its stack is bound to that caller.

## Background

At the `#orchestrators` TODO gate (2026-11-14) the developer chose to continue with one node that
closes these two TODOs together. Both are the same missing fact: whether a node binds this session.
Each path lacks it for a different reason. The jail bridge sits in a crate below the classifier.
The forwarded path sits on a host that does not hold the session.

## Responsibility

- **Move the classification down a layer.** Add
  `tddy_tool_engine::stack_tools::caller_records::stack_caller_from_records(sessions_base,
  session_id)`. It holds the record reads that `stack_caller_of` does today in `tddy-daemon-rpc`
  (`changeset_of`, `node_binding`). Errors are `StackCallerUnreadable { session_id, what, why }`.
  `stack_caller_of` becomes a thin adapter that maps the error to `PermissionDenied` with today's
  message.
- **Combine the offer.** `stack_and_git_tools_offered_to(role)` returns `stack_tools_offered_to` then
  `stack_git_tools_offered_to`. The daemon's `offered_catalog` and the jail bridge both append it,
  for a session whose tool role is not `ProjectHome`. One function, so the two paths cannot drift.
- **Jail bridge.** `list_exec_tools_from_jail` classifies the session from
  `BoundJailSession::sessions_base()` (the parent of `session_dir`'s `sessions/` directory) and
  appends the combined offer. A session whose records cannot be read is refused
  (`PermissionDenied`), as on the daemon.
- **Forwarded caller.** `stack_bound_to_session_here(sessions_base, project_id, session_id)` finds the
  project's stack, in this host's store, that has a node bound to the session.
  `forwarded_stack_caller_role(listed_session_role, bound_stack_id)` returns an orchestrator role
  first (owning no stack here), else `StackNode { stack_id }`. In `project_home_remote.rs`, a stack
  tool for a caller held elsewhere is confirmed at its owner. The owner must still report the
  session itself; the role is then decided by these two functions. A node is served
  `is_stack_tool_offered_to`'s two reads, and every other stack tool is refused as permission
  denied.

## Boundaries

- No proto change. `ListSessions` is not widened.
- The stack git tools are still served only to an orchestrator-role caller, forwarded or not.
- The conversation spawner's forwarded start tools stay refused (the main item of the 2026-10-26
  TODO).
- A node session whose orchestrator and stack live on **another** host is still not listed the two
  reads by its own host's `ListExecTools`. That host has no record of the binding. Closing it means
  carrying the binding to the session's host, a future enhancement this node does not plan.
- `handle_rpc` and `exec_tool/ports.rs` are not edited.
- No change to `StackCallerRole`, `stack_caller_role`, `addressed_stack_id` or
  `is_stack_tool_offered_to`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs**; implementing one
here collides with the PR that owns it. All are merged except #901.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n7 `stack-plan-tools` (#884, merged) | `StackCallerRole`, `StackCaller`, `StackCallerFacts`, `stack_caller_role`, `stack_tools_offered_to`, `is_stack_tool_offered_to`, `addressed_stack_id` (`tddy-tool-engine::stack_tools`); `stack_caller_of`, `run_stack_tool`, `serve_stack_tool_for_caller_held_elsewhere` (`tddy-daemon-rpc`) | the records reader feeds `stack_caller_role`; the forwarded node is served through `serve_stack_tool_for_caller_held_elsewhere` | change the role model, the addressing rules or which tools a role is offered |
| n10 `stack-git-tools` (#887, merged) | `stack_git_tools_offered_to` (`tddy-tool-engine::stack_git_tools`) | part of the combined offer | serve a git tool to a node |
| `#project-home` 5/7 (#700, merged) | `list_exec_tools_from_jail`, `BoundJailSession` | the bridge appends the offer | change the bridge's refusal of a request naming another session |
| `#start-parity` 1/9 (merged) | `serve_project_home_for_caller_held_elsewhere`, `home_caller_confirmed_at_owner` | the stack-tool branch decides the role from the owner's row plus this host's store | change how project-home tools confirm their caller |
| n12 sweep (#901, open) | the tip this node is planned on (B9 `CreateProjectStack`, the B5 `tool-engine` split) | base branch only; no surface consumed | touch the sweep's changes |

## Draft PR contract

Written 2026-11-14. Each stub is marked `TODO(stack-node-reads)`.

- `packages/tddy-tool-engine/src/stack_tools/caller_records.rs` (new, `pub mod caller_records` in
  `stack_tools.rs`):
  - `StackCallerUnreadable { session_id, what: &'static str, why }` (Display, Error). Written in
    full.
  - `stack_caller_from_records(&Path, &str) -> Result<Option<StackCaller>, StackCallerUnreadable>`.
    Stub: `Ok(None)`.
  - `stack_bound_to_session_here(&Path, project_id, session_id) -> Result<Option<String>, StackCallerUnreadable>`.
    Stub: `Ok(None)`.
  - `forwarded_stack_caller_role(listed_session_role: &str, bound_stack_id: Option<&str>) -> Option<StackCallerRole>`.
    Stub: `None`.
  - `stack_and_git_tools_offered_to(Option<&StackCallerRole>) -> Vec<ToolDef>`. Stub: empty.
- `BoundJailSession::sessions_base(&self) -> Option<&Path>` (`pub(crate)`,
  `daemon_rpc_handler.rs`). Stub: `None`. It carries `#[cfg_attr(not(test), allow(dead_code))]`
  until green calls it.
- `list_exec_tools_from_jail`: the `TODO(stack-plan-tools)` marker is re-pointed to
  `TODO(stack-node-reads)`, naming the calls green adds. No behaviour change.
- `tddy-tool-engine` `[dev-dependencies]`: `tddy-changeset` with `test-support`, for
  `stack_fixtures`. It is internal and already in the tree through `tddy-core`; `Cargo.lock` gains
  the one edge.
- Nothing in `tddy-daemon-rpc/src` changes in the contract. Its acceptance tests reach the jail
  bridge through `DaemonSessionHost::sandbox_rpc_handler`, which needs no new dependency.

## Green wave

**Wave:** 1 of 1 (the only open node above #901)
**Greenable independently:** yes. Every test drives this node's own surface over merged parents.
**Concurrent with:** #901's CI fix (no shared files)
**Blocks:** none

## Scope

- [x] Classification moved to `tddy-tool-engine`; `stack_caller_of` delegates
- [x] Jail bridge lists a node's two reads
- [x] Forwarded node caller served the two reads by the host that holds its stack
- [ ] ✅ [jail TODO](../todo/2026-11-14-jail-list-exec-tools-misses-stack-node-reads.md) deleted at wrap
- [ ] ✅ [forwarded TODO](../todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md): stack section removed at wrap

## Technical Changes

### State A (Current)

Verified on `1ce80419a`.

- `stack_caller_of` + `changeset_of` + `node_binding`
  (`tddy-daemon-rpc/src/exec_tool/stack_tools.rs:78-155`) read every fact from `tddy_core`. Callers:
  `run_stack_tool` (:168), `offered_catalog` (`project_home.rs:248`), `run_stack_git_tool`
  (`stack_git_tools.rs:65`).
- `list_exec_tools_from_jail` (`svc_list_exec_tools_wiring.rs:80`) offers
  `session_offered_catalog_for` alone and carries `TODO(stack-plan-tools)`.
- `home_caller_confirmed_at_owner` (`project_home_remote.rs:154`) refuses every row whose role is
  not an orchestrator role, for every tool family, before the stack branch (:80) runs.

### State B (Target)

- The classifier lives in `tddy-tool-engine::stack_tools::caller_records`, and both `ListExecTools`
  paths append `stack_and_git_tools_offered_to` from it.
- The forwarded path confirms the owner's row first and decides the role per tool family. The
  project-home tools still require an orchestrator role. The stack tools accept
  `forwarded_stack_caller_role`. The stack git tools still require an orchestrator role.

### Delta

- `tddy-tool-engine`: new `stack_tools/caller_records.rs`; dev-dependency on `tddy-changeset`
  (`test-support`).
- `tddy-session-lifecycle`: `svc_list_exec_tools_wiring.rs`, `daemon_rpc_handler.rs`
  (`BoundJailSession::sessions_base`).
- `tddy-daemon-rpc`: `exec_tool/stack_tools.rs` (`stack_caller_of` delegates; its unit tests move
  with the reads or stay as adapter tests), `exec_tool/project_home.rs` (combined offer),
  `exec_tool/project_home_remote.rs` (owner row, then role per family).

## Decisions

- **Classification moves down a layer, into `tddy-tool-engine`.** The alternatives were to record
  the node binding on the session at spawn, or to inject a classifier port from `tddy-daemon-rpc`
  into the lifecycle host. A recorded binding is a second copy of the store's
  `StackNode.session_id` that can drift when a node is re-bound. A port adds plumbing through
  `DaemonSessionHost` for a pure read of `tddy-core` records. Orchestrator decision (developer:
  finish without stopping), 2026-11-14.
- **The forwarded node is confirmed from this host's own store, not by widening `ListSessions`.**
  This needs no proto change. The host that serves the read is the one that holds the binding, so
  it never trusts a peer's claim about a node. Orchestrator decision (developer: finish without
  stopping), 2026-11-14.
- **One combined offer for both `ListExecTools` paths**, git tools included, so the jail answer
  equals the daemon's by construction. For a node the git tools are empty, so this widens nothing.
  Orchestrator decision (developer: finish without stopping), 2026-11-14.
- **`BoundJailSession::sessions_base()` is derived from `session_dir`**, not added as a field. The
  field would change `sandbox_rpc_handler`'s signature, which the daemon's jail build calls. The
  layout is the documented `<base>/sessions/<id>`, and an off-layout directory answers `None`, which
  the bridge refuses. Orchestrator decision (developer: finish without stopping), 2026-11-14.
- **Two guard tests are kept, though they pass on the contract** (the orchestrator jail parity, and
  a stranger on another host is refused). Each pins a behaviour green could break: a duplicated
  offer, or a forwarded caller let through without a binding. Orchestrator decision (developer:
  finish without stopping), 2026-11-14.
- **The acceptance tests extend #884's `stack_plan_tools_acceptance.rs`** instead of starting a new
  file. They reuse its two-host `World` and its node and orchestrator fixtures; a new file would
  copy about 150 lines of them. Orchestrator decision (developer: finish without stopping),
  2026-11-14.
- **The other-host listing gap stays out of scope** (Boundaries). It is recorded as a future
  enhancement in this changeset only, not as a TODO file, because no planned scope is deferred.
  Orchestrator decision (developer: finish without stopping), 2026-11-14.

## Implementation Milestones

- [x] `caller_records` implemented; `stack_caller_of` delegates
- [x] Jail bridge appends the offer
- [x] Forwarded stack-tool caller decided per family

## Testing Plan

**Level**: `tddy-tool-engine` unit tests (records over `stack_fixtures`, pure role decision, combined
offer); `tddy-session-lifecycle` unit tests (bridge, `sessions_base`); `tddy-daemon-rpc` acceptance in
`tests/stack_plan_tools_acceptance.rs`. That file drives the jail through the daemon's production
`sandbox_rpc_handler` (no jail spawned) and crosses hosts through `support`'s in-memory peer
forwarder, never LiveKit.

## Acceptance Tests

Written and run 2026-11-14 (fluent-tests style). Local runs were scoped to single test targets with
a name filter, as the developer asked, to confirm each failure. CI verifies the rest.

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/stack_plan_tools_acceptance.rs`

These four tests are appended to #884's suite and reuse its two-host `World`. The jail is the
daemon's production `sandbox_rpc_handler`. Hosts talk over the in-memory peer forwarder, never
LiveKit.

| Test | Checks | On the contract |
|---|---|---|
| `a_stack_node_session_in_a_jail_is_listed_its_two_reads_as_the_daemon_lists_them` (:818) | the jail lists `[pr_stack_status, pr_read]`, the same as the daemon's `ListExecTools` | ❌ FAILS: the jail lists `[]` |
| `an_orchestrators_jail_is_listed_every_stack_tool_the_daemon_lists_it` (:836) | an orchestrator's jail lists all eleven, the same as the daemon | ✅ passes — **guard**: an `orchestrator` role has the `ProjectHome` tool role, whose catalogue already carries them. It pins that green's append is skipped for `ProjectHome` (no duplicates) |
| `a_stack_node_session_on_another_host_reads_its_stack_where_the_stack_is_held` (:851) | a node on host-b gets `pr_stack_status` answered by host-a with its stack's nodes; its `pr_add_planned` is refused as `PermissionDenied`; the stack is unchanged | ❌ FAILS: `PermissionDenied`, "tool pr_stack_status is served only to a project-home session, and daemon host-b reports session … is not one" |
| `a_session_on_another_host_bound_to_no_node_there_is_refused_the_stack_reads` (:893) | a host-b session that no node is bound to is refused `pr_stack_status` on host-a as `PermissionDenied` | ✅ passes — **guard**: green must keep refusing it |

### Unit tests

`packages/tddy-tool-engine/src/stack_tools/caller_records.rs` (`--lib caller_records`: 9 fail and
3 pass, the 3 being guards):

- ❌ `a_session_its_orchestrators_stack_binds_is_classified_a_node_of_that_stack` (:144)
- ❌ `an_orchestrator_role_session_is_classified_owning_the_stack_its_changeset_names` (:165)
- ✅ guard `a_child_its_orchestrators_stack_does_not_bind_is_classified_with_no_stack_role` (:183)
- ❌ `a_session_with_no_record_cannot_be_classified` (:197) — expects `Err { "ghost", "record" }`
- ❌ `the_stack_with_a_node_bound_to_the_session_is_found_on_the_host_that_holds_it` (:215)
- ✅ guard `a_session_no_stack_of_the_project_binds_is_bound_to_none` (:228)
- ❌ `a_forwarded_caller_its_owner_lists_as_an_orchestrator_owns_no_stack_here` (:243)
- ❌ `a_forwarded_caller_bound_to_a_node_here_is_a_node_of_that_stack` (:257)
- ❌ `an_orchestrator_role_wins_over_a_node_binding_for_a_forwarded_caller` (:273)
- ✅ guard `a_forwarded_caller_with_no_role_and_no_binding_has_no_stack_role` (:287)
- ❌ `a_node_is_offered_status_and_read_and_no_git_tool` (:300)
- ❌ `an_orchestrator_is_offered_every_stack_tool_then_every_stack_git_tool` (:314)

`packages/tddy-session-lifecycle/src/connection_service/svc_list_exec_tools_wiring.rs`
(`--lib svc_list_exec_tools_wiring`: 2 new tests fail and 1 new guard passes; the 3 existing tests pass):

- ❌ `a_stack_node_jail_is_answered_status_and_read_for_its_stack` (:198) — left `[]`
- ❌ `a_jails_sessions_base_is_the_directory_holding_its_sessions_directory` (:241) — left `None`
- ✅ guard `a_session_dir_outside_a_sessions_directory_has_no_sessions_base` (:257)

`cargo clippy -p tddy-tool-engine -p tddy-session-lifecycle --lib --tests -- -D warnings` is clean.
The workspace `--all-targets` run is CI's.

**Green moves `stack_caller_of`'s five unit tests** (`tddy-daemon-rpc/src/exec_tool/stack_tools.rs`)
or keeps them as adapter tests. The tests above already cover the records reader, so green may drop
the daemon-side copies that duplicate them.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests: waived. The developer said "finish the remaining PRs without
  stopping" (orchestrator decision, 2026-11-14)
- [x] TDD Red — write failing unit/integration tests
- [x] Green
- [ ] Wrap
