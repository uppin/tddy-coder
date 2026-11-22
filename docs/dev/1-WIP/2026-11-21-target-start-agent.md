# Changeset: A target session starts without the agent on the daemon host

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — agent-launch and session-lifecycle; start and resume ordering change (probe before the prelude and the worktree cut); a target agent resolver keeping host-resolved deps; green blocked on 1/4's green for every target test (re-scored at the contract: unchanged)
**Stack**: `#tab-hardening` 4/4 — branch `feature/tab-hardening/target-start-agent`, base `feature/tab-hardening/jail-death-watch`
**PR**: #992 https://github.com/uppin/tddy-coder/pull/992

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

### Sequencing on 1/4's green

This branch was cut from 1/4's **contract** (2/4 and 3/4 sit on it), so `probe_agent_on_target` and
`tab_launch_status` are still `todo!()` here. Every n4 test fails first on **n4's own** todo
(`resolve_session_agent_for_target` or `require_the_agent_on_the_target`) — but a target start or
resume calls 1/4's probe right after them, so these tests turn green only once 1/4's green is
cascaded under this branch (`/pr-stack-rebase`), not on n4's green alone:

- all four acceptance tests below (the start, the missing-agent refusal, the resume, the Coder start)
  — the probe **is** the point of the refusal test, and it sits on the real path of the other three
  (the daemon host has no seam for a double, and adding one only for tests is ruled out);
- the three `require_the_agent_on_the_target` unit tests (`target_start_agent_tests.rs`) — they
  pin n4's mapping through 1/4's probe and mapper;
- every **existing** target-start/resume test in `tddy-session-lifecycle` — measured on the
  contract: `remote_target_session_acceptance` (6 of its 10; the four combination refusals stay
  green), `coder_target_session_acceptance` (all 5), `remote_hooks_session_acceptance` (2),
  `orchestrator_target_start_acceptance` (1 of 3), `derived_host_start_acceptance` (1). The contract
  wires the target branch into the start and resume, so they are red on n4's todo now and on 1/4's
  probe after n4's green. Suites outside `tddy-session-lifecycle` that start on a target were not run
  locally; CI lists them.

The five `resolve_session_agent_for_target` unit tests depend on nothing of 1/4: they go green on
n4's own implementation.

## Draft PR contract

The first push publishes, in `packages/tddy-agent-launch/src/target_start_agent.rs`:

- `resolve_session_agent_for_target(agents, kind) -> Result<CliSessionAgent, Status>` — `todo!()`;
- `require_the_agent_on_the_target(shell, session_agent) -> Result<(), Status>` — `todo!()` (1/4's
  probe from the login's own directory `.`, its error mapped by 1/4's `tab_launch_status`);
- `LaunchSessions::agent_where_it_runs(kind, target)` — the dispatch (host resolve, or the two above),
  wired into `start_cli_harness` (after `refuse_a_target_combination`, before the prelude and any cut)
  and `resume_cli_harness` (in place of `resolve_session_agent`);
- `LaunchSessions::start_target_shell(req)` — the start's target shell, resolved before the prelude;

and the failing tests below. `CliSessionAgent` keeps its shape: a target agent's `tool.binary` is the
spec's bare command (documented on the struct).

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
- `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness.md` — the contract swaps its one `resolve_session_agent` statement for `agent_where_it_runs`, which rustfmt lays over three lines (+2); `/green` recovers the two lines or records them at wrap.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [x] TDD Red — failing unit/integration tests
- [x] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

`start_cli_harness` calls `resolve_session_agent` (host `resolve`) first (cli_branch_starts.rs:120); the target shell is resolved after the worktree cut.

### State B (Target)

As Responsibility.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/remote_target_session_acceptance.rs` (extended):
- [x] `a_target_session_starts_when_the_daemon_host_has_no_agent`
- [x] `a_target_whose_agent_is_missing_fails_the_start_before_any_worktree_is_cut`
- [x] `a_target_session_resumes_without_the_agent_on_the_daemon_host`

`packages/tddy-session-lifecycle/tests/coder_target_session_acceptance.rs` (extended):
- [x] `a_coder_target_session_starts_without_the_agent_on_the_daemon_host`

## Unit Tests

`packages/tddy-agent-launch/src/target_start_agent_tests.rs`:

- [x] `resolve_session_agent_for_target`: `a_target_agent_is_its_specs_bare_command_on_a_host_without_the_agent`,
  `a_target_agent_is_never_this_hosts_copy_of_the_agent`, `a_target_agents_tddy_tools_is_the_one_on_this_host`,
  `a_target_agent_on_a_host_without_tddy_tools_is_refused_naming_it`,
  `a_target_cursor_agent_runs_the_command_its_spec_names_not_its_id`
- [x] `require_the_agent_on_the_target` (blocked on 1/4's green, see Dependencies):
  `a_target_whose_login_shell_finds_the_agent_admits_it_with_one_probe`,
  `a_target_without_the_agent_refuses_failed_precondition_naming_the_command`,
  `an_unreachable_target_refuses_unavailable_with_the_transports_reason`

## Decisions & Trade-offs

- **Resolve on the target, probe first** (developer decision), like target agent tabs.
- **The probe runs from the target login's own directory (`.`)**, at start and resume alike: a start
  has no worktree on the target before its cut, and a resume whose worktree is gone must be reported
  by its launch (as today), not misread as a missing agent by a failed `cd`.
- **`CliSessionAgent` is not split into a host and a target type**: a target agent's `tool.binary` is
  the bare command, which every target path already treats it as (`launch_on_target` swaps it in,
  the conversation mint uses `tool.command`, the node launcher is skipped on a target). A separate
  type would touch every reader of `agent.tool.binary` for no behaviour change.
- **`refuse_a_target_combination` moves ahead of the agent's resolution** in `start_cli_harness`, so
  a conflicting request is refused before the target is asked anything.
- **The start resolves its target shell twice** (`start_target_shell` before the prelude, and again
  inside `start_cli_harness_session`): passing it down would grow `start_cli_harness_from_request`
  and `start_cli_harness_session`, both open complexity issues. The resolution makes no remote call.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
