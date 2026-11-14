# Changeset: a stack node's session sees its stack read tools in a jail and on another host

**Date**: 2026-11-14
**Status**: 🚧 In Progress — planned (commit 1)
**Type**: Feature
**Complexity**: 2 — three crates, no proto or schema change. The work moves one existing classifier down a layer, appends its result in the jail bridge, and adds one branch in the forwarded-caller confirmation.
**Stack**: `#orchestrators` 13/13 · branch `feature/orchestrators/stack-node-reads` · base `feature/orchestrators/sweep` (#901)

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

The first push of this PR publishes the surface below as stubs marked `TODO(stack-node-reads)`,
plus the failing tests that specify it:

- `tddy_tool_engine::stack_tools::caller_records`: `StackCallerUnreadable`,
  `stack_caller_from_records`, `stack_bound_to_session_here`, `forwarded_stack_caller_role`,
  `stack_and_git_tools_offered_to`.
- `BoundJailSession::sessions_base()` (`pub(crate)`).

## Green wave

**Wave:** 1 of 1 (the only open node above #901)
**Greenable independently:** yes. Every test drives this node's own surface over merged parents.
**Concurrent with:** #901's CI fix (no shared files)
**Blocks:** none

## Scope

- [ ] Classification moved to `tddy-tool-engine`; `stack_caller_of` delegates
- [ ] Jail bridge lists a node's two reads
- [ ] Forwarded node caller served the two reads by the host that holds its stack
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
- **The other-host listing gap stays out of scope** (Boundaries). It is recorded as a future
  enhancement in this changeset only, not as a TODO file, because no planned scope is deferred.
  Orchestrator decision (developer: finish without stopping), 2026-11-14.

## Implementation Milestones

- [ ] `caller_records` implemented; `stack_caller_of` delegates
- [ ] Jail bridge appends the offer
- [ ] Forwarded stack-tool caller decided per family

## Testing Plan

**Level**: `tddy-tool-engine` unit tests (records over `stack_fixtures`, pure role decision, combined
offer); `tddy-session-lifecycle` unit tests (bridge, `sessions_base`); `tddy-daemon-rpc` acceptance in
`tests/stack_plan_tools_acceptance.rs`. That file drives the jail through the daemon's production
`sandbox_rpc_handler` (no jail spawned) and crosses hosts through `support`'s in-memory peer
forwarder, never LiveKit.

## Acceptance Tests

Planned, in `packages/tddy-daemon-rpc/tests/stack_plan_tools_acceptance.rs`:

- a stack node session in a jail is listed its two reads, as the daemon lists them
- an orchestrator's jail is listed every stack tool the daemon lists it (guard)
- a stack node session on another host reads its stack where the stack is held; its write is refused
- a session on another host bound to no node there is refused the stack reads (guard)

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] Green
- [ ] Wrap
