# Changeset: A target session starts without the agent on the daemon host

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — agent-launch and session-lifecycle, start ordering change (probe before the worktree cut), host-resolved agent reshaped for targets
**Stack**: `#tab-hardening` 4/4 — branch `feature/tab-hardening/target-start-agent`, base `feature/tab-hardening/jail-death-watch`

## Initial Discovery

[2026-11-21-target-start-agent-initial-discovery.md](./2026-11-21-target-start-agent-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: `resolve_session_agent_for_target` (spec + bare command, no host resolve), the target start and resume use it and probe first
- **tddy-session-lifecycle**: start/resume acceptance tests

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-start-agent.md](../../ft/daemon/1-WIP/PRD-2026-11-21-target-start-agent.md)

## Summary

A target start and resume resolve the agent on the target (bare command, chat minted there) and probe it before the worktree is cut.

## Responsibility

- `resolve_session_agent_for_target(agents, kind)`: the spec and its bare command from the registry (as `target_agent_tab` does); `CliSessionAgent` reshaped so a target start carries no host binary.
- `start_cli_harness` (and `start_spawned_cli_child`, the claude-cli start) for a request with a target: resolve the target shell before the prelude, `probe_agent_on_target` (from 1/4), then cut the worktree.
- Target resume: the same resolution and probe.
- Tools the daemon runs itself (`tddy-tools` closure) stay host-resolved.

## Boundaries

- Does not change `probe_agent_on_target` (1/4 owns it) or `launch_on_target`'s startup logic.
- No change to host or sandboxed starts.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `tab-errors` (1/4) | `probe_agent_on_target`, `RemoteLaunchError::AgentNotOnTarget`, `tab_launch_status` | calls the probe before the worktree cut and maps its error | change the probe or the mapper |
| `jail-death-watch` (3/4), `tab-hygiene` (2/4) | nothing consumed — the line puts them below | — | — |

## Draft PR contract

The first push publishes `resolve_session_agent_for_target` as `todo!()` and the failing tests below.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** no — it calls 1/4's probe
**Concurrent with:** none
**Blocks:** none

Real dependency edges: `n1 → n4` (the probe helper). n2 and n3 have none.

## Prerequisites

### ✅ RESOLVED HERE — [2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md](../todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md)

The session-start half (the tab half closed in `#session-liveness` 8/8): delete the entry at wrap.

### ⚠ DURING — [2026-11-19-coder-remote-agent-local-paths.md](../todo/2026-11-19-coder-remote-agent-local-paths.md)

No host path reaches the target.

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-agent-launch/docs/code-issues/complexity-cli-branch-starts-start-cli-harness-from-request.md` — the target branch is a new helper.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [ ] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

`start_cli_harness` calls `resolve_session_agent` (host `resolve`) first (cli_branch_starts.rs:120); the target shell is resolved after the worktree cut.

### State B (Target)

As Responsibility.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/remote_target_session_acceptance.rs` (extended):
- [ ] `a_target_session_starts_when_the_daemon_host_has_no_agent`
- [ ] `a_target_whose_agent_is_missing_fails_the_start_before_any_worktree_is_cut`
- [ ] `a_target_session_resumes_without_the_agent_on_the_daemon_host`

`packages/tddy-session-lifecycle/tests/coder_target_session_acceptance.rs` (extended):
- [ ] `a_coder_target_session_starts_without_the_agent_on_the_daemon_host`

## Unit Tests

- `resolve_session_agent_for_target`: spec + bare command; never calls `resolve`.

## Decisions & Trade-offs

- **Resolve on the target, probe first** (developer decision), like target agent tabs.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
