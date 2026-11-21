# Changeset: Target tab errors say what went wrong

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — two crates (agent-launch, session-lifecycle) plus an I/O seam (a remote probe through RemoteShell), error-contract change across tab kinds
**Stack**: `#tab-hardening` 1/4 — branch `feature/tab-hardening/tab-errors`, base `master`
**PR**: #989 https://github.com/uppin/tddy-coder/pull/989

## Initial Discovery

[2026-11-21-tab-errors-initial-discovery.md](./2026-11-21-tab-errors-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: `probe_agent_on_target`, a `RemoteLaunchError` variant for a missing agent, one tab error mapper
- **tddy-session-lifecycle**: `target_tabs.rs` / `target_agent_tabs.rs` use the shared mapper

## Related Feature Documentation

- PRD: [PRD-2026-11-21-tab-errors.md](../../ft/daemon/1-WIP/PRD-2026-11-21-tab-errors.md)

## Summary

Target tab failures carry the right code (`UNAVAILABLE` / `FAILED_PRECONDITION` / `INTERNAL`), an agent tab probes its agent on the target first, and a target cursor tab's argv is pinned in full.

## Responsibility

- `probe_agent_on_target(shell, worktree, command) -> Result<(), RemoteLaunchError>` in `remote_agent_launch/`: `cd <wt> && exec "${SHELL:-/bin/sh}" -lc 'command -v <cmd>'`, single-quoted; non-zero → `RemoteLaunchError::AgentNotOnTarget { command }`.
- One mapper `tab_launch_status(&RemoteLaunchError) -> Status` beside `RemoteLaunchError`: `Shell` → UNAVAILABLE, vault/account → FAILED_PRECONDITION, `Spawn` → INTERNAL, `AgentNotOnTarget` → FAILED_PRECONDITION naming the command; used by both shell and agent tabs.
- `launch_agent_tab_on_target` probes after `reach_the_worktree`, before spawning (new and restored tabs).
- Tighten `restore_tabs_target_agent_tests.rs` cursor test to the full argv (`--trust`).

## Boundaries

- No probe for shell tabs or for the target **main** start (n4 reuses the helper for starts).
- No change to host tabs or the target main's error mapping.

## Dependencies

None in this stack: n1–n4 of `#session-liveness` are merged on master. This PR's `probe_agent_on_target` is consumed by `target-start-agent` (4/4).

## Draft PR contract

The first push publishes `probe_agent_on_target`, `RemoteLaunchError::AgentNotOnTarget` and `tab_launch_status` as `todo!()`, and the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes
**Concurrent with:** `tab-hygiene` (2/4), `jail-death-watch` (3/4)
**Blocks:** `target-start-agent` (4/4)

Real dependency edges: `n1 → n4` (the probe helper). n2 and n3 have none.

## Prerequisites

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-agent-launch/docs/code-issues/complexity-remote-agent-launch-launch-on-target.md` — not edited.
- `packages/tddy-cli-sessions/docs/code-issues/complexity-pty-spawn-spawn-tool-on.md` — not edited.

### ℹ ANSWERED — [2026-11-16-coder-hosts-never-run-against-a-real-deployment.md](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)

The probe is tested on the stand-in target only; the extra round trip on a real Coder target (about 6–8 s) is accepted.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [x] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

`target_agent_tabs.rs:54` maps every launch error to FAILED_PRECONDITION; `target_tabs.rs` `reach_failure` maps `Shell` → UNAVAILABLE and everything else (incl. `Spawn`) → FAILED_PRECONDITION. No `command -v` probe exists; a missing agent exits 127 in its tab. The cursor tab test checks `argv[..3]` only.

### State B (Target)

One mapper for both kinds; a probe before an agent tab spawns; full cursor argv pinned.

## Acceptance Tests

`packages/tddy-agent-launch/tests/target_agent_tab_acceptance.rs` (extended):
- [x] `an_agent_tab_whose_agent_is_missing_on_the_target_fails_before_its_terminal_starts`
- [x] `an_agent_tab_on_an_unreachable_target_fails_unavailable`

`packages/tddy-session-lifecycle/tests/target_agent_tab_acceptance.rs` (extended):
- [x] `an_agent_tab_on_an_unreachable_target_is_refused_unavailable`
- [x] `an_agent_tab_whose_agent_is_missing_on_the_target_is_refused_naming_it`
- [x] `a_tab_whose_terminal_cannot_spawn_is_refused_internal`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [x] `a_target_agent_tab_whose_agent_is_missing_is_dropped_naming_it_on_resume`

## Unit Tests

- `tab_launch_status`: each `RemoteLaunchError` variant to its code.
- `probe_agent_on_target`: the command string is single-quoted and runs through the login shell.
- `a_target_cursor_tab_resumes_its_chat_by_the_bare_command`: full argv incl. `--trust`.

### Draft PR contract — what the contract commit carries

- `probe_agent_on_target` (`remote_agent_launch/agent_probe.rs`) and `tab_launch_status`
  (`remote_agent_launch/tab_launch_status.rs`) are `todo!()`; `RemoteLaunchError::AgentNotOnTarget`
  and its message are real.
- Both call sites are wired already: `launch_agent_tab_on_target` probes after `reach_the_worktree`,
  and the lifecycle's shell and agent tabs map their launch error through `tab_launch_status`
  (`reach_failure` is gone). So **every** existing target agent tab test, and every target tab error
  test, panics on a `todo!()` until green — expected red, not a regression.
- Test fixtures: the stand-in targets' login `PATH` used to end in this host's `PATH`, so a host with
  `claude` installed made "the agent is missing on the target" untestable.
  `StandInSshTarget::behind_ssh_with_only_the_system_path()` (testing-commons) and the lifecycle fake
  (`target_tab_support/fake_target_shells.rs`: `-lc` `PATH` is its agents dir plus `/usr/bin:/bin`,
  `$SHELL` set on `exec` too) close that gap. The lifecycle fake also gains
  `target_shells_whose_client_this_host_lacks()` — the PTY spawn-failure seam (a nonexistent
  transport client on the interactive argv; ENOENT at spawn, verified on master to reach
  `RemoteLaunchError::Spawn`).

## Decisions & Trade-offs

- **An unreachable target is UNAVAILABLE at every step** (developer decision 2026-11-21), including `Trust(TargetTrustError::Shell)` during a main start's clone pre-trust; `Trust(Malformed)` stays FAILED_PRECONDITION.

- **Probe every agent tab, new and restored** (developer decision): one extra round trip per tab.
- **Spawn failure is INTERNAL**: a local resource problem, not the user's precondition.
- **The mapper keeps every other variant `FAILED_PRECONDITION`** (`Plan`, `Trust`, and the main-only
  startup variants), as today. "Vault/account → FAILED_PRECONDITION" is not a `RemoteLaunchError`:
  the `TabTargetShell` port refuses those with a `Status` before any launch, unchanged.
- **The probe reads only the exit status**: non-zero is `AgentNotOnTarget`. A worktree the target
  lacks is caught before it by `reach_the_worktree` (tabs) or the worktree cut (the 4/4 start).

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
