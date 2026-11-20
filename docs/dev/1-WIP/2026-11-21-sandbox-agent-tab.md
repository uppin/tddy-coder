# Changeset: A sandboxed session's agent tab runs in a jail of its own

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — several crates (agent-launch, daemon-sandbox, session-lifecycle, terminal-rpc, session-web), a shared persistent home between two live jails, liveness counting for jailed agent tabs
**Stack**: `#session-liveness` 7/8 — branch `feature/session-liveness/sandbox-agent-tab`, base `feature/session-liveness/target-shell-tab`

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

## Draft PR contract

The first push publishes the agent variant of `TabJailSpawn` as `todo!()` and the failing tests below.

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
- [ ] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [ ] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

After n5, `sandbox` around an agent is refused; only shell tabs run in tab jails. An agent's jail exec plan is built only by `launch_jailed` for the main.

### State B (Target)

`sandbox` → `agent` starts the agent in its own tab jail with its own conversation; it is live while it runs; Resume restores it into a new jail, resuming its conversation where the harness can.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/sandbox_agent_tab_acceptance.rs` (fake `TabJailSpawn`):
- [ ] `an_agent_tab_opened_in_sandbox_runs_the_agent_in_a_jail_of_its_own`
- [ ] `a_jailed_agent_tab_runs_in_a_conversation_of_its_own`
- [ ] `a_jailed_agent_tab_keeps_the_session_live`
- [ ] `a_jailed_agent_tab_does_not_overwrite_the_mains_session_records`
- [ ] `stopping_the_session_stops_its_jailed_agent_tabs`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [ ] `a_jailed_claude_tab_is_restored_into_a_new_jail_resuming_its_conversation`
- [ ] `a_jailed_opencode_tab_reopens_fresh_and_is_reported_fresh`

`packages/tddy-session-web/cypress/component/NewTerminalMenuSandboxAcceptance.cy.tsx` (extended):
- [ ] `lists agent kinds under In sandbox for a sandboxed session`

## Unit Tests

- The tab-jail agent exec plan: same grants as the main's for the same agent, the tab's own paths.
- `restore_plan`: `[sandbox, agent]` plans a jailed resume or a jailed fresh agent.

## Decisions & Trade-offs

- **Its own conversation per tab**, like host agent tabs (n4's rule).
- **Session records belong to the main** — a tab launch writes none.

## TODO

- [x] Record initial discovery (`2026-11-21-sandbox-agent-tab-initial-discovery.md`)
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
