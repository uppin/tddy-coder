# Changeset: TerminateSession stops a session as the UI's Terminate does

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 2 — one tool over an existing RPC; the cross-host path is inherited
**Stack**: `#start-parity` 7/9 — branch `feature/start-parity/terminate`, base `n6` `session-follow` (`feature/start-parity/session-follow`)
**PRD**: [PRD-2026-10-25-start-parity-terminate.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-terminate.md)

## Initial Discovery

See [./2026-10-25-start-parity-terminate-initial-discovery.md](./2026-10-25-start-parity-terminate-initial-discovery.md).

## Responsibility

The `TerminateSession` tool, its port method and its daemon implementation.

## Boundaries

Does not expose `DeleteSession` (which removes the worktree), SIGINT or SIGKILL. Recorded as a TODO if asked for.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` `remote-targets` (`feature/start-parity/remote-targets`) | the delegated home caller, the `daemon_instance_id` target argument, and the forward of a whole project-home tool call to the target's host | its tool takes `daemon_instance_id` and is forwarded by n1's route; one cross-host test drives it | add a forward, a delegated-caller check, or a per-tool peer call |

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

`TERMINATE_SESSION` def, `ProjectHomePort::terminate_session`, failing tests.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its tests exercise `n1` `remote-targets` (`feature/start-parity/remote-targets`)'s behaviour, which must be green first
**Concurrent with:** `n3` `worktree-list` (`feature/start-parity/worktree-list`), `n4` `async-start` (`feature/start-parity/async-start`), `n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`)
**Blocks:** nothing

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n8` `disk-refresh` (`feature/start-parity/disk-refresh`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-terminate.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-terminate.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

An orchestrator cannot stop a session it started. This node adds **`TerminateSession`**, which
does exactly what the UI's Terminate does.

## Scope

- [ ] An orchestrator can terminate a session in its project on any host, with the same effect as the UI's Terminate.
- [ ] The tool says the worktree and the branch are kept.

## Technical Changes

### State A

- The UI's Terminate calls `SessionService.SignalSession{session_id, signal: SIGTERM}` on the
  owning host (`useSessionsDrawerPaneActions.ts:114-129`).
- The handler (`tddy-agent-launch/src/svc_signal_delete_session.rs:26-114`) reads the PID, refuses
  with FAILED_PRECONDITION if the process is not alive, and sends SIGTERM.
- **It does not touch the session dir, the worktree or the branch.**

### State B

`TerminateSession {session_id, daemon_instance_id?}` calls `SignalSession` with SIGTERM on the
target's host. The description says the process gets SIGTERM, the session's worktree, branch and
record are kept, a stopped session can be resumed, and terminating an already-stopped session is an
error. It is limited to the caller's project.

## Testing Plan

### Acceptance Tests

- [ ] **Integration** `packages/tddy-daemon-rpc/tests/terminate_session_acceptance.rs` — `terminate_sends_sigterm_and_keeps_worktree_and_branch`: A live child process exits with SIGTERM. The worktree directory, the branch and the session dir all remain.
- [ ] **Integration** `same file` — `terminating_a_stopped_session_is_an_error_naming_it`: `is_error`, and the message says the process is not alive.
- [ ] **Integration** `same file` — `a_session_of_another_project_is_refused`: Error names "another project".
- [ ] **Unit** `packages/tddy-tool-engine/tests/terminate_session_schema.rs` — `the_description_says_what_happens_to_worktree_and_branch`: The description mentions SIGTERM, the worktree kept and the branch kept.
- [ ] **Integration** `packages/tddy-daemon/tests/project_home_remote_targets_acceptance.rs` — `terminate_stops_a_session_on_another_host`: The peer's process exits.

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Parity means `SignalSession` SIGTERM, not `DeleteSession`.** The UI's Terminate keeps everything; its Delete is a separate action.

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

## TODO

- [x] Record initial discovery (`2026-10-25-start-parity-terminate-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
