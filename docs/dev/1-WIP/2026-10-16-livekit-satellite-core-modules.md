# Changeset: LiveKit-free modules leave the LiveKit plugin

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #627
**Type**: Refactor (mechanical move)
**Stack**: #satellite 5/13. Base: `feature/satellite/room-tokens` (#626).

## Initial Discovery

[2026-10-16-livekit-satellite-core-modules-initial-discovery.md](./2026-10-16-livekit-satellite-core-modules-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ DURING | `svc_start_session_core.rs` changes only its `placement` import path; the function's complexity is untouched. |
| `packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` | ⚠ DURING | `os_user_resolution` lands as its own kernel module, not in `config.rs`. |
| [2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs.md](../todo/2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs.md) | ⚠ DURING | A known engine gap on crate moves; check visibility after each move. |

No `**Claimed by:**` marker in any touched package's code issues.

## Responsibility

Moving `placement`, `session_git` and `os_user_resolution` out of the plugin, repointing the
instance-id imports at the kernel, and removing the two unused `livekit` edges.

## Boundaries

- Moves no module that uses LiveKit and introduces no port.
- Does not touch `PeerRoute`, `classify_peer_route`, `CommonRoom` or `daemon_rpc_identity` imports
  (node 2's re-export story) beyond the identity helpers named above.
- Strikes nothing from `KNOWN_OFFENDERS`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | The ratchet and `WorkspaceGraph` | Extends the ratchet suite with this node's edge assertion | Change the allowed sets |
| #624–#626 | Nothing consumed (line predecessors) | — | Touch their ports |

**As published (wave 2):** the two shape tests. The `session_git` suite moves to
`tddy-agent-launch/tests/` together with the module in green — relocating it first would leave
`tddy-agent-launch`'s test target unable to compile. The SDK-edge assertion waits on #623's
`WorkspaceGraph`.

## Draft PR contract

Moves cannot be published as signatures ahead of the move, so the first push of this PR publishes
**the failing tests that define the end state**:

- `the_plugin_declares_only_livekit_work`: the plugin's `lib.rs` declares no `placement`,
  `session_git` or `os_user_resolution` module.
- `the_daemon_rpc_and_session_lifecycle_crates_do_not_name_the_livekit_sdk`: `WorkspaceGraph`
  shows neither crate naming `livekit`.
- `session_git_identity` suite relocated under `tddy-agent-launch/tests/`, importing
  `tddy_agent_launch::session_git` (fails to compile until the move).

This is the narrow "purely mechanical move" exception to the boundary contract: no behaviour, no
new surface.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes; a pure move, and no test depends on another node's behaviour.
**Concurrent with:** nodes 2–4 and 6–9
**Blocks:** none. It shrinks the import surface nodes 10 and 11 would otherwise rewrite; no behaviour
edge.

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-livekit`, `tddy-agent-launch`, `tddy-daemon-kernel`, `tddy-session-lifecycle`,
`tddy-daemon-rpc`, `tddy-session-agents`, `tddy-session-files`, `tddy-host-service`, `tddy-daemon`,
`tddy-coder` (one identity import via the lifecycle re-export), `tddy-daemon-extensions` (ratchet
suite).

## Summary

The plugin crate holds only LiveKit work, and two dead SDK edges disappear.

## Scope

- [ ] Move `placement` → `tddy-agent-launch` (engine)
- [ ] Move `session_git` → `tddy-agent-launch` with its suite (engine)
- [ ] Move `os_user_resolution` → `tddy-daemon-kernel` (engine)
- [ ] Repoint re-exports in `tddy-session-lifecycle` (`connection_service.rs:512`, `svc_resolve_os_user.rs:128-129`)
- [ ] Repoint the 24 identity imports
- [ ] Remove `livekit = "0.7"` from `tddy-daemon-rpc` and `tddy-session-lifecycle`

## Technical Changes

### State A
- `tddy-daemon-livekit/src/placement.rs` (155 lines, no imports); `session_git.rs` (48 lines,
  `tddy_accounts`); `os_user_resolution.rs` (23 lines, `tddy_rpc`, `tddy_daemon_kernel`). Declared
  at `lib.rs:44-48`.
- Users: `tddy-agent-launch/src/svc_start_session_core.rs:12,18,19` (placement);
  `session_acting_identity.rs:22` (session_git); `session_coordinate_handlers.rs:12`
  (os_user_resolution); lifecycle re-exports `connection_service.rs:512`,
  `svc_resolve_os_user.rs:17,128,129`; plugin test `tests/session_git_identity_acceptance.rs:5`.
- Identity re-export users (`livekit_peer_discovery::local_instance_id_for_config`): 3 in
  agent-launch, 3 in daemon-rpc, 3 in daemon, 1 in host-service, 7 in session-agents, 1 in
  session-files, 5 in session-lifecycle, 1 in tddy-coder (`web_server.rs`). The kernel already
  exports them at `tddy_daemon_kernel::daemon_identity`.
- `grep -E '(^|[^_a-z])livekit::'` over `tddy-daemon-rpc/src` and `tddy-session-lifecycle/src`:
  no hits, so both `livekit = "0.7"` edges are dead.

### State B
Each module lives with its consumer, the identity helper is imported from where it is defined, and
the two manifests are clean of the SDK.

### Delta
Moves and import rewrites in the listed packages; two manifest lines removed.

## Implementation Milestones
- [ ] M1 Three engine moves, each building and testing green in its new crate
- [ ] M2 Imports repointed
- [ ] M3 Edges removed; shape tests green

## Testing Plan

The moved code keeps its tests (the relocated `session_git` suite and any inline tests). The end
state is asserted by two shape tests. A pure move is verified by the scoped test runs of every touched
package.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `the_plugin_declares_only_livekit_work` | `packages/tddy-daemon-livekit/tests/plugin_holds_only_livekit_work.rs` (new) | `lib.rs` declares no `placement`, `session_git`, `os_user_resolution` module |
| `the_daemon_rpc_and_session_lifecycle_crates_do_not_name_the_livekit_sdk` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | `names_directly` for both crates excludes `livekit` |
| `session_git` suite (existing tests, relocated) | `packages/tddy-agent-launch/tests/session_git_identity.rs` (moved from the plugin's `session_git_identity_acceptance.rs`) | The session git environment, now from `tddy_agent_launch::session_git` |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Destinations follow the consumers.** `placement` and `session_git` have one consumer each
  (agent-launch), and `os_user_resolution` sits beside the resolver type it wraps.
- **Engine moves only.** Per this repo's practice, module moves go through `tddy-tools restructure`.
  An engine refusal is reported and the developer decides before any hand move.
- **No ratchet strike.** Every touched crate still names the plugin for LiveKit work; the shrink is
  recorded by the two shape tests instead.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-16-livekit-satellite-core-modules-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: developer asked to finish the stack without stopping, 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
