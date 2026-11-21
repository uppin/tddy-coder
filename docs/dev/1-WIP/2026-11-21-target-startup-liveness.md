# Changeset: The startup liveness probe of a target agent has a round trip to answer

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 3 — one crate (agent-launch), integration tests over a stand-in ssh target, a spawned-process seam
**Stack**: `#target-parity` 5/7 — branch `feature/target-parity/startup-liveness`, base `feature/target-parity/running-tool-calls` — PR [#973](https://github.com/uppin/tddy-coder/pull/973)

## Initial Discovery

[2026-11-21-target-startup-liveness-initial-discovery.md](./2026-11-21-target-startup-liveness-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: `remote_agent_launch/startup.rs` `exits_during_startup`

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-startup-liveness.md](../../ft/daemon/1-WIP/PRD-2026-11-21-target-startup-liveness.md)

## Summary

The `kill -0` probe is bounded by the start's transport allowance (`start_timeout`), and only a probe that answered can judge the agent alive at the end of the window.

## Responsibility

- The liveness judgement of `exits_during_startup` for a target start.

## Boundaries

- Does not change the pid wait (#963), the startup window's length or local starts.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1`–`n4` | nothing consumed — the line puts them below | — | — |

## Draft PR contract

No new public surface; the first push is the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes
**Concurrent with:** n1, n2, n3, n4
**Blocks:** none

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7):

    n1 permission-answer → n6 question-answer
    n2 target-worktree-actions → n7 target-worktree-stats

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7. n1 and n2 lead wave 1 (one dependent each; n1 first as
the more foundational); n3–n5 block nothing.

## Scope

- [ ] Probe bound and judgement
- [ ] Feature doc updated through the wrap

## Technical Changes

### State A (Current)

`startup.rs:86-97`: each probe `.within(what the window has left)`, `.unwrap_or(true)`; over Coder (6–9 s a call) no probe answers inside a 500 ms window.

### State B (Target)

A probe has the transport allowance to answer; the window closes only on an answered probe; a dead pid fails the start with its last output.

## Acceptance Tests

`packages/tddy-agent-launch/tests/remote_agent_launch_acceptance.rs`:
- [ ] `an_agent_that_dies_on_a_slow_target_fails_the_start_though_its_terminal_reports_late` (the reproduction; passes with a round trip shorter than the window — the control)
- [x] `an_agent_that_stays_up_on_a_slow_target_starts` — a **guard**: green today (an unanswered probe counts as alive), must stay green once the probe has a round trip to answer

## Decisions & Trade-offs

- **`start_timeout` as the probe's bound**: it already names "the transport's connect time and the remote command's start" (#963); no new setting.

## TODO

- [x] Record initial discovery (`2026-11-21-target-startup-liveness-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (delegated: the user said "finish the rest without approval")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
