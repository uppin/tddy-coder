# Changeset: A sandboxed session's agent tab runs in a jail of its own

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — several crates (agent-launch, daemon-sandbox, session-lifecycle, terminal-rpc, session-web), a shared persistent home between two live jails, liveness counting for jailed agent tabs
**Stack**: `#session-liveness` 7/8 — branch `feature/session-liveness/sandbox-agent-tab`, base `feature/session-liveness/target-shell-tab`
**PR**: #966 https://github.com/uppin/tddy-coder/pull/966

## Initial Discovery

[2026-11-21-sandbox-agent-tab-initial-discovery.md](./2026-11-21-sandbox-agent-tab-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: the tab-jail agent launch (the agent's exec plan and grants under the tab's jail paths, a fresh conversation); `[sandbox, agent]` restore
- **tddy-daemon-sandbox**: `TabJails` counts a jailed agent tab as live
- **tddy-terminal-rpc**: `sandbox` around an agent accepted
- **tddy-session-lifecycle**: roster start of a jailed agent tab
- **tddy-session-web**: agent kinds listed under "In sandbox"

## Related Feature Documentation

- PRD: [PRD-2026-11-21-sandbox-agent-tab.md](../../ft/daemon/1-WIP/PRD-2026-11-21-sandbox-agent-tab.md)

## Summary

An agent tab opened "In sandbox" runs the agent in a jail of its own, in its own conversation, keeps the session live, and is restored into a new jail on Resume, resuming its conversation where the harness can.

## Responsibility

- A `TabJailSpawn` agent variant: the agent's exec plan built as the main's jail builds it (`exec_plan`, the agent's `ToolDef` grants through `lower_tool_jail`), under n5's per-tab paths, with a conversation from `new_agent_tab`'s rule.
- The main's session records (`record_role_of_session_type_before_spawn`, `record_usage_context`) are written only by the main's launch; a tab-jail launch writes none.
- A persistent agent home shared with the main jail: used as is, no second credential bootstrap.
- `TabJails::live` / `HeldSessions::live_sessions` count a jailed agent tab, never a jailed shell.
- Recording `[sandbox, agent]` (harness, conversation, resumable); restore resumes it where the harness can, otherwise reopens fresh and says so.
- `sandbox` around an agent accepted on the wire; the menu lists agents under "In sandbox".

## Boundaries

- Does not change n5's registry, paths, teardown or menu group — only adds the agent variant.
- No target agent tabs (n8).
- No change to the main jail's launch.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n5` sandbox-shell-tab | `TabJails`, `TabJailSpawn`, per-tab jail paths, teardown, store routing, `SANDBOX_KIND`, "In sandbox" group, `SessionEntry.sandbox` | adds the agent variant of the spawn, the live count, and the agent restore | change the registry, the paths or the teardown points |
| `n4` resume-roster (#961) | `new_agent_tab`, `agent_launch_node`, `tab_conversation_is_resumable`, `restore_plan` | jailed agent tabs mint and record their conversation through the same rule | change the agent-tab rule |
| `n1` agent-tab-liveness (#958) | liveness from held agent tabs | a jailed agent tab keeps the session live | change the rule |
| `n6` target-shell-tab | nothing consumed — the line puts it below | — | — |

**Sequencing of this node's tests on n5's green** (pass 2, 2026-11-21). Each n7 path sits *ahead* of
n5's `todo!()` instead of inside it (`from_wrapped_request` → `sandboxed_agent`, `restore_plan` →
`jailed_agent_plan`), so the parsing, restore-rule, `TabJails::start_agent` / `live_agent_sessions`
and `TabJailAgentPlanner` unit tests fail on n7's own todo. These pass only once n5 is green too,
because they run through n5's real objects:

- `tab_jails_agent_tests::a_jailed_shell_tab_never_makes_its_session_live` (n5's `TabJails::start`)
  and `a_stopped_agent_tab_no_longer_makes_its_session_live` (n5's `TabJails::stop`).
- `sandbox_agent_tab_acceptance::stopping_the_session_stops_its_jailed_agent_tabs` (n5's
  `HeldSessions::stop_session` teardown of tab jails), and every acceptance test that lists or
  streams a jailed tab (n5's store routing, `sandboxed_session_tab`).
- `resume_tab_roster_acceptance::a_jailed_*_tab_*` (n5's `TabJailPaths::for_tab` and restore wiring).
- Cypress `lists agent kinds under In sandbox for a sandboxed session` (n5's "In sandbox" group).

n7 inverts three n5 tests and deletes them:
`terminal_kind_sandbox_wrapper_unit::an_agent_wrapped_in_sandbox_is_refused_naming_the_agent`,
`restore_tabs_sandbox_tests::an_agent_recorded_in_a_sandbox_is_dropped_naming_the_agent`, and
`sandbox_shell_tab_acceptance::a_sandbox_wrapper_around_an_agent_is_refused`.

## Draft PR contract

The first push publishes the agent variant of `TabJailSpawn` as `todo!()` and the failing tests below.

Published surface (bodies `todo!()`, `TODO(sandbox-agent-tab)`, unless marked):

- tddy-daemon-sandbox `tab_jails.rs`: `TabJailAgent { harness, conversation_id, resume }`; `TabJailSpawn::spawn_agent` (a required method beside n5's `spawn_shell`); `TabJails::start_agent`, `TabJails::live_agent_sessions`.
- tddy-terminal-rpc `service/ports.rs`: `TerminalKind::SandboxedAgent { agent }`; `sandboxed_agent`, reached from `from_wrapped_request` ahead of n5's `sandboxed`.
- tddy-agent-launch: `tab_jail_agent.rs` — `TabJailAgentPlan`, `TabJailAgentPlanner::plan` (the jail's grants and exec plan, writing no session record — the seam the main-only-records rule is tested at); `LaunchSessions::spawn_agent`; `TabRestore::JailedAgent { harness, resume_conversation }`, `jailed_agent_plan` (dispatched ahead of n5's `sandbox_plan`), `restore_jailed_agent`.
- tddy-session-lifecycle: `start_jailed_agent_tab` (`jailed_tabs.rs`), routed from `TerminalRoster::start`.
- tddy-coder participant: the `SandboxedAgent` refusal arm (`FAILED_PRECONDITION`, implemented).
- tddy-session-web: `TODO(sandbox-agent-tab)` on `NewTerminalMenu`'s `sandboxed` prop.
- Left to green: the `DaemonTerminalSessionStore::live_sessions` union with `live_agent_sessions` (a `todo!()` there would panic every listing) and the `wrappers` proto comment (regenerates TS).

## Green wave

**Wave:** 5 of 5
**Greenable independently:** no — it builds on n5's tab jails
**Concurrent with:** `target-agent-tab` (n8)
**Blocks:** none

Real dependency edges: `n4 → n5, n6      n5 → n7      n6 → n8`

## Prerequisites

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-agent-launch/docs/code-issues/complexity-jail-launch-launch-jailed.md` (151 lines) — share extracted pieces with the tab launch; `launch_jailed` must not grow.
- `packages/tddy-daemon-sandbox/docs/code-issues/complexity-tool-jail-lower-tool-jail.md` — called as is.

### ⚠ DURING — [2026-11-02-jailed-claude-loads-project-mcp-json.md](../todo/2026-11-02-jailed-claude-loads-project-mcp-json.md)

A jailed agent tab inherits the main jail's behaviour here; it must not make it worse. Recorded so it is considered, not fixed here.

### ⚠ DURING — [2026-10-24-claude-agent-tab-shares-session-hooks.md](../todo/2026-10-24-claude-agent-tab-shares-session-hooks.md)

A jailed claude tab shares the session's hooks like a host agent tab; unchanged here.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW — written and failing; review approved
- [x] TDD Red — failing unit/integration tests
- [x] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

After n5, `sandbox` around an agent is refused; only shell tabs run in tab jails. An agent's jail exec plan is built only by `launch_jailed` for the main.

### State B (Target)

`sandbox` → `agent` starts the agent in its own tab jail with its own conversation; it is live while it runs; Resume restores it into a new jail, resuming its conversation where the harness can.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/sandbox_agent_tab_acceptance.rs` (fake `TabJailSpawn`):
- [x] `an_agent_tab_opened_in_sandbox_runs_the_agent_in_a_jail_of_its_own`
- [x] `a_jailed_agent_tab_runs_in_a_conversation_of_its_own`
- [x] `a_jailed_agent_tab_keeps_the_session_live`
- [x] `a_jailed_agent_tab_does_not_overwrite_the_mains_session_records`
- [x] `stopping_the_session_stops_its_jailed_agent_tabs`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [x] `a_jailed_claude_tab_is_restored_into_a_new_jail_resuming_its_conversation`
- [x] `a_jailed_opencode_tab_reopens_fresh_and_is_reported_fresh`

`packages/tddy-session-web/cypress/component/NewTerminalMenuSandboxAcceptance.cy.tsx` (extended):
- [x] `lists agent kinds under In sandbox for a sandboxed session`

## Unit Tests

- [x] The tab-jail agent exec plan: same grants as the main's for the same agent, the tab's own paths — `tddy-agent-launch/src/tab_jail_agent_tests.rs` (7).
- [x] `restore_plan`: `[sandbox, agent]` plans a jailed resume or a jailed fresh agent — `tddy-agent-launch/src/opencode_cli_spawn/restore_tabs_sandbox_agent_tests.rs` (5).
- [x] `TabJails::start_agent` / `live_agent_sessions` — `tddy-daemon-sandbox/src/tab_jails_agent_tests.rs` (9; 2 blocked on n5's `start`/`stop`).
- [x] `sandbox` around an agent parses to `SandboxedAgent` — `tddy-terminal-rpc/tests/terminal_kind_sandbox_agent_unit.rs` (4).

## Refactoring Needed

### From @red (TDD Red Phase)

- `tab_jails_agent_tests.rs` and n5's `tab_jails_tests.rs` each carry a `sleep`-runner stand-in `TabJailSpawn`; share one test helper once both are green.
- `tab_jail_agent_tests.rs`'s `AHostOfStubs` duplicates `tddy-session-lifecycle::test_util::AHostOfBinaries`; a shared fixture host belongs in `tddy-testing-commons` (or `tddy-agent-launch` test support).
- The `has_a_main_usage_context` / `session_records` helpers exist in both `sandbox_tab_support` and `tab_jail_agent_tests.rs`.

## Decisions & Trade-offs

- **Its own conversation per tab**, like host agent tabs (n4's rule).
- **Session records belong to the main** — a tab launch writes none.

## TODO

- [x] Record initial discovery (`2026-11-21-sandbox-agent-tab-initial-discovery.md`)
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
